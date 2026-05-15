use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Thread 唯一标识符（UUID v7，按时间排序）
pub type ThreadId = String;

/// Thread 元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadMeta {
    pub id: ThreadId,
    /// 对话标题，可由第一条用户消息自动截取
    pub title: Option<String>,
    /// 创建时的工作目录
    pub cwd: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    /// 消息内容总字节数（由 list_threads 查询时计算）
    #[serde(default)]
    pub content_size: u64,
}

impl ThreadMeta {
    pub fn new(cwd: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            title: None,
            cwd: cwd.into(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            content_size: 0,
        }
    }
}
