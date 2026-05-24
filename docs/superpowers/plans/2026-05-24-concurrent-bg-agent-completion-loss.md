# Concurrent BG Agent Completion Loss Fix Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix bug where only one `BackgroundTaskCompleted` event is received when 2+ concurrent background agents finish, causing the parent agent to hang waiting for completions that never arrive.

**Architecture:** The fix adds diagnostic tracing across the entire bg completion event pipeline (sender → bg pump → transport → client pump → TUI handler), writes a concurrency integration test to reproduce the issue, then patches identified root causes. The piggybacks on existing unbounded channel architecture — no new channels or transport layers.

**Tech Stack:** Rust, tokio unbounded channels, serde_json, peri-acp transport layer (MpscTransport), peri-tui event handling

---

## Architecture Analysis

```
Background Task Closure (spawn closure in define.rs)
  │  spawn_bg_sender.send(AgentEvent::BackgroundTaskCompleted(result))
  │  [unbounded mpsc: bg_event_tx → bg_event_rx]
  ▼
Bg Event Pump (executor.rs:350, tokio::spawn)
  │  bg_event_rx.recv() → bg_sink.push_event()
  │  [TransportEventSink::push_event: serialize → transport.send_notification]
  ▼
MpscServerTransport.send_notification() → server_tx channel
  │  [pump_incoming task: server_rx → outgoing_tx (IncomingMessage)]
  ▼
Client pump (client.rs:86, run_pump)
  │  transport.recv() → deserialize → notification_tx.send(AcpNotification::AgentEvent)
  ▼
TUI poll_agent (polling.rs:76)
  │  acp_notification_rx.try_recv() → handle_acp_notification → handle_agent_event
  ▼
handle_background_task_completed (agent_events_bg.rs:52)
  │  decrement count → match SubAgentGroup → check done_pending_bg
  ▼
Set pending_bg_continuation when count == 0 && agent_done_pending_bg
```

**Key channel lifetimes:**
- `bg_event_tx` clones: 1 in SubAgentMiddleware + 1 per bg task closure
- `bg_event_rx`: moved into bg event pump tokio::spawn task
- Bg pump: NOT awaited — runs independently until channel closes
- `bg_sink` (Arc<dyn EventSink>): held by both main pump and bg pump as Arc clones

**Suspected failure point**: Bg event pump exits before all bg tasks complete, OR events are silently dropped at the `push_event` boundary (serialization failure, transport error).

---

### Task 1: Add diagnostic tracing across the entire bg event pipeline

**Files:**
- Modify: `peri-middlewares/src/subagent/tool/define.rs:485-488` (sender side)
- Modify: `peri-acp/src/session/executor.rs:350-354` (bg pump side)
- Modify: `peri-acp/src/session/event_sink.rs:50-58` (push_event serialization)
- Modify: `peri-tui/src/acp_client/client.rs:116-125` (client pump deserialization)
- Modify: `peri-tui/src/app/agent_events_bg.rs:52-62` (TUI handler entry)

- [ ] **Step 1: Add tracing at sender side** — log when bg task sends completion event

In `peri-middlewares/src/subagent/tool/define.rs`, lines 485-488, add a `tracing::info!` before the send:

```rust
// 通过独立通道发送完成事件（不依赖 event_tx，不受 close_channel 影响）
if let Some(ref sender) = spawn_bg_sender {
    tracing::info!(
        task_id = %spawn_task_id,
        agent_name = %spawn_agent_name,
        success = result.success,
        "bg-task sending BackgroundTaskCompleted via bg_event_tx"
    );
    let _ = sender.send(AgentEvent::BackgroundTaskCompleted(result));
} else {
    tracing::warn!(
        task_id = %spawn_task_id,
        agent_name = %spawn_agent_name,
        "bg-task spawn_bg_sender is None — BackgroundTaskCompleted will NOT be sent"
    );
}
```

- [ ] **Step 2: Add tracing at bg pump side** — log each event received and pushed

In `peri-acp/src/session/executor.rs`, lines 346-355, modify the bg event pump:

