//! MCP server definition with tool implementations.
//!
//! Defines [`McpTaskServer`] which implements the rmcp [`ServerHandler`] trait,
//! exposing shedul3r's task execution engine as MCP tools.

use std::sync::Arc;

use domain_types::EnvironmentMap;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_handler, tool_router};
use serde::Deserialize;

use commands::TaskEngine;
use state::{InMemoryBulkhead, InMemoryCircuitBreaker, InMemoryRateLimiter, TokioRetryExecutor};
use subprocess::TokioSubprocessRunner;

/// Concrete engine type alias — matches the one in the api crate's `state.rs`.
pub type ConcreteEngine = TaskEngine<
    TokioSubprocessRunner,
    InMemoryRateLimiter,
    InMemoryCircuitBreaker,
    InMemoryBulkhead,
    TokioRetryExecutor,
>;

/// MCP server backed by the shedul3r task execution engine.
///
/// Each MCP session gets its own `McpTaskServer` instance, all sharing
/// the same underlying `ConcreteEngine` via `Arc`.
#[derive(Clone)]
pub struct McpTaskServer {
    engine: Arc<ConcreteEngine>,
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for McpTaskServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpTaskServer")
            .field("engine", &"ConcreteEngine")
            .finish_non_exhaustive()
    }
}

impl McpTaskServer {
    /// Creates a new MCP task server backed by the given engine.
    pub fn new(engine: Arc<ConcreteEngine>) -> Self {
        Self {
            engine,
            tool_router: Self::tool_router(),
        }
    }
}

/// Parameters for the `execute_task` MCP tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExecuteTaskParams {
    /// YAML task definition string. Must contain at minimum a `command` field.
    ///
    /// Example: `"name: my-task\ncommand: echo hello\ntimeout: 30s"`
    pub task: String,

    /// Input piped to the subprocess's stdin. For `claude -p`, this is the prompt.
    #[serde(default)]
    pub input: Option<String>,

    /// Override the limiter key (defaults to `provider-id` in the YAML).
    #[serde(default)]
    pub limiter_key: Option<String>,

    /// Environment variables as key-value pairs.
    #[serde(default)]
    pub environment: Option<EnvironmentMap>,

    /// Working directory for the subprocess (must be an absolute path).
    #[serde(default)]
    pub working_directory: Option<String>,

    /// Maximum execution time in milliseconds.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[tool_router]
impl McpTaskServer {
    /// Execute a task through the shedul3r engine.
    #[tool(
        description = "Execute a task. Provide a YAML task definition and optional input/prompt. The task runs as a subprocess with resilience patterns (rate limiting, circuit breaker, retry)."
    )]
    async fn execute_task(
        &self,
        Parameters(params): Parameters<ExecuteTaskParams>,
    ) -> Result<CallToolResult, McpError> {
        let request = domain_types::TaskRequest {
            task: params.task,
            input: params.input,
            limiter_key: params.limiter_key,
            environment: params.environment,
            working_directory: params.working_directory.map(std::path::PathBuf::from),
            timeout_ms: params.timeout_ms,
        };

        match self.engine.execute(request).await {
            Ok(response) => {
                let json = serde_json::to_string_pretty(&response)
                    .unwrap_or_else(|_| "failed to serialize response".to_owned());
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    /// Get the current scheduler status.
    #[tool(
        description = "Get scheduler status: active task count, pending tasks, and when the scheduler started."
    )]
    #[allow(clippy::unnecessary_wraps)] // rmcp #[tool] macro requires Result<CallToolResult, McpError> signature
    fn get_status(&self) -> Result<CallToolResult, McpError> {
        let status = self.engine.status();
        let json = serde_json::to_string_pretty(&status)
            .unwrap_or_else(|_| "failed to serialize status".to_owned());
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler]
impl ServerHandler for McpTaskServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("shedul3r", env!("CARGO_PKG_VERSION")))
            .with_protocol_version(ProtocolVersion::V_2025_03_26)
            .with_instructions(
                "shedul3r task execution engine. Execute tasks with resilience patterns \
             (rate limiting, circuit breaking, retry, bulkhead)."
                    .to_owned(),
            )
    }
}
