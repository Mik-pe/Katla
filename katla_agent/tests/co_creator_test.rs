use katla_agent::CoCreatorAgent;
use katla_agent::co_creator::build_system_prompt;
use katla_ecs::agent::{Agent, Observation};
use katla_ecs::scene_tool::SceneOp;

#[test]
fn test_co_creator_new() {
    let agent = CoCreatorAgent::new("Test system prompt");
    assert!(agent.pending_request().is_none());
    assert_eq!(agent.messages().len(), 1);
    assert_eq!(agent.messages()[0].0, "system");
    assert_eq!(agent.messages()[0].1, "Test system prompt");
}

#[test]
fn test_co_creator_submit_request() {
    let mut agent = CoCreatorAgent::new("Test");
    assert!(agent.pending_request().is_none());

    agent.submit_request("Add a light at origin");
    assert_eq!(agent.pending_request(), Some("Add a light at origin"));
}

#[test]
fn test_co_creator_observe() {
    let mut agent = CoCreatorAgent::new("Test");
    let obs = Observation {
        scene_summary: "Scene: 5 entities. Transform: 3, Light: 2".to_string(),
        entity_count: 5,
        last_action_result: None,
    };
    agent.observe(&obs);
    assert_eq!(agent.messages().len(), 2);
    assert_eq!(agent.messages()[1].0, "user");
    assert!(agent.messages()[1].1.contains("5 entities"));
}

#[test]
fn test_co_creator_decide_returns_none_initially() {
    let mut agent = CoCreatorAgent::new("Test");
    let result = agent.decide();
    assert!(result.is_none());
}

#[test]
fn test_co_creator_on_result() {
    let mut agent = CoCreatorAgent::new("Test");
    let action = katla_ecs::agent::AgentAction {
        id: katla_ecs::agent::ActionId(0),
        operation: SceneOp::GetSceneHierarchy,
        result: Some(katla_ecs::scene_tool::ToolResult {
            success: true,
            message: "Scene has 3 entities".to_string(),
            affected_entities: Vec::new(),
        }),
        error: None,
    };
    agent.on_result(&action);
    assert_eq!(agent.messages().len(), 2);
    assert_eq!(agent.messages()[1].0, "tool");
    assert!(agent.messages()[1].1.contains("Scene has 3 entities"));
}

#[test]
fn test_build_system_prompt() {
    let prompt = build_system_prompt();
    assert!(prompt.contains("co-creator"));
    assert!(prompt.contains("spawn_entity"));
    assert!(prompt.contains("World Building"));
}
