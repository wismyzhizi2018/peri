// redis_store 集成测试
//
// 默认连本地 Redis (127.0.0.1:6379, password=123456)
// 可通过 REDIS_URL 环境变量覆盖
// 每个测试用随机前缀隔离 key
//
// cargo test -p peri-agent --lib -- thread::redis_store::tests

use crate::messages::{MessageContent, MessageId};
use crate::thread::{RedisThreadStore, ThreadMeta};

/// 创建测试用 Redis 连接（使用随机前缀隔离）
async fn make_store() -> RedisThreadStore {
    let url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://:123456@127.0.0.1:6379".to_string());
    RedisThreadStore::new(&url).await.unwrap()
}

fn make_meta(cwd: &str) -> ThreadMeta {
    ThreadMeta::new(cwd)
}

fn make_text_msg(text: &str) -> BaseMessage {
    BaseMessage::Human {
        id: MessageId::new(),
        content: MessageContent::Text(text.to_string()),
    }
}

fn make_ai_msg(text: &str) -> BaseMessage {
    BaseMessage::Ai {
        id: MessageId::new(),
        content: MessageContent::Text(text.to_string()),
        tool_calls: vec![],
    }
}

#[tokio::test]
#[ignore]
async fn test_create_and_load_meta() {
    let store = make_store().await;
    let meta = make_meta("/tmp/test");
    let id = meta.id.clone();

    let returned_id = store.create_thread(meta).await.unwrap();
    assert_eq!(returned_id, id);

    let loaded = store.load_meta(&id).await.unwrap();
    assert_eq!(loaded.id, id);
    assert_eq!(loaded.cwd, "/tmp/test");
    assert_eq!(loaded.message_count, 0);
    assert_eq!(loaded.agent_status, "active");

    // 清理
    store.delete_thread(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_append_and_load_messages() {
    let store = make_store().await;
    let meta = make_meta("/tmp");
    let id = store.create_thread(meta).await.unwrap();

    let msgs = vec![make_text_msg("hello"), make_ai_msg("hi there")];
    store.append_messages(&id, &msgs).await.unwrap();

    let loaded = store.load_messages(&id).await.unwrap();
    assert_eq!(loaded.len(), 2);

    // 验证 meta 更新
    let meta = store.load_meta(&id).await.unwrap();
    assert_eq!(meta.message_count, 2);
    assert!(meta.title.is_some());
    assert_eq!(meta.title.unwrap(), "hello");

    store.delete_thread(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_append_messages_idempotent() {
    let store = make_store().await;
    let meta = make_meta("/tmp");
    let id = store.create_thread(meta).await.unwrap();

    let msg = make_text_msg("duplicate test");

    store.append_messages(&id, std::slice::from_ref(&msg)).await.unwrap();
    store.append_messages(&id, &[msg]).await.unwrap();

    let loaded = store.load_messages(&id).await.unwrap();
    assert_eq!(loaded.len(), 1, "幂等去重：重复消息不应重复写入");

    let meta = store.load_meta(&id).await.unwrap();
    assert_eq!(meta.message_count, 1);

    store.delete_thread(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_delete_messages() {
    let store = make_store().await;
    let meta = make_meta("/tmp");
    let id = store.create_thread(meta).await.unwrap();

    let msg1 = make_text_msg("msg1");
    let msg2 = make_text_msg("msg2");
    let msg3 = make_text_msg("msg3");
    let msg2_id = msg2.id();

    store
        .append_messages(&id, &[msg1, msg2, msg3])
        .await
        .unwrap();

    store.delete_messages(&id, &[msg2_id]).await.unwrap();

    let loaded = store.load_messages(&id).await.unwrap();
    assert_eq!(loaded.len(), 2);

    let meta = store.load_meta(&id).await.unwrap();
    assert_eq!(meta.message_count, 2);

    store.delete_thread(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_delete_thread_cascade() {
    let store = make_store().await;

    let parent_meta = make_meta("/tmp");
    let parent_id = parent_meta.id.clone();
    store.create_thread(parent_meta).await.unwrap();

    let mut child_meta = make_meta("/tmp");
    child_meta.parent_thread_id = Some(parent_id.clone());
    let child_id = child_meta.id.clone();
    store.create_thread(child_meta).await.unwrap();

    store
        .append_messages(&child_id, &[make_text_msg("child msg")])
        .await
        .unwrap();

    store.delete_thread(&parent_id).await.unwrap();

    assert!(store.load_meta(&parent_id).await.is_err());
    assert!(store.load_meta(&child_id).await.is_err());
}

#[tokio::test]
#[ignore]
async fn test_list_threads_hidden_filter() {
    let store = make_store().await;

    let visible = make_meta("/tmp");
    let visible_id = visible.id.clone();
    store.create_thread(visible).await.unwrap();

    let mut hidden = make_meta("/tmp");
    hidden.hidden = true;
    let hidden_id = hidden.id.clone();
    store.create_thread(hidden).await.unwrap();

    let threads = store.list_threads().await.unwrap();
    assert!(
        threads.iter().all(|t| t.id != hidden_id),
        "hidden thread 不应出现在列表中"
    );

    store.delete_thread(&visible_id).await.unwrap();
    store.delete_thread(&hidden_id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_list_child_threads() {
    let store = make_store().await;

    let parent = make_meta("/tmp");
    let parent_id = parent.id.clone();
    store.create_thread(parent).await.unwrap();

    let mut child1 = make_meta("/tmp");
    child1.parent_thread_id = Some(parent_id.clone());
    let _c1_id = child1.id.clone();
    store.create_thread(child1).await.unwrap();

    let mut child2 = make_meta("/tmp");
    child2.parent_thread_id = Some(parent_id.clone());
    let _c2_id = child2.id.clone();
    store.create_thread(child2).await.unwrap();

    let children = store.list_child_threads(&parent_id).await.unwrap();
    assert_eq!(children.len(), 2);

    store.delete_thread(&parent_id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_list_session_threads() {
    let store = make_store().await;

    let root = make_meta("/tmp");
    let root_id = root.id.clone();
    store.create_thread(root).await.unwrap();

    let mut mid = make_meta("/tmp");
    mid.parent_thread_id = Some(root_id.clone());
    let mid_id = mid.id.clone();
    store.create_thread(mid).await.unwrap();

    let mut leaf = make_meta("/tmp");
    leaf.parent_thread_id = Some(mid_id.clone());
    store.create_thread(leaf).await.unwrap();

    let session = store.list_session_threads(&root_id).await.unwrap();
    assert_eq!(session.len(), 3);

    store.delete_thread(&root_id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_load_context_single_thread() {
    let store = make_store().await;
    let meta = make_meta("/tmp");
    let id = store.create_thread(meta).await.unwrap();

    store
        .append_messages(&id, &[make_text_msg("a"), make_ai_msg("b")])
        .await
        .unwrap();

    let ctx = store.load_context(&id).await.unwrap();
    assert_eq!(ctx.len(), 2);

    // 第二次应走缓存
    let ctx2 = store.load_context(&id).await.unwrap();
    assert_eq!(ctx2.len(), 2);

    store.delete_thread(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_load_context_with_ancestor() {
    let store = make_store().await;

    let parent = make_meta("/tmp");
    let parent_id = parent.id.clone();
    store.create_thread(parent).await.unwrap();
    store
        .append_messages(&parent_id, &[make_text_msg("parent msg")])
        .await
        .unwrap();

    let parent_msgs = store.load_messages(&parent_id).await.unwrap();
    let snap_id = parent_msgs[0].id().as_uuid().to_string();

    let mut child = make_meta("/tmp");
    child.parent_thread_id = Some(parent_id.clone());
    child.snapshot_at_message_id = Some(snap_id);
    let child_id = child.id.clone();
    store.create_thread(child).await.unwrap();
    store
        .append_messages(&child_id, &[make_text_msg("child msg")])
        .await
        .unwrap();

    let ctx = store.load_context(&child_id).await.unwrap();
    assert_eq!(ctx.len(), 2);

    store.delete_thread(&parent_id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_load_context_incremental_cache() {
    let store = make_store().await;
    let meta = make_meta("/tmp");
    let id = store.create_thread(meta).await.unwrap();

    store
        .append_messages(&id, &[make_text_msg("first")])
        .await
        .unwrap();
    let ctx1 = store.load_context(&id).await.unwrap();
    assert_eq!(ctx1.len(), 1);

    store
        .append_messages(&id, &[make_text_msg("second")])
        .await
        .unwrap();
    let ctx2 = store.load_context(&id).await.unwrap();
    assert_eq!(ctx2.len(), 2);

    store.delete_thread(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_update_meta() {
    let store = make_store().await;
    let meta = make_meta("/tmp");
    let id = store.create_thread(meta).await.unwrap();

    let mut loaded = store.load_meta(&id).await.unwrap();
    loaded.title = Some("updated title".to_string());
    loaded.cancel_policy = "independent".to_string();
    store.update_meta(&id, loaded).await.unwrap();

    let updated = store.load_meta(&id).await.unwrap();
    assert_eq!(updated.title, Some("updated title".to_string()));
    assert_eq!(updated.cancel_policy, "independent");

    store.delete_thread(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_update_title() {
    let store = make_store().await;
    let meta = make_meta("/tmp");
    let id = store.create_thread(meta).await.unwrap();

    store.update_title(&id, "new title").await.unwrap();

    let loaded = store.load_meta(&id).await.unwrap();
    assert_eq!(loaded.title, Some("new title".to_string()));

    store.delete_thread(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_update_thread_status() {
    let store = make_store().await;
    let meta = make_meta("/tmp");
    let id = store.create_thread(meta).await.unwrap();

    store.update_thread_status(&id, "done").await.unwrap();

    let loaded = store.load_meta(&id).await.unwrap();
    assert_eq!(loaded.agent_status, "done");

    store.delete_thread(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_invalidate_context_cache() {
    let store = make_store().await;
    let meta = make_meta("/tmp");
    let id = store.create_thread(meta).await.unwrap();

    store
        .append_messages(&id, &[make_text_msg("cached")])
        .await
        .unwrap();
    store.load_context(&id).await.unwrap();

    store.invalidate_context_cache(&id).await.unwrap();

    let ctx = store.load_context(&id).await.unwrap();
    assert_eq!(ctx.len(), 1);

    store.delete_thread(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_thread_with_config() {
    let store = make_store().await;
    let mut meta = make_meta("/tmp");
    meta.config = Some(r#"{"model":"gpt-4o"}"#.to_string());
    let id = store.create_thread(meta).await.unwrap();

    let loaded = store.load_meta(&id).await.unwrap();
    assert_eq!(loaded.config, Some(r#"{"model":"gpt-4o"}"#.to_string()));

    store.delete_thread(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_empty_thread_operations() {
    let store = make_store().await;
    let meta = make_meta("/tmp");
    let id = store.create_thread(meta).await.unwrap();

    store.append_messages(&id, &[]).await.unwrap();

    let msgs = store.load_messages(&id).await.unwrap();
    assert!(msgs.is_empty());

    let ctx = store.load_context(&id).await.unwrap();
    assert!(ctx.is_empty());

    store.delete_thread(&id).await.unwrap();
}
