//! ACP Notification dispatch — handles incoming notifications and pushes
//! session update notifications. Extracted from original acp_server.rs (2026-05-20 split).

use std::collections::HashMap;

use serde_json::Value;
use tracing::{debug, info};

use agent_client_protocol::schema::{
    AvailableCommand, AvailableCommandsUpdate, ConfigOptionUpdate, SessionId, SessionNotification,
    SessionUpdate,
};

use super::{build_config_options, AcpServerConfig, SessionState};

// ── Notification dispatch ────────────────────────────────────────────────────

pub(crate) fn handle_notification(
    method: &str,
    params: &Value,
    sessions: &HashMap<String, SessionState>,
) {
    if method == "$/cancel_request" {
        let session_id = params
            .get("sessionId")
            .or_else(|| params.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(state) = sessions.get(session_id) {
            if let Some(ref token) = state.cancel_token {
                token.cancel();
                info!(session_id = %session_id, "Cancel requested");
            }
        }
    } else {
        debug!(method = %method, "Unhandled notification");
    }
}

// ── Notification helpers ───────────────────────────────────────────────────────

/// Extract `sessionId` from JSON-RPC params, returning `default_value` if absent.
pub(crate) fn extract_session_id<'a>(params: &'a Value, default_value: &'a str) -> &'a str {
    params
        .get("sessionId")
        .or_else(|| params.get("session_id"))
        .and_then(|v| v.as_str())
        .unwrap_or(default_value)
}

/// Build the current set of config options and push a `ConfigOptionUpdate` notification.
pub(crate) async fn send_config_option_update(
    transport: &dyn peri_acp::transport::AcpTransport,
    session_id: &str,
    cfg: &AcpServerConfig,
) {
    if session_id.is_empty() {
        return;
    }
    let config_options = {
        let c = cfg.peri_config.read();
        let p = cfg.provider.read();
        build_config_options(&c, &p, cfg.permission_mode.load())
    };
    let update = SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(config_options));
    let notif = SessionNotification::new(SessionId::new(session_id.to_string()), update);
    let payload = match serde_json::to_value(&notif) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "Failed to serialize ConfigOptionUpdate notification");
            return;
        }
    };
    let _ = transport.send_notification("session/update", payload).await;
}

/// Push an `AvailableCommandsUpdate` notification for the given session.
pub(crate) async fn send_available_commands_update(
    transport: &dyn peri_acp::transport::AcpTransport,
    session_id: &str,
) {
    if session_id.is_empty() {
        return;
    }
    let commands = build_available_commands();
    let update = SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(commands));
    let notif = SessionNotification::new(SessionId::new(session_id.to_string()), update);
    let payload = match serde_json::to_value(&notif) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "Failed to serialize AvailableCommandsUpdate notification");
            return;
        }
    };
    let _ = transport.send_notification("session/update", payload).await;
}

/// Push a `SessionInfoUpdate` notification after prompt/compact completes.
pub(crate) async fn send_session_info_update(
    transport: &dyn peri_acp::transport::AcpTransport,
    session_id: &str,
) {
    use agent_client_protocol::schema::SessionInfoUpdate;
    let info = SessionInfoUpdate::new().updated_at(chrono::Utc::now().to_rfc3339());
    let update = SessionUpdate::SessionInfoUpdate(info);
    let notif = SessionNotification::new(SessionId::new(session_id.to_string()), update);
    let payload = match serde_json::to_value(&notif) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "Failed to serialize SessionInfoUpdate notification");
            return;
        }
    };
    let _ = transport.send_notification("session/update", payload).await;
}

/// Build the list of available slash commands for ACP clients.
fn build_available_commands() -> Vec<AvailableCommand> {
    vec![
        AvailableCommand::new("help", "Show available commands and their descriptions"),
        AvailableCommand::new("clear", "Clear the current conversation"),
        AvailableCommand::new(
            "compact",
            "Compress the conversation history to save context",
        ),
        AvailableCommand::new("context", "Display context usage / token statistics"),
        AvailableCommand::new("cost", "Show token usage and estimated cost"),
        AvailableCommand::new("model", "Switch the current LLM model"),
        AvailableCommand::new("mode", "Switch the current permission mode"),
        AvailableCommand::new("effort", "Configure LLM reasoning/thinking effort"),
        AvailableCommand::new("loop", "Control agent iteration loop"),
        AvailableCommand::new("history", "View and resume previous conversations"),
        AvailableCommand::new("doctor", "Diagnose configuration and connection issues"),
        AvailableCommand::new("mcp", "Manage MCP (Model Context Protocol) servers"),
        AvailableCommand::new("hooks", "Manage Claude Code hooks"),
        AvailableCommand::new("plugin", "Manage installed plugins"),
        AvailableCommand::new("cron", "Manage scheduled/cron tasks"),
        AvailableCommand::new("agents", "Manage sub-agent definitions"),
        AvailableCommand::new("memory", "Manage persistent memory entries"),
        AvailableCommand::new("login", "Configure authentication"),
        AvailableCommand::new("split", "Manage split session layouts"),
        AvailableCommand::new("rename", "Rename the current session"),
        AvailableCommand::new("lang", "Switch display language / locale"),
        AvailableCommand::new("exit", "Exit the application"),
    ]
}
