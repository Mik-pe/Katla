#[cfg(feature = "llm-assistant")]
pub mod co_creator;

pub mod context;
pub mod tools;

#[cfg(feature = "llm-assistant")]
pub use co_creator::CoCreatorAgent;
#[cfg(feature = "llm-assistant")]
pub use co_creator::{LocalAction, LocalResponse, StreamEvent};
pub use context::{SceneContext, serialize_scene_context};
pub use tools::{assets, placement, templates, tuning};

#[cfg(feature = "llm-assistant")]
pub mod config;

#[cfg(feature = "llm-assistant")]
pub mod llm;

#[cfg(feature = "llm-assistant")]
pub mod runtime;

#[cfg(feature = "llm-assistant")]
pub use config::LlmConfig;

#[cfg(feature = "llm-assistant")]
pub use llm::{
    ChatMessage, ChatResponse, FinishReason, LlmError, MessageRole, StreamChunk, ToolCall,
    ToolCallDelta, ToolDefinition,
};

#[cfg(feature = "llm-assistant")]
pub use llm::openai::OpenAiProvider;

#[cfg(feature = "llm-assistant")]
pub use runtime::{AsyncBridge, PendingChatRequest, PendingStreamRequest};

#[cfg(feature = "mcp-server")]
pub mod mcp;

#[cfg(feature = "mcp-server")]
pub use mcp::{
    KatlaMcpServer, McpBridge, McpOp, McpOpKind, McpResponse, McpToolResult, PendingMcpRequest,
    start_mcp_server_thread,
};
