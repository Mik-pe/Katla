#![cfg(feature = "llm-assistant")]
use katla_agent::CoCreatorAgent;
use katla_agent::co_creator::build_system_prompt;
use katla_agent::llm::MockStreamProvider;
use katla_agent::{AsyncBridge, StreamEvent};
use std::sync::Arc;

#[test]
fn test_co_creator_new() {
    let agent = CoCreatorAgent::new();
    assert!(!agent.is_streaming());
    assert!(agent.history().is_empty());
}

#[test]
fn test_build_system_prompt() {
    let prompt = build_system_prompt();
    assert!(prompt.contains("co-creator"));
    assert!(prompt.contains("spawn_entity"));
    assert!(prompt.contains("World Building"));
}

#[test]
fn test_local_request_spawn_cube() {
    let agent = CoCreatorAgent::new();
    let resp = agent.handle_local_request("spawn a cube");
    assert!(resp.text.contains("Spawned"));
    assert_eq!(resp.actions.len(), 1);
}

#[test]
fn test_local_request_spawn_sphere() {
    let agent = CoCreatorAgent::new();
    let resp = agent.handle_local_request("add a sphere");
    assert!(resp.text.contains("sphere"));
}

#[test]
fn test_local_request_help() {
    let agent = CoCreatorAgent::new();
    let resp = agent.handle_local_request("help");
    assert!(resp.text.contains("spawn a cube"));
    assert!(resp.actions.is_empty());
}

#[test]
fn test_local_request_unknown() {
    let agent = CoCreatorAgent::new();
    let resp = agent.handle_local_request("foobar");
    assert!(resp.text.contains("foobar"));
    assert!(resp.actions.is_empty());
}

#[test]
fn test_finalize_response() {
    let mut agent = CoCreatorAgent::new();
    agent.finalize_response("Hello from the assistant.");
    assert_eq!(agent.history().len(), 1);
    assert_eq!(agent.history()[0].role, katla_agent::MessageRole::Assistant);
    assert_eq!(agent.history()[0].content, "Hello from the assistant.");
}

#[test]
fn test_finalize_response_empty_ignored() {
    let mut agent = CoCreatorAgent::new();
    agent.finalize_response("");
    agent.finalize_response("   ");
    assert!(agent.history().is_empty());
}

#[test]
fn test_streaming_text_chunks() {
    let bridge = AsyncBridge::new().unwrap();
    let provider = Arc::new(MockStreamProvider::text_chunks(&["Hello", " world", "!"]));
    let mut agent = CoCreatorAgent::new();

    agent.submit_request(&bridge, provider, "{}", "Hi there");
    assert!(agent.is_streaming());

    let mut all_text = String::new();
    while agent.is_streaming() {
        let events = agent.poll_stream();
        for event in events {
            if let StreamEvent::TextDelta(delta) = event {
                all_text.push_str(&delta);
            }
        }
    }

    assert_eq!(all_text, "Hello world!");
    assert!(!agent.is_streaming());
}

#[test]
fn test_streaming_tool_call_accumulation() {
    let bridge = AsyncBridge::new().unwrap();
    let provider = Arc::new(MockStreamProvider::tool_call(
        "call_1",
        "spawn_entity",
        &serde_json::json!({"position":[1,2,3],"name":"TestCube"}),
    ));
    let mut agent = CoCreatorAgent::new();

    agent.submit_request(&bridge, provider, "{}", "spawn a cube");
    assert!(agent.is_streaming());

    let mut tool_calls = Vec::new();
    while agent.is_streaming() {
        let events = agent.poll_stream();
        for event in events {
            if let StreamEvent::ToolCall(calls) = event {
                tool_calls = calls;
            }
        }
    }

    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "call_1");
    assert_eq!(tool_calls[0].name, "spawn_entity");
    assert_eq!(
        tool_calls[0].arguments["position"],
        serde_json::json!([1, 2, 3])
    );
    assert!(agent.has_pending_tool_calls());
}

