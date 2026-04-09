use katla_agent::CoCreatorAgent;
use katla_agent::co_creator::build_system_prompt;

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
