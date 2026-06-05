use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use deadpool_redis::redis::Script;
use deadpool_redis::{Config as DeadpoolConfig, Connection, Pool, Runtime};
use redis::AsyncCommands;

use crate::{
    messages::BaseMessage,
    thread::{ThreadId, ThreadMeta, ThreadStore},
};

// ── Lua 脚本 ──────────────────────────────────────────────────────────────────

/// append_messages 原子脚本：去重 + 追加 + 更新计数
/// KEYS[1] = msg_ids set, KEYS[2] = messages sorted set, KEYS[3] = msg_data hash
/// KEYS[4] = thread hash
/// ARGV[1] = updated_at, ARGV[2..] = pairs of (message_id, json_content)
/// Returns: 实际新增消息数
const APPEND_MESSAGES_LUA: &str = r#"
local added = 0
-- 获取当前消息数作为起始 score
local count = tonumber(redis.call('HGET', KEYS[4], 'message_count') or "0")
-- 遍历 ARGV，第 1 个是 updated_at，之后每 2 个一组: msg_id, json
local updated_at = ARGV[1]
local score = count
for i = 2, #ARGV, 2 do
    local mid = ARGV[i]
    local json = ARGV[i+1]
    -- 幂等检查
    if redis.call('SISMEMBER', KEYS[1], mid) == 0 then
        score = score + 1
        redis.call('ZADD', KEYS[2], score, mid)
        redis.call('HSET', KEYS[3], mid, json)
        redis.call('SADD', KEYS[1], mid)
        added = added + 1
    end
end
if added > 0 then
    redis.call('HINCRBY', KEYS[4], 'message_count', added)
    redis.call('HSET', KEYS[4], 'updated_at', updated_at)
end
return added
"#;

/// delete_thread 级联脚本：删除自身所有 key + 从父集合移除
/// KEYS[1] = thread, KEYS[2] = messages, KEYS[3] = msg_data, KEYS[4] = msg_ids
/// KEYS[5] = cached_context, KEYS[6] = config, KEYS[7] = children
/// KEYS[8] = threads:all, KEYS[9] = threads:by_updated
/// ARGV[1] = thread_id, ARGV[2] = parent_id (or "")
const DELETE_THREAD_LUA: &str = r#"
-- 删除自身 key
redis.call('DEL', KEYS[1], KEYS[2], KEYS[3], KEYS[4], KEYS[5], KEYS[6], KEYS[7])
-- 从全局集合移除
redis.call('SREM', KEYS[8], ARGV[1])
redis.call('ZREM', KEYS[9], ARGV[1])
-- 从父集合移除
if ARGV[2] ~= '' then
    redis.call('SREM', 'children:' .. ARGV[2], ARGV[1])
end
return 1
"#;

/// delete_messages 原子脚本：按 ID 批量删除消息 + 更新计数
/// KEYS[1] = messages sorted set, KEYS[2] = msg_data hash, KEYS[3] = msg_ids set, KEYS[4] = thread hash
/// ARGV[1..] = message_ids to delete
/// Returns: 实际删除数
const DELETE_MESSAGES_LUA: &str = r#"
local deleted = 0
for i = 1, #ARGV do
    local mid = ARGV[i]
    local removed = redis.call('ZREM', KEYS[1], mid)
    if removed > 0 then
        redis.call('HDEL', KEYS[2], mid)
        redis.call('SREM', KEYS[3], mid)
        deleted = deleted + 1
    end
end
if deleted > 0 then
    redis.call('HINCRBY', KEYS[4], 'message_count', -deleted)
end
return deleted
"#;

// ── Key 构造 ──────────────────────────────────────────────────────────────────

