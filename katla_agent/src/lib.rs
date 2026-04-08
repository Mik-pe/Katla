pub mod co_creator;
pub mod context;

pub use co_creator::CoCreatorAgent;
pub use context::{SceneContext, serialize_scene_context};

#[cfg(feature = "llm-assistant")]
pub mod llm;

#[cfg(feature = "llm-assistant")]
pub mod runtime;

#[cfg(feature = "llm-assistant")]
pub use llm::{
    ChatMessage, ChatResponse, FinishReason, LlmError, MessageRole, ToolCall, ToolDefinition,
};

#[cfg(feature = "llm-assistant")]
pub use llm::openai::OpenAiProvider;

#[cfg(feature = "llm-assistant")]
pub use runtime::{AsyncBridge, PendingChatRequest};
