//! Stdio transport wrapper for ACP.
//!
//! Specifically handles the `RequestPermission` and `elicitation/create` responses
//! for ToolSearch support.
//!
//! See [`MpscToJsonRpcBridge`] for the mpsc-to-JSON-RPC bridge layer.

use async_trait::async_trait;
use serde_json::Value;

use super::types::{AcpError, IncomingMessage, RequestId};
use super::AcpTransport;

/// Stdio-based ACP transport — stub implementation.
///
/// The actual stdio transport uses `agent_client_protocol_tokio::Stdio` directly
/// with `Agent::builder().connect_to(Stdio::new())`. This struct exists to satisfy
/// the [`AcpTransport`] trait bound and for future use when we want a unified
/// transport interface for stdio-driven agents.
pub struct StdioTransport;

#[async_trait]
impl AcpTransport for StdioTransport {
    async fn send_request(&self, _method: &str, _params: Value) -> Result<Value, AcpError> {
        Err(AcpError::new(-32603, "StdioTransport not yet implemented"))
    }

    async fn send_notification(&self, _method: &str, _params: Value) -> Result<(), AcpError> {
        Err(AcpError::new(-32603, "StdioTransport not yet implemented"))
    }

    async fn recv(&self) -> Option<IncomingMessage> {
        None
    }

    async fn send_response(
        &self,
        _id: RequestId,
        _result: Result<Value, AcpError>,
    ) -> Result<(), AcpError> {
        Err(AcpError::new(-32603, "StdioTransport not yet implemented"))
    }
}