fn key_thread(id: &str) -> String {
    format!("thread:{id}")
}
fn key_thread_config(id: &str) -> String {
    format!("thread:{id}:config")
}
fn key_thread_cached(id: &str) -> String {
    format!("thread:{id}:cached_context")
}
fn key_messages(tid: &str) -> String {
    format!("messages:{tid}")
}
fn key_msg_data(tid: &str) -> String {
    format!("msg_data:{tid}")
}
fn key_msg_ids(tid: &str) -> String {
    format!("msg_ids:{tid}")
}
fn key_children(parent_id: &str) -> String {
    format!("children:{parent_id}")
}

// msg_data_key 别名（Lua 脚本中使用 key_msg_data，这里统一）
fn msg_data_key(tid: &str) -> String {
    key_msg_data(tid)
}

// ── gzip 辅助 ─────────────────────────────────────────────────────────────────

const GZIP_THRESHOLD: usize = 4096;

fn maybe_compress(data: &str) -> Vec<u8> {
    if data.len() > GZIP_THRESHOLD {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(data.as_bytes()).ok();
        encoder.finish().unwrap_or_else(|_| data.as_bytes().to_vec())
    } else {
        data.as_bytes().to_vec()
    }
}

fn maybe_decompress(data: &[u8]) -> Result<String> {
    // 尝试 gzip 解压，失败则按明文处理
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        use std::io::Read;
        let mut decoder = flate2::read::GzDecoder::new(data);
        let mut s = String::new();
        decoder.read_to_string(&mut s)?;
        Ok(s)
    } else {
        Ok(String::from_utf8(data.to_vec())?)
    }
}

// ── RedisThreadStore ──────────────────────────────────────────────────────────

/// 基于 Redis 的 ThreadStore 实现
///
/// 数据模型：
/// - Hash `thread:{id}` — 元数据（不含 config/cached_context）
/// - String `thread:{id}:config` — 配置快照（大字段单独存）
/// - String `thread:{id}:cached_context` — 物化缓存（超 4KB gzip）
/// - Sorted Set `threads:by_updated` — score=updated_at, member=id
/// - Set `threads:all` — 全部 thread id
/// - Sorted Set `messages:{tid}` — score=序号, member=message_id
/// - Hash `msg_data:{tid}` — field=message_id, value=JSON
/// - Set `msg_ids:{tid}` — 幂等去重
/// - Set `children:{parent_id}` — 子线程集合
pub struct RedisThreadStore {
    pool: Pool,
}

impl RedisThreadStore {
    /// 使用 Redis URL 创建（含连接测试）
    pub async fn new(redis_url: &str) -> Result<Self> {
        let cfg = DeadpoolConfig::from_url(redis_url);
        let pool = cfg
            .builder()
            .map_err(|e| anyhow::anyhow!("Redis 配置错误: {e}"))?
            .max_size(10)
            .runtime(Runtime::Tokio1)
            .build()
            .map_err(|e| anyhow::anyhow!("Redis 连接池创建失败: {e}"))?;

        // 启动时 PING 测试
        let mut conn = pool
            .get()
            .await
            .context("Redis 连接获取失败，请检查 REDIS_URL")?;
        let _: String = redis::cmd("PING")
            .query_async(&mut *conn)
            .await
            .context("Redis PING 失败，请检查 REDIS_URL")?;

        Ok(Self { pool })
    }

    async fn conn(&self) -> Result<Connection> {
        self.pool.get().await.context("Redis 连接获取失败")
    }

    /// HSET 多个字段
    async fn hset_fields(&self, conn: &mut Connection, key: &str, fields: &[(String, String)]) -> Result<()> {
        let mut cmd = redis::cmd("HSET");
        cmd.arg(key);
        for (f, v) in fields {
            cmd.arg(f).arg(v);
        }
        cmd.query_async::<_, ()>(conn).await?;
        Ok(())
    }

    /// HGETALL 返回 HashMap
    async fn hgetall(&self, conn: &mut Connection, key: &str) -> Result<std::collections::HashMap<String, String>> {
        let hash: std::collections::HashMap<String, String> = redis::cmd("HGETALL")
            .arg(key)
            .query_async(conn)
            .await?;
        Ok(hash)
    }
}