```rust
{
    let mut bg_event_rx = agent_output.bg_event_rx;
    let bg_session_id = session_id.clone();
    let bg_cw = effective_context_window;
    tokio::spawn(async move {
        let mut bg_event_count: u64 = 0;
        while let Some(bg_event) = bg_event_rx.recv().await {
            bg_event_count += 1;
            if matches!(&bg_event, ExecutorEvent::BackgroundTaskCompleted(r) if !r.success) {
                tracing::warn!(
                    count = bg_event_count,
                    task_id = %r.task_id,
                    agent_name = %r.agent_name,
                    success = r.success,
                    "bg-event-pump: received BackgroundTaskCompleted (FAILED)"
                );
            } else if matches!(&bg_event, ExecutorEvent::BackgroundTaskCompleted(_)) {
                tracing::info!(
                    count = bg_event_count,
                    "bg-event-pump: received BackgroundTaskCompleted"
                );
            }
            bg_sink.push_event(&bg_session_id, &bg_event, bg_cw).await;
        }
        tracing::info!(
            total_bg_events = bg_event_count,
            "bg-event-pump: channel closed, exiting"
        );
    });
}
```

- [ ] **Step 3: Add tracing at push_event** — log serialization result and transport sending

In `peri-acp/src/session/event_sink.rs`, modify `TransportEventSink::push_event` to add logging after serialization:

```rust
let event_json = match serde_json::to_string(event) {
    Ok(s) => s,
    Err(e) => {
        error!(error = %e, "EventSink: serialize ExecutorEvent failed");
        return;
    }
};
// ADD: log when sending BackgroundTaskCompleted specifically
if matches!(event, ExecutorEvent::BackgroundTaskCompleted(r) if r.success) {
    tracing::info!(
        task_id = %r.task_id,
        agent_name = %r.agent_name,
        event_json_len = event_json.len(),
        "EventSink: serialized BackgroundTaskCompleted, sending via transport"
    );
}
```

- [ ] **Step 4: Add tracing at client pump** — log deserialization of BackgroundTaskCompleted

In `peri-tui/src/acp_client/client.rs`, after the successful deserialization match at line 117-124, add:

```rust
match event_result {
    Ok(event) => {
        if matches!(&event, peri_agent::agent::events::AgentEvent::BackgroundTaskCompleted(r) if r.success) {
            tracing::info!(
                task_id = %r.task_id,
                agent_name = %r.agent_name,
                event_count = event_count,
                "client-pump: deserialized BackgroundTaskCompleted, sending to TUI"
            );
        }
        let _ = notification_tx.send(AcpNotification::AgentEvent { session_id, event });
    }
    // ...
}
```

- [ ] **Step 5: Add tracing at TUI handler entry** — log receipt of BackgroundTaskCompleted

In `peri-tui/src/app/agent_events_bg.rs`, at the start of `handle_background_task_completed` (line 52), add:

```rust
tracing::info!(
    task_id = %task_id,
    agent_name = %agent_name,
    success = success,
    bg_count_before = self.session_mgr.sessions[self.session_mgr.active].background_task_count,
    agent_done_pending = self.session_mgr.sessions[self.session_mgr.active].agent.agent_done_pending_bg,
    "TUI: handle_background_task_completed called"
);
```

- [ ] **Step 6: Verify diagnostic logging compiles and runs**

```bash
cargo build -p peri-middlewares -p peri-acp -p peri-tui 2>&1 | tail -5
```

Expected: clean build with no errors. Then manually test by launching TUI and triggering 2+ concurrent bg agents.

---

### Task 2: Write integration test reproducing concurrent bg agent completion loss

**Files:**
- Create: `peri-acp/tests/concurrent_bg_agent_test.rs`

- [ ] **Step 1: Create test file with scaffolding**

Create `peri-acp/tests/concurrent_bg_agent_test.rs`:

```rust
//! Integration test: verify that N concurrent background agents all produce
//! BackgroundTaskCompleted events when they finish.
//!
//! This test reproduces the bug described in
//! spec/issues/2026-05-24-concurrent-bg-agent-only-one-completion.md

use std::sync::Arc;
use tokio::sync::mpsc;

/// Skips the full ACP transport layer and directly tests the bg event channel
/// by simulating N background tasks all sending completion events concurrently.
#[tokio::test]
async fn test_concurrent_bg_tasks_all_emit_completion() {
    use peri_agent::agent::events::{AgentEvent, BackgroundTaskResult};

    let (bg_tx, mut bg_rx) = mpsc::unbounded_channel::<AgentEvent>();
    let task_count = 2usize;

    let handles: Vec<_> = (0..task_count)
        .map(|i| {
            let tx = bg_tx.clone();
            tokio::spawn(async move {
                // Simulate variable completion time
                tokio::time::sleep(std::time::Duration::from_millis(i as u64 * 50)).await;
                let result = BackgroundTaskResult {
                    task_id: format!("bg-task-{}", i),
                    agent_name: format!("agent-{}", i),
                    prompt_summary: format!("task {}", i),
                    success: true,
                    output: format!("output {}", i),
                    tool_calls_count: i,
                    duration_ms: 100 + i as u64 * 10,
                };
                let _ = tx.send(AgentEvent::BackgroundTaskCompleted(result));
            })
        })
        .collect();

    // Collect all events within a timeout
    let mut received = Vec::new();
    let collect_fut = async {
        while let Some(event) = bg_rx.recv().await {
            received.push(event);
            if received.len() == task_count {
                break;
            }
        }
    };

    let timeout = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        collect_fut,
    )
    .await;

    assert!(
        timeout.is_ok(),
        "Timed out waiting for all bg task completions"
    );

    let bg_completions: Vec<_> = received
        .iter()
        .filter(|e| matches!(e, AgentEvent::BackgroundTaskCompleted(_)))
        .collect();
    assert_eq!(
        bg_completions.len(),
        task_count,
        "Expected {} BackgroundTaskCompleted events, got {}",
        task_count,
        bg_completions.len()
    );

    // Cleanup
    drop(bg_tx);
    for h in handles {
        let _ = h.await;
    }
}
```

