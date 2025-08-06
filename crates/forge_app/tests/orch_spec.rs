use forge_domain::{Agent, AgentId, Template, Workflow};

mod orchestrator_test_helpers;

use crate::orchestrator_test_helpers::Setup;

#[tokio::test]
async fn test_orchestrator_creation() {
    let mut setup = Setup::default();
    let _ = setup.chat("Hello".into(), "forge".into()).await;
    assert!(true);
}

#[tokio::test]
async fn test_history_is_saved() {
    let mut setup = Setup::default();
    let _ = setup.chat("Hello".into(), "forge".into()).await;
    let actual = setup.services.get_history().await;
    assert!(!actual.is_empty());
}

#[tokio::test]
async fn test_render_system_prompt_with_custom_agent_tool_supported() {
    let fixture = Agent::new("custom-agent")
        .system_prompt(Template::new("This is custom prompt for user"))
        .model("gpt-4.1")
        .subscribe(vec!["custom-agent/user_task_init".into()])
        .tool_supported(true);

    let mut setup = Setup::default().add_agent(fixture);
    let _ = setup
        .chat("Hello".into(), "custom-agent".into())
        .await
        .unwrap();

    let actual = setup.system_prompt().await.unwrap();
    insta::assert_snapshot!(
        "system_prompt_with_tool_supported_literal_system_prompt",
        actual
    );
}

#[tokio::test]
async fn test_render_system_prompt_with_custom_agent_tool_not_supported() {
    let fixture = Agent::new("custom-agent")
        .system_prompt(Template::new("This is custom prompt for user"))
        .model("gpt-4.1")
        .subscribe(vec!["custom-agent/user_task_init".into()])
        .tool_supported(false);

    let mut setup = Setup::default().add_agent(fixture);
    let _ = setup
        .chat("Hello".into(), "custom-agent".into())
        .await
        .unwrap();

    let actual = setup.system_prompt().await.unwrap();
    insta::assert_snapshot!(
        "system_prompt_without_tool_supported_literal_system_prompt",
        actual
    );
}

#[tokio::test]
async fn test_render_system_prompt_with_custom_agent_template_tool_supported() {
    let fixture = Agent::new("custom-agent")
        .system_prompt(Template::new("{{> custom-agent.hbs}}"))
        .subscribe(vec!["custom-agent/user_task_init".into()])
        .model("gpt-4.1")
        .tool_supported(true);

    let mut setup = Setup::default().add_agent(fixture);
    setup
        .services
        .register_template("custom-agent.hbs", "Testing custom agent")
        .await
        .unwrap();

    let _ = setup
        .chat("Hello".into(), "custom-agent".into())
        .await
        .unwrap();

    let actual = setup.system_prompt().await.unwrap();
    insta::assert_snapshot!("system_prompt_with_tool_supported_hbs_template", actual);
}

#[tokio::test]
async fn test_render_system_prompt_with_custom_agent_template_tool_not_supported() {
    let fixture = Agent::new("custom-agent")
        .system_prompt(Template::new("{{> custom-agent.hbs}}"))
        .subscribe(vec!["custom-agent/user_task_init".into()])
        .model("gpt-4.1")
        .tool_supported(false);

    let mut setup = Setup::default().add_agent(fixture);
    setup
        .services
        .register_template("custom-agent.hbs", "Testing custom agent")
        .await
        .unwrap();

    let _ = setup
        .chat("Hello".into(), "custom-agent".into())
        .await
        .unwrap();

    let actual = setup.system_prompt().await.unwrap();
    insta::assert_snapshot!("system_prompt_without_tool_supported_hbs_template", actual);
}

#[tokio::test]
async fn test_render_system_prompt_forge_agent_tool_supported() {
    let workflow = Workflow::default();
    let fixture = workflow
        .get_agent(&AgentId::new("forge"))
        .unwrap()
        .clone()
        .tool_supported(true);

    let mut setup = Setup::default().add_agent(fixture);
    let _ = setup
        .chat("Hello".into(), "forge".to_string())
        .await
        .unwrap();

    let actual = setup.system_prompt().await.unwrap();
    insta::assert_snapshot!("system_prompt_with_tool_supported_forge_agent", actual);
}

#[tokio::test]
async fn test_render_system_prompt_forge_agent_tool_not_supported() {
    let workflow = Workflow::default();
    let fixture = workflow
        .get_agent(&AgentId::new("forge"))
        .unwrap()
        .clone()
        .tool_supported(false);

    let mut setup = Setup::default().add_agent(fixture);
    let _ = setup
        .chat("Hello".into(), "forge".to_string())
        .await
        .unwrap();

    let actual = setup.system_prompt().await.unwrap();
    insta::assert_snapshot!("system_prompt_without_tool_supported_forge_agent", actual);
}

#[tokio::test]
async fn test_render_system_prompt_muse_agent_tool_supported() {
    let workflow = Workflow::default();
    let fixture = workflow
        .get_agent(&AgentId::new("muse"))
        .unwrap()
        .clone()
        .tool_supported(true);

    let mut setup = Setup::default().add_agent(fixture);
    let _ = setup
        .chat("Hello".into(), "muse".to_string())
        .await
        .unwrap();

    let actual = setup.system_prompt().await.unwrap();
    insta::assert_snapshot!("system_prompt_with_tool_supported_muse_agent", actual);
}

#[tokio::test]
async fn test_render_system_prompt_muse_agent_tool_not_supported() {
    let workflow = Workflow::default();
    let fixture = workflow
        .get_agent(&AgentId::new("muse"))
        .unwrap()
        .clone()
        .tool_supported(false);

    let mut setup = Setup::default().add_agent(fixture);
    let _ = setup
        .chat("Hello".into(), "muse".to_string())
        .await
        .unwrap();

    let actual = setup.system_prompt().await.unwrap();
    insta::assert_snapshot!("system_prompt_without_tool_supported_muse_agent", actual);
}