// ── ThreadMeta ↔ Redis Hash 转换 ─────────────────────────────────────────────

fn meta_to_hash(meta: &ThreadMeta) -> Vec<(String, String)> {
    vec![
        ("id".into(), meta.id.clone()),
        ("title".into(), meta.title.clone().unwrap_or_default()),
        ("cwd".into(), meta.cwd.clone()),
        ("created_at".into(), meta.created_at.to_rfc3339()),
        ("updated_at".into(), meta.updated_at.to_rfc3339()),
        ("message_count".into(), meta.message_count.to_string()),
        ("content_size".into(), meta.content_size.to_string()),
        (
            "parent_thread_id".into(),
            meta.parent_thread_id.clone().unwrap_or_default(),
        ),
        (
            "snapshot_at_message_id".into(),
            meta.snapshot_at_message_id.clone().unwrap_or_default(),
        ),
        ("hidden".into(), if meta.hidden { "1" } else { "0" }.to_string()),
        ("cancel_policy".into(), meta.cancel_policy.clone()),
        ("agent_status".into(), meta.agent_status.clone()),
    ]
}

fn hash_to_meta(
    hash: &std::collections::HashMap<String, String>,
    config: Option<String>,
    cached_context: Option<String>,
) -> Result<ThreadMeta> {
    let get = |k: &str| hash.get(k).cloned().unwrap_or_default();
    let get_opt = |k: &str| hash.get(k).filter(|v| !v.is_empty()).cloned();

    Ok(ThreadMeta {
        id: get("id"),
        title: get_opt("title"),
        cwd: get("cwd"),
        created_at: get("created_at").parse::<DateTime<Utc>>()?,
        updated_at: get("updated_at").parse::<DateTime<Utc>>()?,
        message_count: get("message_count").parse().unwrap_or(0),
        content_size: get("content_size").parse().unwrap_or(0),
        parent_thread_id: get_opt("parent_thread_id"),
        snapshot_at_message_id: get_opt("snapshot_at_message_id"),
        hidden: get("hidden") == "1",
        cancel_policy: get("cancel_policy"),
        config,
        cached_context,
        agent_status: get("agent_status"),
    })
}

/// 从消息列表中提取标题（取第一条 Human 消息的前 50 字符）
fn extract_title(msgs: &[BaseMessage]) -> Option<String> {
    use crate::messages::{ContentBlock, MessageContent};
    for msg in msgs {
        if let BaseMessage::Human { content, .. } = msg {
            let text = match content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::Text { text } = b {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
                MessageContent::Raw(_) => continue,
            };
            let title: String = text.chars().take(50).collect();
            if !title.is_empty() {
                return Some(title);
            }
        }
    }
    None
}

// ── ThreadStore impl ──────────────────────────────────────────────────────────

#[async_trait]
impl ThreadStore for RedisThreadStore {
    async fn create_thread(&self, meta: ThreadMeta) -> Result<ThreadId> {
        let id = meta.id.clone();
        let mut conn = self.conn().await?;

        let now_ts = meta.updated_at.timestamp_millis() as f64;

        // 1. HSET thread:{id} 元数据
        let hash_fields = meta_to_hash(&meta);
        self.hset_fields(&mut conn, &key_thread(&id), &hash_fields).await?;

        // 2. 单独存 config（大字段）
        if let Some(ref config) = meta.config {
            let _: () = conn.set(key_thread_config(&id), config.as_str()).await?;
        }

        // 3. ZADD threads:by_updated + SADD threads:all
        let _: () = conn.zadd("threads:by_updated", &id, now_ts).await?;
        let _: () = conn.sadd("threads:all", &id).await?;

        // 4. 父子关系
        if let Some(ref parent_id) = meta.parent_thread_id {
            let _: () = conn.sadd(key_children(parent_id), &id).await?;
        }

        Ok(id)
    }