- [ ] **Step 2: Run test to verify it passes (baseline)**

```bash
cargo test -p peri-acp --test concurrent_bg_agent_test -- --nocapture
```

Expected: PASS — this verifies the unbounded channel itself works correctly for N concurrent senders.

---

### Task 3: Add concurrency stress test for the actual SubAgentTool::invoke_background path

**Files:**
- Modify: `peri-acp/tests/concurrent_bg_agent_test.rs` (append)

- [ ] **Step 1: Add test that exercises the full bg event path with a mock transport**

Append to `peri-acp/tests/concurrent_bg_agent_test.rs`:

```rust
/// Tests the full ACP bg event flow: SubAgentTool → bg_event_tx → bg pump → EventSink → transport.
/// Uses peri-acp's own mpsc transport pair and TransportEventSink.
#[tokio::test]
async fn test_bg_event_pump_receives_all_completions() {
    use peri_acp::session::event_sink::TransportEventSink;
    use peri_acp::transport::mpsc::mpsc_transport_pair;
    use peri_acp::transport::AcpTransport;
    use peri_agent::agent::events::{AgentEvent, BackgroundTaskResult};

    let (client_transport, server_transport) = mpsc_transport_pair();
    let sink = Arc::new(TransportEventSink::new(Arc::new(server_transport)));
    let (bg_tx, mut bg_rx) = mpsc::unbounded_channel::<AgentEvent>();

    let session_id = "test-session".to_string();
    let context_window = 200_000u32;
    let bg_sink = Arc::clone(&sink);
    let bg_session_id = session_id.clone();
    let bg_cw = context_window;

    // Spawn bg event pump (same pattern as executor.rs:350)
    let pump_handle = tokio::spawn(async move {
        while let Some(bg_event) = bg_rx.recv().await {
            bg_sink.push_event(&bg_session_id, &bg_event, bg_cw).await;
        }
    });

    // Spawn N concurrent bg tasks
    let task_count = 3usize;
    let handles: Vec<_> = (0..task_count)
        .map(|i| {
            let tx = bg_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(i as u64 * 30)).await;
                let result = BackgroundTaskResult {
                    task_id: format!("bg-{}", i),
                    agent_name: format!("test-agent-{}", i),
                    prompt_summary: format!("prompt-{}", i),
                    success: true,
                    output: "test output".to_string(),
                    tool_calls_count: 1,
                    duration_ms: 100,
                };
                let _ = tx.send(AgentEvent::BackgroundTaskCompleted(result));
            })
        })
        .collect();

    // Wait for all senders to finish and drop
    for h in handles {
        let _ = h.await;
    }
    // Drop the last sender so bg_rx returns None
    drop(bg_tx);

    // Wait for pump to finish
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        pump_handle,
    )
    .await
    .expect("bg event pump did not finish in time");

    // Now verify the client side received all events from the transport
    let mut received_count = 0;
    let collect_fut = async {
        loop {
            match client_transport.recv().await {
                Some(msg) => {
                    received_count += 1;
                }
                None => break,
            }
        }
        received_count
    };

    // Transport should have received 3 events × 3 pushes each (peri/agent_event + peri/* + session/update)
    // At minimum we check that at least 3 notifications were sent
    let count = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        collect_fut,
    )
    .await
    .unwrap_or(0);

    assert!(
        count >= 3,
        "Expected at least 3 transport notifications (one per bg task), got {}",
        count
    );
}
```

- [ ] **Step 2: Run the integration test**

```bash
cargo test -p peri-acp --test concurrent_bg_agent_test -- --nocapture
```

