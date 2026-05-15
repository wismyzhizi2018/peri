use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::agent::events::{AgentEvent, FnEventHandler};
use crate::agent::react::{AgentInput, Reasoning};
use crate::agent::state::AgentState;
use crate::middleware::r#trait::Middleware;
use crate::tools::BaseTool;

/// 验证 before_tool 非拒绝错误（P4 路径）在 i>0 时，
/// 已通过 before_tool 的 modified_calls 也获得 error tool_result，
/// 不产生孤儿 tool_use（Anthropic API 400）。
///
/// 场景：3 个工具调用，call[0] 通过 before_tool（推入 modified_calls），
/// call[1] 的 before_tool 返回非 ToolRejected 错误 → P3 路径触发。
/// 修复前：call[0] 成为孤儿 tool_use；修复后：call[0] 也获得 error tool_result。
#[tokio::test]
async fn test_p3_error_flushes_modified_calls_no_orphaned_tool_use() {
    // 中间件：第一个工具通过，后续全部返回非 ToolRejected 错误
    struct PartialFailMiddleware;
    #[async_trait::async_trait]
    impl<S: State> Middleware<S> for PartialFailMiddleware {
        fn name(&self) -> &str {
            "PartialFailMiddleware"
        }
        async fn before_tool(&self, _state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
            if tool_call.id == "id1" {
                // 第一个工具通过
                Ok(tool_call.clone())
            } else {
                // 后续工具返回非 ToolRejected 错误
                Err(AgentError::ToolExecutionFailed {
                    tool: tool_call.name.clone(),
                    reason: "模拟 before_tool 错误".to_string(),
                })
            }
        }
    }

    struct ThreeToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for ThreeToolLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "call three tools",
                    vec![
                        ToolCall::new("id1", "tool_a", serde_json::json!({})),
                        ToolCall::new("id2", "tool_b", serde_json::json!({})),
                        ToolCall::new("id3", "tool_c", serde_json::json!({})),
                    ],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "all results received"))
            }
        }
    }

    struct EchoTool {
        name_str: &'static str,
    }
    #[async_trait::async_trait]
    impl BaseTool for EchoTool {
        fn name(&self) -> &str {
            self.name_str
        }
        fn description(&self) -> &str {
            "echo"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok(format!("{} done", self.name_str))
        }
    }

    let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let agent = ReActAgent::new(ThreeToolLLM)
        .max_iterations(5)
        .register_tool(Box::new(EchoTool { name_str: "tool_a" }))
        .register_tool(Box::new(EchoTool { name_str: "tool_b" }))
        .register_tool(Box::new(EchoTool { name_str: "tool_c" }))
        .add_middleware(Box::new(PartialFailMiddleware))
        .with_event_handler(Arc::new(FnEventHandler(move |event| {
            events_clone.lock().unwrap().push(event);
        })));

    let mut state = AgentState::new("/tmp");
    // P3 路径返回错误，execute 应传播该错误
    let result = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await;

    // P3 路径返回错误，execute 应传播该错误
    assert!(result.is_err(), "P3 路径应返回错误，实际: {:?}", result);

    // 核心断言：state 中每个 tool_use 必须有配对的 tool_result
    // 收集所有 AI 消息中的 tool_call_id
    let mut ai_tool_ids: Vec<String> = Vec::new();
    let mut tool_result_ids: Vec<String> = Vec::new();
    for msg in state.messages() {
        if let BaseMessage::Ai { tool_calls, .. } = msg {
            for tc in tool_calls {
                ai_tool_ids.push(tc.id.clone());
            }
        }
        if let BaseMessage::Tool { tool_call_id, .. } = msg {
            tool_result_ids.push(tool_call_id.clone());
        }
    }

    // 每个 tool_use 必须有对应 tool_result
    for id in &ai_tool_ids {
        assert!(
            tool_result_ids.contains(id),
            "tool_use id={} 缺少配对的 tool_result（孤儿 tool_use 会导致 Anthropic API 400）",
            id
        );
    }

    // 3 个 tool_use 都应有 tool_result
    assert_eq!(ai_tool_ids.len(), 3, "应有 3 个 tool_use");
    assert_eq!(
        tool_result_ids.len(),
        3,
        "应有 3 个 tool_result（含 error）"
    );
}