    async fn append_messages(&self, id: &ThreadId, msgs: &[BaseMessage]) -> Result<()> {
        if msgs.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn().await?;

        // 构建 Lua 脚本参数: [updated_at, msg_id1, json1, msg_id2, json2, ...]
        let mut args: Vec<String> = Vec::with_capacity(msgs.len() * 2 + 1);
        args.push(Utc::now().to_rfc3339());
        for msg in msgs {
            let mid = msg.id().as_uuid().to_string();
            let json = serde_json::to_string(msg)?;
            args.push(mid);
            args.push(json);
        }

        let result: i64 = Script::new(APPEND_MESSAGES_LUA)
            .key(key_msg_ids(id))
            .key(key_messages(id))
            .key(msg_data_key(id))
            .key(key_thread(id))
            .arg(&args)
            .invoke_async(&mut *conn)
            .await?;

        // 如果有新增消息，检查是否需要更新标题
        if result > 0 {
            if let Some(title) = extract_title(msgs) {
                let current: Option<String> = conn.hget(key_thread(id), "title").await?;
                if current.as_deref().unwrap_or("").is_empty() {
                    let _: () = conn.hset(key_thread(id), "title", &title).await?;
                }
            }
        }

        Ok(())
    }

    async fn load_messages(&self, id: &ThreadId) -> Result<Vec<BaseMessage>> {
        let mut conn = self.conn().await?;

        // ZRANGE 取全部 message_id（按 score 排序 = 插入顺序）
        let msg_ids: Vec<String> = conn.zrange(key_messages(id), 0, -1).await?;
        if msg_ids.is_empty() {
            return Ok(vec![]);
        }

        // pipeline HGET msg_data:{tid} {msg_id}
        let jsons: Vec<Option<String>> = conn
            .hget(msg_data_key(id), &msg_ids)
            .await?;

        let mut msgs = Vec::with_capacity(jsons.len());
        for json in jsons.into_iter().flatten() {
            msgs.push(serde_json::from_str(&json)?);
        }
        Ok(msgs)
    }

    async fn load_meta(&self, id: &ThreadId) -> Result<ThreadMeta> {
        let mut conn = self.conn().await?;

        let hash = self.hgetall(&mut conn, &key_thread(id)).await?;

        if hash.is_empty() {
            anyhow::bail!("thread {id} 不存在");
        }

        // 加载 config 和 cached_context（大字段单独存）
        let config: Option<String> = conn.get(key_thread_config(id)).await?;
        let raw_cached: Option<Vec<u8>> = conn.get(key_thread_cached(id)).await?;
        let cached_context = match raw_cached {
            Some(data) => Some(maybe_decompress(&data)?),
            None => None,
        };

        hash_to_meta(&hash, config, cached_context)
    }

    async fn update_meta(&self, id: &ThreadId, meta: ThreadMeta) -> Result<()> {
        let mut conn = self.conn().await?;

        // 更新 Hash 字段
        let hash_fields = meta_to_hash(&meta);
        self.hset_fields(&mut conn, &key_thread(id), &hash_fields).await?;

        // 更新 config（覆盖写入）
        if let Some(ref config) = meta.config {
            let _: () = conn.set(key_thread_config(id), config.as_str()).await?;
        } else {
            let _: () = conn.del(key_thread_config(id)).await?;
        }

        // 更新 cached_context
        if let Some(ref cached) = meta.cached_context {
            let compressed = maybe_compress(cached);
            let _: () = conn.set(key_thread_cached(id), compressed).await?;
        } else {
            let _: () = conn.del(key_thread_cached(id)).await?;
        }

        // 更新 sorted set score
        let now_ts = meta.updated_at.timestamp_millis() as f64;
        let _: () = conn.zadd("threads:by_updated", id, now_ts).await?;

        Ok(())
    }