Expected: PASS. If it FAILS, the bg event pump is dropping events.

- [ ] **Step 3: Commit diagnostic tracing and tests**

```bash
git add peri-middlewares/src/subagent/tool/define.rs peri-acp/src/session/executor.rs peri-acp/src/session/event_sink.rs peri-tui/src/acp_client/client.rs peri-tui/src/app/agent_events_bg.rs peri-acp/tests/concurrent_bg_agent_test.rs
git commit -m "feat: add diagnostic tracing for bg agent completion event pipeline

Add tracing at sender, bg-pump, push_event, client-pump, and TUI handler
levels to diagnose why concurrent bg agent completions are lost.

Add integration tests verifying the bg event channel and full ACP pump
receive all completions from N concurrent senders.

Related: spec/issues/2026-05-24-concurrent-bg-agent-only-one-completion.md

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 4: Fix root cause — ensure bg event pump is not silently dropped

Based on diagnostic output, if the root cause is that `bg_event_rx` returns `None` prematurely (channel closes before all tasks finish), the fix is to ensure the sender clone inside `SubAgentMiddleware` outlives all background tasks. However, since `tokio::spawn` closures each hold their own sender clone, the channel should naturally stay open. The more likely fix is one of the following:

**Files:**
- Modify: `peri-middlewares/src/subagent/mod.rs:224-226` (ensure sender propagation)
- Modify: `peri-acp/src/session/executor.rs:343-355` (ensure bg pump is awaited or verified)
- Possibly: `peri-middlewares/src/subagent/tool/define.rs:420` (verify sender clone is captured correctly)

**(Task 4's exact content depends on diagnostic findings from Tasks 1-3. Execute Tasks 1-3 first, analyze the logs, then fill in Task 4.)**

- [ ] **Step 1: Run diagnostic build and reproduce issue to collect logs**

After completing Tasks 1-3, manually trigger 2+ concurrent bg agents in the TUI:

1. Start TUI: `cargo run -p peri-tui`
2. Send a prompt that triggers concurrent bg agents, e.g.: "Review the changes in peri-acp/src/agent/builder.rs and peri-acp/src/session/executor.rs in parallel using code-reviewer background agents"
3. Observe RUST_LOG output for diagnostic messages

- [ ] **Step 2: Analyze logs and identify the exact point of event loss**

Expected diagnostic output (normal path):
```
INFO bg-task sending BackgroundTaskCompleted via bg_event_tx task_id=bg-xxx1 agent_name=code-reviewer
INFO bg-task sending BackgroundTaskCompleted via bg_event_tx task_id=bg-xxx2 agent_name=code-reviewer
INFO bg-event-pump: received BackgroundTaskCompleted count=1
INFO bg-event-pump: received BackgroundTaskCompleted count=2
INFO EventSink: serialized BackgroundTaskCompleted, sending via transport task_id=bg-xxx1
INFO EventSink: serialized BackgroundTaskCompleted, sending via transport task_id=bg-xxx2
INFO client-pump: deserialized BackgroundTaskCompleted, sending to TUI task_id=bg-xxx1
INFO client-pump: deserialized BackgroundTaskCompleted, sending to TUI task_id=bg-xxx2
INFO TUI: handle_background_task_completed called task_id=bg-xxx1
INFO TUI: handle_background_task_completed called task_id=bg-xxx2
INFO all background tasks completed, auto-submitting continuation
```

Bug scenario: if ONE completion is lost, identify which step has the gap (e.g., sender logs 2, pump logs 1).

- [ ] **Step 3: Apply targeted fix based on diagnostic findings**

**Scenario A** (sender never calls send — `spawn_bg_sender` is None):
→ Fix `SubAgentMiddleware::build_tool` to verify sender is propagated

**Scenario B** (sender calls send but pump never receives):
→ Channel was closed prematurely. Check if `tokio::spawn` task holding `bg_event_rx` is cancelled.
→ Fix: store `JoinHandle` from line 350 and ensure pump completes before `execute_prompt` returns.

**Scenario C** (pump receives but push_event drops it):
→ `serde_json::to_string` or transport failed silently.
→ Fix: add retry or fallback path in push_event.

**Scenario D** (client pump receives but TUI doesn't handle):
→ The `AgentDone` notification arrives between events, and `poll_agent` breaks early.
→ Fix: ensure `poll_agent` drains ALL pending notifications before returning.

- [ ] **Step 4: Commit the fix**

---

### Task 5: Fix SubAgentGroup matching — handle duplicate agent_name in concurrent bg tasks

This is a preemptive fix regardless of diagnostic findings. When two bg agents share the same `agent_name`, the second `BackgroundTaskCompleted` event won't find its matching SubAgentGroup (already marked `is_running=false`).

**Files:**
- Modify: `peri-tui/src/app/agent_events_bg.rs:112-131` (SubAgentGroup matching loop)

- [ ] **Step 1: Change SubAgentGroup matching to prefer the first still-running match**

Current code matches `agent_id == &agent_name && is_background && is_running` and `break`s on first match. If two groups have the same `agent_name` and the first was already updated, the second completion will never match.

Fix: change the matching to prefer a match where we ALSO check that `final_result.is_none()` (not yet updated):

```rust
// Find the best match: prefer a SubAgentGroup that is still running AND
// hasn't been updated yet (handles same-name concurrent bg agents).
let mut best_idx: Option<usize> = None;
for (idx, vm) in session.messages.view_messages.iter().enumerate() {
    if let MessageViewModel::SubAgentGroup {
        agent_id,
        is_running,
        is_background,
        bg_hash: _,
        final_result,
        ..
    } = vm
    {
        if *is_background && *is_running && agent_id == &agent_name {
            // Prefer a group that hasn't been updated yet
            if final_result.is_none() {
                best_idx = Some(idx);
                break; // exact match — this one is for us
            }
            // Fallback: group is running but already has a result (shouldn't happen
            // but handle gracefully)
            if best_idx.is_none() {
                best_idx = Some(idx);
            }
        }
    }
}