#[test]
fn test_streaming_error() {
    let bridge = AsyncBridge::new().unwrap();
    let provider = Arc::new(MockStreamProvider::error("rate limited"));
    let mut agent = CoCreatorAgent::new();

    agent.submit_request(&bridge, provider, "{}", "Hi");

    let mut errors = Vec::new();
    while agent.is_streaming() {
        let events = agent.poll_stream();
        for event in events {
            if let StreamEvent::Error(msg) = event {
                errors.push(msg);
            }
        }
    }

    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("rate limited"));
    assert!(!agent.is_streaming());
}

#[test]
fn test_streaming_truncated() {
    use katla_agent::llm::{FinishReason, StreamChunk};
    let bridge = AsyncBridge::new().unwrap();
    let provider = Arc::new(MockStreamProvider::new(vec![
        Ok(StreamChunk {
            content_delta: "partial...".to_string(),
            finish_reason: None,
            tool_call_deltas: Vec::new(),
        }),
        Ok(StreamChunk {
            content_delta: String::new(),
            finish_reason: Some(FinishReason::Length),
            tool_call_deltas: Vec::new(),
        }),
    ]));
    let mut agent = CoCreatorAgent::new();

    agent.submit_request(&bridge, provider, "{}", "Tell me a long story");

    let mut truncated = false;
    while agent.is_streaming() {
        let events = agent.poll_stream();
        for event in events {
            if matches!(event, StreamEvent::Truncated) {
                truncated = true;
            }
        }
    }

    assert!(truncated);
}

#[test]
fn test_tool_result_added_to_history() {
    let bridge = AsyncBridge::new().unwrap();
    let provider = Arc::new(MockStreamProvider::tool_call(
        "call_42",
        "spawn_entity",
        &serde_json::json!({"position":[0,0,0]}),
    ));
    let mut agent = CoCreatorAgent::new();

    agent.submit_request(&bridge, provider, "{}", "spawn something");

    while agent.is_streaming() {
        let _ = agent.poll_stream();
    }

    let full_text = String::new();
    agent.finalize_response(&full_text);

    assert!(agent.has_pending_tool_calls());
    let calls = agent.take_pending_tool_calls();
    assert_eq!(calls.len(), 1);

    agent.add_tool_result("call_42".to_string(), r#"{"success":true}"#.to_string());

    let history = agent.history();
    let tool_msg = history
        .iter()
        .find(|m| m.role == katla_agent::MessageRole::Tool);
    assert!(tool_msg.is_some());
    assert_eq!(tool_msg.unwrap().tool_call_id, Some("call_42".to_string()));
    assert_eq!(tool_msg.unwrap().content, r#"{"success":true}"#);
}

#[test]
fn test_bridge_streaming_end_to_end() {
    let bridge = AsyncBridge::new().unwrap();
    let provider = Arc::new(MockStreamProvider::text_chunks(&["chunk1", "chunk2"]));

    let messages = vec![katla_agent::ChatMessage {
        role: katla_agent::MessageRole::User,
        content: "test".to_string(),
        tool_calls: None,
        tool_call_id: None,
    }];

    let mut pending = bridge.submit_chat_stream(provider, messages, vec![]);

    let mut chunks = Vec::new();
    for _ in 0..100 {
        let received = pending.poll_chunks();
        chunks.extend(received);
        if pending.is_done() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert!(pending.is_done());
    assert_eq!(chunks.len(), 3); // "chunk1", "chunk2", finish
    assert_eq!(chunks[0].as_ref().unwrap().content_delta, "chunk1");
    assert_eq!(chunks[1].as_ref().unwrap().content_delta, "chunk2");
    assert_eq!(
        chunks[2].as_ref().unwrap().finish_reason,
        Some(katla_agent::FinishReason::Stop)
    );
}