    async fn list_threads(&self) -> Result<Vec<ThreadMeta>> {
        let mut conn = self.conn().await?;

        // ZREVRANGEBYSCORE 按 updated_at 降序
        let ids: Vec<String> = conn
            .zrevrangebyscore("threads:by_updated", "+inf", "-inf")
            .await?;

        if ids.is_empty() {
            return Ok(vec![]);
        }

        // pipeline HGETALL 每个 thread
        let mut metas = Vec::with_capacity(ids.len());
        for id in &ids {
            let hash = self.hgetall(&mut conn, &key_thread(id)).await?;

            if hash.is_empty() {
                continue;
            }

            // list_threads 不返回 cached_context（对齐 SQLite THREAD_META_COLUMNS 优化）
            let meta = hash_to_meta(&hash, None, None)?;

            // 应用层过滤 hidden（对齐 SQLite WHERE hidden = 0）
            if meta.hidden {
                continue;
            }

            // 计算 content_size：用 messages sorted set 的 ZCARD * 估算
            // 精确值需要查询，这里用 Hash 中存储的值
            metas.push(meta);
        }

        Ok(metas)
    }

    async fn delete_thread(&self, id: &ThreadId) -> Result<()> {
        let mut conn = self.conn().await?;

        // 递归删除子线程
        self.delete_thread_recursive(&mut conn, id).await
    }

    async fn update_title(&self, id: &ThreadId, title: &str) -> Result<()> {
        let mut conn = self.conn().await?;

        let now = Utc::now().to_rfc3339();
        let _: () = conn.hset(key_thread(id), "title", title).await?;
        let _: () = conn.hset(key_thread(id), "updated_at", &now).await?;

        let now_ts = Utc::now().timestamp_millis() as f64;
        let _: () = conn.zadd("threads:by_updated", id, now_ts).await?;

        Ok(())
    }

    async fn load_context(&self, thread_id: &ThreadId) -> Result<Vec<BaseMessage>> {
        let mut conn = self.conn().await?;

        // 1. 尝试从 cached_context 读取
        let raw_cached: Option<Vec<u8>> = conn.get(key_thread_cached(thread_id)).await?;

        if let Some(data) = raw_cached {
            let cached_str = maybe_decompress(&data)?;
            let mut cached_msgs: Vec<BaseMessage> = serde_json::from_str(&cached_str)?;
            let cached_count = cached_msgs.len();

            // 2. 检查是否有新消息
            let total: i64 = conn.hget(key_thread(thread_id), "message_count").await?;
            if total as usize <= cached_count {
                return Ok(cached_msgs);
            }

            // 3. 增量获取新消息
            let new_ids: Vec<String> = conn
                .zrange(key_messages(thread_id), cached_count as isize, -1)
                .await?;

            if !new_ids.is_empty() {
                let jsons: Vec<Option<String>> = conn
                    .hget(msg_data_key(thread_id), &new_ids)
                    .await?;
                for json in jsons.into_iter().flatten() {
                    cached_msgs.push(serde_json::from_str(&json)?);
                }
            }

            // 4. 更新缓存
            let new_cached = serde_json::to_string(&cached_msgs)?;
            let compressed = maybe_compress(&new_cached);
            let _: () = conn.set(key_thread_cached(thread_id), compressed).await?;

            return Ok(cached_msgs);
        }

        // 缓存未命中：解析祖先链
        let chain = self.resolve_ancestor_chain(&mut conn, thread_id).await?;
        let mut all_msgs = Vec::new();

        for (i, tid) in chain.iter().enumerate() {
            let is_last = i == chain.len() - 1;

            if is_last {
                // 自身线程：加载全部消息
                let msgs = self.load_messages_from_conn(&mut conn, tid).await?;
                all_msgs.extend(msgs);
            } else {
                // 祖先线程：snapshot_at_message_id 存在下一个（子）线程上
                let next_tid = &chain[i + 1];
                let child_hash = self.hgetall(&mut conn, &key_thread(next_tid)).await?;

                if let Some(snap_id) =
                    child_hash.get("snapshot_at_message_id").filter(|v| !v.is_empty())
                {
                    let msgs = self
                        .load_messages_up_to(&mut conn, tid, snap_id)
                        .await?;
                    all_msgs.extend(msgs);
                }
            }
        }

        // 保存缓存
        if !all_msgs.is_empty() {
            let cached = serde_json::to_string(&all_msgs)?;
            let compressed = maybe_compress(&cached);
            let _: () = conn.set(key_thread_cached(thread_id), compressed).await?;
        }

        Ok(all_msgs)
    }