/// 验证取消信号在 i>0 时，modified_calls 也获得 error tool_result。
#[tokio::test]
async fn test_cancel_at_i_gt_0_flushes_modified_calls() {
    struct SlowTool;
    #[async_trait::async_trait]
    impl BaseTool for SlowTool {
        fn name(&self) -> &str {
            "slow_tool"
        }
        fn description(&self) -> &str {
            "hangs in before_tool then in execution"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok("never".to_string())
        }
    }

    // 中间件：第一个工具通过 before_tool 但后续挂起
    struct HangingBeforeToolMiddleware {
        call_count: Arc<Mutex<usize>>,
    }
    #[async_trait::async_trait]
    impl<S: State> Middleware<S> for HangingBeforeToolMiddleware {
        fn name(&self) -> &str {
            "HangingBeforeToolMiddleware"
        }
        async fn before_tool(
            &self,
            _state: &mut S,
            _tool_call: &ToolCall,
        ) -> AgentResult<ToolCall> {
            let should_hang = {
                let mut count = self.call_count.lock().unwrap();
                *count += 1;
                *count > 1
            };
            // guard 已在块内释放
            if should_hang {
                // 后续工具挂起（等待取消），较短超时避免测试卡住
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
            Ok(_tool_call.clone())
        }
    }

    struct TwoToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for TwoToolLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "call two tools",
                    vec![
                        ToolCall::new("id1", "slow_tool", serde_json::json!({})),
                        ToolCall::new("id2", "slow_tool", serde_json::json!({})),
                    ],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "ok"))
            }
        }
    }

    let cancel = CancellationToken::new();
    let call_count = Arc::new(Mutex::new(0usize));
    let agent = ReActAgent::new(TwoToolLLM)
        .max_iterations(5)
        .register_tool(Box::new(SlowTool))
        .add_middleware(Box::new(HangingBeforeToolMiddleware {
            call_count: Arc::clone(&call_count),
        }));

    // 在 before_tool 处理第二个工具时触发取消
    let token = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        token.cancel();
    });

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("go"), &mut state, Some(cancel))
        .await;

    assert!(
        matches!(result, Err(AgentError::Interrupted)),
        "取消后应返回 Interrupted，实际: {:?}",
        result
    );

    // 核心断言：所有 tool_use 必须有配对 tool_result
    let mut ai_tool_ids: Vec<String> = Vec::new();
    let mut tool_result_ids: Vec<String> = Vec::new();
    for msg in state.messages() {
        if let BaseMessage::Ai { tool_calls, .. } = msg {
            for tc in tool_calls {
                ai_tool_ids.push(tc.id.clone());
            }
        }
        if let BaseMessage::Tool { tool_call_id, .. } = msg {
            tool_result_ids.push(tool_call_id.clone());
        }
    }

    for id in &ai_tool_ids {
        assert!(
            tool_result_ids.contains(id),
            "取消后 tool_use id={} 缺少配对的 tool_result",
            id
        );
    }
    assert_eq!(
        ai_tool_ids.len(),
        tool_result_ids.len(),
        "取消后所有 tool_use 必须有配对 tool_result"
    );
}

/// 验证混合路径：Ok + ToolRejected + 非 ToolRejected 错误
/// call[0] Ok → 推入 modified_calls
/// call[1] ToolRejected → 独立写入 error tool_result，continue
/// call[2] 非 ToolRejected 错误 → P4 路径，flush modified_calls + flush pending
/// 所有 3 个 tool_use 都应有 tool_result，且无重复写入。
#[tokio::test]
async fn test_mixed_ok_rejected_error_all_tool_results_written() {
    struct MixedResultMiddleware;
    #[async_trait::async_trait]
    impl<S: State> Middleware<S> for MixedResultMiddleware {
        fn name(&self) -> &str {
            "MixedResultMiddleware"
        }
        async fn before_tool(&self, _state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
            match tool_call.id.as_str() {
                "id1" => Ok(tool_call.clone()),
                "id2" => Err(AgentError::ToolRejected {
                    tool: tool_call.name.clone(),
                    reason: "用户拒绝".to_string(),
                }),
                _ => Err(AgentError::ToolExecutionFailed {
                    tool: tool_call.name.clone(),
                    reason: "before_tool 错误".to_string(),
                }),
            }
        }
    }

    struct ThreeToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for ThreeToolLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "call three tools",
                    vec![
                        ToolCall::new("id1", "tool_a", serde_json::json!({})),
                        ToolCall::new("id2", "tool_b", serde_json::json!({})),
                        ToolCall::new("id3", "tool_c", serde_json::json!({})),
                    ],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "all results received"))
            }
        }
    }

    struct EchoTool {
        name_str: &'static str,
    }
    #[async_trait::async_trait]
    impl BaseTool for EchoTool {
        fn name(&self) -> &str {
            self.name_str
        }
        fn description(&self) -> &str {
            "echo"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok(format!("{} done", self.name_str))
        }
    }

    let agent = ReActAgent::new(ThreeToolLLM)
        .max_iterations(5)
        .register_tool(Box::new(EchoTool { name_str: "tool_a" }))
        .register_tool(Box::new(EchoTool { name_str: "tool_b" }))
        .register_tool(Box::new(EchoTool { name_str: "tool_c" }))
        .add_middleware(Box::new(MixedResultMiddleware));

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await;

    assert!(result.is_err(), "混合路径应返回错误，实际: {:?}", result);

    // 收集 tool_use 和 tool_result ID
    let mut ai_tool_ids: Vec<String> = Vec::new();
    let mut tool_result_ids: Vec<String> = Vec::new();
    for msg in state.messages() {
        if let BaseMessage::Ai { tool_calls, .. } = msg {
            for tc in tool_calls {
                ai_tool_ids.push(tc.id.clone());
            }
        }
        if let BaseMessage::Tool { tool_call_id, .. } = msg {
            tool_result_ids.push(tool_call_id.clone());
        }
    }

    // 所有 tool_use 都有配对 tool_result
    assert_eq!(ai_tool_ids.len(), 3, "应有 3 个 tool_use");
    assert_eq!(tool_result_ids.len(), 3, "应有 3 个 tool_result");
    for id in &ai_tool_ids {
        assert!(
            tool_result_ids.contains(id),
            "tool_use id={} 缺少配对的 tool_result",
            id
        );
    }

    // 无重复写入
    let unique_ids: HashSet<&str> = tool_result_ids.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        unique_ids.len(),
        tool_result_ids.len(),
        "tool_result 不应有重复 ID: {:?}",
        tool_result_ids
    );
}
