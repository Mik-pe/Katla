//! Message roles and tool calls shared by editor and optional host adapters.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The source role of a displayed or transported message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A scene or resource call supplied by an agent host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}