    async fn list_child_threads(&self, parent_id: &ThreadId) -> Result<Vec<ThreadMeta>> {
        let mut conn = self.conn().await?;

        let child_ids: Vec<String> = conn.smembers(key_children(parent_id)).await?;
        if child_ids.is_empty() {
            return Ok(vec![]);
        }

        let mut metas = Vec::with_capacity(child_ids.len());
        for cid in &child_ids {
            match self.load_meta_from_conn(&mut conn, cid).await {
                Ok(meta) => metas.push(meta),
                Err(_) => continue, // 孤儿记录跳过
            }
        }

        // 按 created_at 排序
        metas.sort_by_key(|m| m.created_at);
        Ok(metas)
    }

    async fn list_session_threads(&self, root_id: &ThreadId) -> Result<Vec<ThreadMeta>> {
        let mut conn = self.conn().await?;

        // BFS 展开子树
        let mut all_ids = vec![root_id.clone()];
        let mut queue = vec![root_id.clone()];
        let mut visited = std::collections::HashSet::new();
        visited.insert(root_id.clone());

        while let Some(pid) = queue.pop() {
            let child_ids: Vec<String> = conn.smembers(key_children(&pid)).await?;
            for cid in child_ids {
                if visited.insert(cid.clone()) {
                    all_ids.push(cid.clone());
                    queue.push(cid);
                }
            }
        }

        // pipeline 批量 load_meta
        let mut metas = Vec::with_capacity(all_ids.len());
        for id in &all_ids {
            match self.load_meta_from_conn(&mut conn, id).await {
                Ok(meta) => metas.push(meta),
                Err(_) => continue,
            }
        }

        metas.sort_by_key(|m| m.created_at);
        Ok(metas)
    }

    async fn update_thread_status(&self, id: &ThreadId, status: &str) -> Result<()> {
        let mut conn = self.conn().await?;

        let now = Utc::now().to_rfc3339();
        let _: () = conn.hset(key_thread(id), "agent_status", status).await?;
        let _: () = conn.hset(key_thread(id), "updated_at", &now).await?;

        let now_ts = Utc::now().timestamp_millis() as f64;
        let _: () = conn.zadd("threads:by_updated", id, now_ts).await?;

        Ok(())
    }

    async fn invalidate_context_cache(&self, thread_id: &ThreadId) -> Result<()> {
        let mut conn = self.conn().await?;
        let _: () = conn.del(key_thread_cached(thread_id)).await?;
        Ok(())
    }

    async fn delete_messages(
        &self,
        thread_id: &ThreadId,
        message_ids: &[crate::messages::MessageId],
    ) -> Result<()> {
        if message_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn().await?;

        let mid_strs: Vec<String> = message_ids
            .iter()
            .map(|m| m.as_uuid().to_string())
            .collect();

        let deleted: i64 = Script::new(DELETE_MESSAGES_LUA)
            .key(key_messages(thread_id))
            .key(msg_data_key(thread_id))
            .key(key_msg_ids(thread_id))
            .key(key_thread(thread_id))
            .arg(&mid_strs)
            .invoke_async(&mut *conn)
            .await?;

        if deleted > 0 {
            // 更新 updated_at
            let now = Utc::now().to_rfc3339();
            let _: () = conn.hset(key_thread(thread_id), "updated_at", &now).await?;

            // 失效缓存
            let _: () = conn.del(key_thread_cached(thread_id)).await?;
        }

        Ok(())
    }
}

// ── 内部辅助方法 ──────────────────────────────────────────────────────────────