if let Some(idx) = best_idx {
    if let MessageViewModel::SubAgentGroup {
        is_running,
        final_result,
        is_error,
        ..
    } = &mut session.messages.view_messages[idx]
    {
        *is_running = false;
        *final_result = Some(output.clone());
        *is_error = !success;
        found_and_updated = true;
    }
}
```

- [ ] **Step 2: Verify the fix compiles**

```bash
cargo build -p peri-tui 2>&1 | tail -5
```

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/agent_events_bg.rs
git commit -m "fix: handle concurrent same-name bg agents in SubAgentGroup matching

Change the SubAgentGroup matching loop to prefer groups that haven't been
updated yet (final_result is None). This prevents the second
BackgroundTaskCompleted event for a same-name agent from being silently
skipped because the first group was already marked as not running.

Related: spec/issues/2026-05-24-concurrent-bg-agent-only-one-completion.md

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 6: Integration verification — manual test with real LLM

**Files:** None (manual test)

- [ ] **Step 1: Build release binary**

```bash
cargo build -p peri-tui 2>&1 | tail -3
```

- [ ] **Step 2: Launch TUI and trigger concurrent bg agents**

```
cargo run -p peri-tui
```

Send prompt: "Launch two background code-reviewer agents: one to review peri-acp/src/agent/builder.rs and another to review peri-acp/src/session/executor.rs."

- [ ] **Step 3: Verify behavior**

Expected:
- Two bg agent cards appear in TUI (background_task_count = 2)
- After both complete, both completion results appear
- background_task_count reaches 0
- Agent automatically submits continuation (spinner stops)
- No manual cancel required

- [ ] **Step 4: Verify with different agent types**

Send prompt: "Launch a background code-reviewer to check peri-acp/src/agent/builder.rs and a background explorer to look for large files in peri-tui/src/"

Expected: same correct behavior with different `agent_name` values.

---

### Task 7: Cleanup — remove diagnostic tracing if too verbose

If the tracing added in Task 1 is too verbose for production, gate it behind a feature or RUST_LOG level.

**Files:**
- Modify: All files modified in Task 1

- [ ] **Step 1: Change tracing level from info to debug for bg pipeline events**

If logs are too noisy, downgrade:

```rust
tracing::debug!(...)  // instead of tracing::info!
```

- [ ] **Step 2: Commit cleanup**

```bash
git commit -m "chore: reduce bg pipeline tracing verbosity

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

## Self-Review Checklist

1. **Spec coverage**: The plan covers all symptoms: missing completion events (Tasks 1-4), stuck parent agent with count > 0 (Task 4), duplicate agent_name matching (Task 5), and manual verification (Task 6).

2. **Placeholder scan**: Task 4 (the actual fix) is left open pending diagnostic results — this is intentional since we can't know the exact root cause without running the diagnostic code first. Task 4 lists 4 concrete fix scenarios.

3. **Type consistency**: All event types are consistent: `AgentEvent::BackgroundTaskCompleted(BackgroundTaskResult)` throughout the pipeline. Channel types match: `UnboundedSender<AgentEvent>` / `UnboundedReceiver<AgentEvent>`.
