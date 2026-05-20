//! ACP Manual compact — runs full context compaction via compact_runner.
//! Extracted from original acp_server.rs (2026-05-20 split).

use std::sync::Arc;

use parking_lot::RwLock;
use serde_json::Value;
use tracing::info;

use peri_acp::session::compact_runner::{self, HookContext};
use peri_acp::session::event_sink::{EventSink, TransportEventSink};
use peri_acp::transport::types::AcpError;
use peri_agent::agent::events::AgentEvent as ExecutorEvent;
use peri_agent::agent::AgentCancellationToken;
use peri_agent::thread::ThreadMeta;

use crate::app::agent::LlmProvider;
use crate::config::PeriConfig;

use super::SharedSessions;

// ── Manual compact (spawned into background task) ────────────────────────────

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_compact(
    params: Value,
    sessions: &SharedSessions,
    provider: &Arc<RwLock<LlmProvider>>,
    peri_config: &Arc<RwLock<PeriConfig>>,
    hook_groups: &[Vec<peri_middlewares::hooks::RegisteredHook>],
    transport: &Arc<dyn peri_acp::transport::AcpTransport>,
    thread_store: &Arc<dyn peri_agent::thread::ThreadStore>,
) -> Result<Value, AcpError> {
    let session_id = params
        .get("sessionId")
        .or_else(|| params.get("session_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?
        .to_string();
    let instructions = params
        .get("instructions")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Read session data under lock.
    let (cwd, history, _thread_id) = {
        let sessions = sessions.lock().await;
        let state = sessions
            .get(&session_id)
            .ok_or_else(|| AcpError::new(-32602, "session not found"))?;
        (
            state.cwd.clone(),
            state.history.clone(),
            state.thread_id.clone(),
        )
    };

    if history.is_empty() {
        return Err(AcpError::new(-32602, "no messages to compact"));
    }

    let cancel = AgentCancellationToken::new();
    {
        let mut sessions = sessions.lock().await;
        if let Some(state) = sessions.get_mut(&session_id) {
            state.cancel_token = Some(cancel.clone());
        }
    }

    // Event channel + pump for compact events.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<ExecutorEvent>();
    let event_tx = Arc::new(std::sync::Mutex::new(Some(event_tx)));
    let event_sink: Arc<dyn EventSink> = Arc::new(TransportEventSink::new(Arc::clone(transport)));
    let sink = event_sink.clone();
    let sid = session_id.clone();
    let (pump_done_tx, pump_done_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        while let Some(exec_event) = event_rx.recv().await {
            sink.push_event(&sid, &exec_event, 0).await;
        }
        sink.push_done(&sid).await;
        let _ = pump_done_tx.send(());
    });

    let provider_snapshot = provider.read().clone();
    let peri_config_snapshot = peri_config.read().clone();
    let mut compact_config = peri_config_snapshot
        .config
        .compact
        .clone()
        .unwrap_or_default();
    compact_config.apply_env_overrides();

    let all_hooks: Vec<_> = hook_groups.iter().flatten().cloned().collect();
    let hook_ctx = HookContext {
        cwd: cwd.clone(),
        session_id: session_id.clone(),
        transcript_path: String::new(),
        provider_name: provider_snapshot.display_name().to_string(),
        instructions,
    };

    let result = compact_runner::run_full_compact(
        &history,
        provider_snapshot.into_model().as_ref(),
        &compact_config,
        &cwd,
        &event_tx,
        &cancel,
        &all_hooks,
        &hook_ctx,
    )
    .await;

    // Close event channel and wait for pump.
    {
        let mut tx_guard = event_tx.lock().unwrap();
        *tx_guard = None;
    }
    let _ = pump_done_rx.await;

    match result {
        Ok(output) => {
            // Update in-memory session history with compacted messages.
            let new_messages = output.new_messages;
            {
                let mut sessions = sessions.lock().await;
                if let Some(state) = sessions.get_mut(&session_id) {
                    state.history = new_messages.clone();
                    state.cancel_token = None;
                }
            }
            // Persist compacted messages as a new thread.
            let mut meta = ThreadMeta::new(&cwd);
            let truncated: String = output.summary.chars().take(30).collect();
            meta.title = Some(format!("Compact: {}…", truncated));
            match thread_store.create_thread(meta).await {
                Ok(new_tid) => {
                    if let Err(e) = thread_store.append_messages(&new_tid, &new_messages).await {
                        tracing::warn!(error = %e, "compact: failed to persist messages");
                    }
                    info!(session_id = %session_id, new_thread = %new_tid, "Manual compact completed");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "compact: failed to create thread");
                }
            }
            serde_json::to_value(serde_json::json!({ "status": "ok" }))
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }
        Err(e) => {
            {
                let mut sessions = sessions.lock().await;
                if let Some(state) = sessions.get_mut(&session_id) {
                    state.cancel_token = None;
                }
            }
            Err(AcpError::new(-32603, format!("Compact failed: {e}")))
        }
    }
}