impl RedisThreadStore {
    /// 递归删除线程子树
    async fn delete_thread_recursive(
        &self,
        conn: &mut Connection,
        id: &ThreadId,
    ) -> Result<()> {
        // 先递归删除子线程
        let child_ids: Vec<String> = conn.smembers(key_children(id)).await?;
        for cid in child_ids {
            Box::pin(self.delete_thread_recursive(conn, &cid)).await?;
        }

        // 获取 parent_thread_id
        let parent_id: Option<String> = conn.hget(key_thread(id), "parent_thread_id").await?;
        let parent = parent_id.unwrap_or_default();

        // 删除自身所有 key
        Script::new(DELETE_THREAD_LUA)
            .key(key_thread(id))
            .key(key_messages(id))
            .key(msg_data_key(id))
            .key(key_msg_ids(id))
            .key(key_thread_cached(id))
            .key(key_thread_config(id))
            .key(key_children(id))
            .key("threads:all")
            .key("threads:by_updated")
            .arg(id.as_str())
            .arg(&parent)
            .invoke_async::<_, ()>(&mut **conn)
            .await?;

        Ok(())
    }

    /// 沿 parent_thread_id 链向上回溯
    async fn resolve_ancestor_chain(
        &self,
        conn: &mut Connection,
        thread_id: &ThreadId,
    ) -> Result<Vec<ThreadId>> {
        let mut chain = vec![thread_id.clone()];
        let mut current = thread_id.clone();
        loop {
            let parent_id: Option<String> = conn.hget(key_thread(&current), "parent_thread_id").await?;
            match parent_id {
                Some(parent) if !parent.is_empty() => {
                    chain.push(parent.clone());
                    current = parent;
                }
                _ => break,
            }
        }
        chain.reverse();
        Ok(chain)
    }

    /// 从连接加载元数据（不含 cached_context）
    async fn load_meta_from_conn(
        &self,
        conn: &mut Connection,
        id: &str,
    ) -> Result<ThreadMeta> {
        let hash = self.hgetall(conn, &key_thread(id)).await?;

        if hash.is_empty() {
            anyhow::bail!("thread {id} 不存在");
        }

        let config: Option<String> = conn.get(key_thread_config(id)).await?;
        hash_to_meta(&hash, config, None)
    }

    /// 加载指定线程的全部消息（内部用连接）
    async fn load_messages_from_conn(
        &self,
        conn: &mut Connection,
        id: &str,
    ) -> Result<Vec<BaseMessage>> {
        let msg_ids: Vec<String> = conn.zrange(key_messages(id), 0, -1).await?;
        if msg_ids.is_empty() {
            return Ok(vec![]);
        }

        let jsons: Vec<Option<String>> = conn.hget(msg_data_key(id), &msg_ids).await?;

        let mut msgs = Vec::with_capacity(jsons.len());
        for json in jsons.into_iter().flatten() {
            msgs.push(serde_json::from_str(&json)?);
        }
        Ok(msgs)
    }

    /// 加载到指定 message_id 为止的消息
    async fn load_messages_up_to(
        &self,
        conn: &mut Connection,
        thread_id: &str,
        target_msg_id: &str,
    ) -> Result<Vec<BaseMessage>> {
        // 找到目标消息的 score（序号）
        let target_score: Option<f64> = conn.zscore(key_messages(thread_id), target_msg_id).await?;

        let target_score = match target_score {
            Some(s) => s,
            None => return Ok(vec![]), // 消息不存在
        };

        // ZRANGEBYSCORE 0 target_score
        let msg_ids: Vec<String> = conn
            .zrangebyscore(key_messages(thread_id), 0, target_score)
            .await?;

        if msg_ids.is_empty() {
            return Ok(vec![]);
        }

        let jsons: Vec<Option<String>> = conn.hget(msg_data_key(thread_id), &msg_ids).await?;

        let mut msgs = Vec::with_capacity(jsons.len());
        for json in jsons.into_iter().flatten() {
            msgs.push(serde_json::from_str(&json)?);
        }
        Ok(msgs)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    include!("redis_store_test.rs");
}
