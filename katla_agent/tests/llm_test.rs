#![cfg(feature = "llm-assistant")]

use katla_agent::llm::{
    ChatMessage, ChatResponse, FinishReason, LlmError, LlmProvider, MessageRole, MockProvider,
    ToolCall, ToolDefinition,
};

#[test]
fn test_mock_provider_simple() {
    let provider = MockProvider::simple("Hello, world!");
    let messages = vec![ChatMessage {
        role: MessageRole::User,
        content: "Hi".to_string(),
        tool_calls: None,
        tool_call_id: None,
    }];

    let rt = tokio::runtime::Runtime::new().unwrap();
    let response = rt
        .block_on(provider.chat_completion(&messages, &[]))
        .unwrap();

    assert_eq!(response.message.content, "Hello, world!");
    assert_eq!(response.message.role, MessageRole::Assistant);
    assert_eq!(response.finish_reason, FinishReason::Stop);
    assert!(response.message.tool_calls.is_none());
}

#[test]
fn test_mock_provider_multiple_responses() {
    let provider = MockProvider::new(vec![
        ChatResponse {
            message: ChatMessage {
                role: MessageRole::Assistant,
                content: "First".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            finish_reason: FinishReason::Stop,
        },
        ChatResponse {
            message: ChatMessage {
                role: MessageRole::Assistant,
                content: "Second".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            finish_reason: FinishReason::Stop,
        },
    ]);

    let messages = vec![ChatMessage {
        role: MessageRole::User,
        content: "Hi".to_string(),
        tool_calls: None,
        tool_call_id: None,
    }];

    let rt = tokio::runtime::Runtime::new().unwrap();
    let r1 = rt
        .block_on(provider.chat_completion(&messages, &[]))
        .unwrap();
    assert_eq!(r1.message.content, "First");

    let r2 = rt
        .block_on(provider.chat_completion(&messages, &[]))
        .unwrap();
    assert_eq!(r2.message.content, "Second");
}

#[test]
fn test_mock_provider_exhausted_responses() {
    let provider = MockProvider::new(vec![]);
    let messages = vec![ChatMessage {
        role: MessageRole::User,
        content: "Hi".to_string(),
        tool_calls: None,
        tool_call_id: None,
    }];

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(provider.chat_completion(&messages, &[]));
    assert!(result.is_err());
}

#[test]
fn test_chat_message_serialization() {
    let msg = ChatMessage {
        role: MessageRole::User,
        content: "Hello".to_string(),
        tool_calls: Some(vec![ToolCall {
            id: "call_123".to_string(),
            name: "spawn_entity".to_string(),
            arguments: serde_json::json!({"position": [0, 0, 0]}),
        }]),
        tool_call_id: None,
    };

    let json = serde_json::to_string(&msg).unwrap();
    let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.role, MessageRole::User);
    assert_eq!(parsed.content, "Hello");
    assert_eq!(parsed.tool_calls.unwrap()[0].name, "spawn_entity");
}

#[test]
fn test_llm_error_display() {
    assert_eq!(
        format!("{}", LlmError::Network("timeout".to_string())),
        "Network error: timeout"
    );
    assert_eq!(
        format!("{}", LlmError::Api("rate limited".to_string())),
        "API error: rate limited"
    );
    assert_eq!(
        format!("{}", LlmError::Serialization("bad json".to_string())),
        "Serialization error: bad json"
    );
    assert_eq!(format!("{}", LlmError::Timeout), "Request timed out");
    assert_eq!(
        format!("{}", LlmError::Config("missing key".to_string())),
        "Configuration error: missing key"
    );
}

#[test]
fn test_async_bridge_submit_and_poll() {
    use katla_agent::runtime::AsyncBridge;
    use std::sync::Arc;

    let bridge = AsyncBridge::new().unwrap();
    let provider = Arc::new(MockProvider::simple("Bridge response"));

    let messages = vec![ChatMessage {
        role: MessageRole::User,
        content: "Test".to_string(),
        tool_calls: None,
        tool_call_id: None,
    }];

    let pending = bridge.submit_chat(provider, messages, vec![]);

    let mut result: Option<Result<ChatResponse, LlmError>> = None;
    for _ in 0..100 {
        if let Some(r) = pending.poll() {
            result = Some(r);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let response = result.unwrap().unwrap();
    assert_eq!(response.message.content, "Bridge response");
}

#[test]
fn test_tool_definition_serialization() {
    let tool = ToolDefinition {
        name: "spawn_entity".to_string(),
        description: "Create a new entity".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "position": {"type": "array", "items": {"type": "number"}}
            }
        }),
    };

    let json = serde_json::to_string(&tool).unwrap();
    assert!(json.contains("spawn_entity"));
    assert!(json.contains("Create a new entity"));
}
