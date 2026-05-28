use std::sync::Arc;

use peri_agent::interaction::channel_types::{ChannelNotification, PermissionResponse};
use peri_agent::interaction::ChannelState;
use rmcp::model::{
    ClientCapabilities, ClientResult, CustomNotification, ErrorCode, ErrorData, Implementation,
    InitializeRequestParams, ServerNotification, ServerRequest,
};
use rmcp::service::{NotificationContext, RequestContext, RoleClient, Service};

/// MCP 自定义通知处理器，实现 `Service<RoleClient>` trait
///
/// 作为 MCP client 角色，接收来自 Channel Server 的自定义通知，
/// 根据 `method` 字段路由到 channel 消息推送或权限响应处理。
pub struct ChannelHandler {
    pub state: Arc<ChannelState>,
}

impl ChannelHandler {
    pub fn new(state: Arc<ChannelState>) -> Self {
        Self { state }
    }
}

impl ChannelHandler {
    /// 处理 `notifications/claude/channel` — 频道消息推送
    fn handle_channel_notification(&self, notif: &CustomNotification) -> Result<(), ErrorData> {
        let Some(params) = &notif.params else {
            tracing::warn!("channel notification params missing");
            return Ok(());
        };

        let Ok(msg) = serde_json::from_value::<ChannelNotification>(params.clone()) else {
            tracing::warn!("channel notification params parse failed");
            return Ok(());
        };

        let server_name = extract_server_name(&msg.source);

        let authorized = self.state.authorized.read().contains_key(&server_name);
        if !authorized {
            tracing::warn!(source = %msg.source, "unauthorized channel, ignoring notification");
            return Ok(());
        }

        let txs: Vec<_> = self
            .state
            .channel_msg_txs
            .read()
            .values()
            .cloned()
            .collect();
        if txs.is_empty() {
            tracing::warn!("no active sessions to receive channel notification");
            return Ok(());
        }

        tracing::info!(source = %msg.source, chat_id = %msg.chat_id, "received channel notification");
        for tx in &txs {
            let _ = tx.send(msg.clone());
        }
        Ok(())
    }

    /// 处理 `notifications/claude/permission` — 权限响应
    fn handle_permission_response(&self, notif: &CustomNotification) -> Result<(), ErrorData> {
        let Some(params) = &notif.params else {
            tracing::warn!("permission response params missing");
            return Ok(());
        };

        let Ok(resp) = serde_json::from_value::<PermissionResponse>(params.clone()) else {
            tracing::warn!("permission response params parse failed");
            return Ok(());
        };

        let sender = {
            let mut pending = self.state.pending_permissions.lock();
            pending.remove(&resp.request_id)
        };

        match sender {
            Some(s) => {
                tracing::info!(request_id = %resp.request_id, approved = resp.approved, "channel permission response");
                let _ = s.send(resp);
            }
            None => {
                tracing::warn!(request_id = %resp.request_id, "no pending permission request found");
            }
        }
        Ok(())
    }
}

impl Service<RoleClient> for ChannelHandler {
    fn handle_request(
        &self,
        _request: ServerRequest,
        _context: RequestContext<RoleClient>,
    ) -> impl std::future::Future<Output = Result<ClientResult, ErrorData>> + Send + '_ {
        // Channel handler 不处理 tool calls — 返回 METHOD_NOT_FOUND
        async {
            Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                "channel handler does not handle requests",
                None,
            ))
        }
    }

    fn handle_notification(
        &self,
        notification: ServerNotification,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = Result<(), ErrorData>> + Send + '_ {
        async move {
            match notification {
                ServerNotification::CustomNotification(notif) => match notif.method.as_str() {
                    "notifications/claude/channel" => self.handle_channel_notification(&notif),
                    "notifications/claude/permission" => self.handle_permission_response(&notif),
                    _ => {
                        tracing::debug!(method = %notif.method, "unhandled custom notification");
                        Ok(())
                    }
                },
                _ => {
                    // 标准通知 (logging, progress 等) 静默忽略
                    Ok(())
                }
            }
        }
    }

    fn get_info(&self) -> InitializeRequestParams {
        InitializeRequestParams::new(
            ClientCapabilities::default(),
            Implementation::from_build_env(),
        )
    }
}

/// 从 channel source 标识符提取 MCP server name
///
/// - `"plugin:weixin@anthropic:weixin"` → `"plugin_weixin_anthropic__weixin"`
/// - `"server:my-mcp"` → `"my-mcp"`
fn extract_server_name(source: &str) -> String {
    if let Some(rest) = source.strip_prefix("plugin:") {
        rest.replace(':', "__").replace('@', "_")
    } else if let Some(rest) = source.strip_prefix("server:") {
        rest.to_string()
    } else {
        source.to_string()
    }
}
