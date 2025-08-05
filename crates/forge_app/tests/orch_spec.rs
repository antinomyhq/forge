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
async fn test_render_system_prompt_with_custom_agent() {
    let system_prompt = async |tool_supported| -> String {
        // create custom agent with a system prompt
        let custom_prompt = "This is custom prompt for user";
        let custom_agent = Agent::new("custom-agent")
            .system_prompt(Template::new(custom_prompt))
            .model("gpt-4.1")
            .subscribe(vec!["custom-agent/user_task_init".into()])
            .tool_supported(tool_supported);

        // setup testing environment with the custom agent
        let mut setup = Setup::default().add_agent(custom_agent);

        // run a chat with the custom agent
        let _ = setup
            .chat("Hello".into(), "custom-agent".into())
            .await
            .unwrap();

        // get the system prompt from the latest context
        setup.system_prompt().await.unwrap()
    };

    let system_prompt_with_tool_supported = system_prompt(true).await;
    let system_prompt_without_tool_supported = system_prompt(false).await;
    insta::assert_snapshot!(
        "system_prompt_without_tool_supported_literal_system_prompt",
        system_prompt_without_tool_supported
    );
    insta::assert_snapshot!(
        "system_prompt_with_tool_supported_literal_system_prompt",
        system_prompt_with_tool_supported
    );
}

#[tokio::test]
async fn test_render_system_prompt_with_custom_agent_template() {
    let system_prompt = async |tool_supported| -> String {
        // configure custom agent
        let custom_prompt = "{{> custom-agent.hbs}}";
        let custom_agent_content = "Testing custom agent";
        let custom_agent = Agent::new("custom-agent")
            .system_prompt(Template::new(custom_prompt))
            .subscribe(vec!["custom-agent/user_task_init".into()])
            .model("gpt-4.1")
            .tool_supported(tool_supported);

        let mut setup = Setup::default().add_agent(custom_agent);

        // register custom agent template
        setup
            .services
            .register_template("custom-agent.hbs", custom_agent_content)
            .await
            .unwrap();

        // execute request with orchestrator
        let _ = setup
            .chat("Hello".into(), "custom-agent".into())
            .await
            .unwrap();

        // get the system prompt from the latest context
        let system_prompt = setup.system_prompt().await.unwrap();
        system_prompt
    };

    let system_prompt_with_tool_supported = system_prompt(true).await;
    let system_prompt_without_tool_supported = system_prompt(false).await;

    insta::assert_snapshot!(
        "system_prompt_without_tool_supported_hbs_template",
        system_prompt_without_tool_supported
    );
    insta::assert_snapshot!(
        "system_prompt_with_tool_supported_hbs_template",
        system_prompt_with_tool_supported
    );
}

#[tokio::test]
async fn test_render_system_prompt_default_agents() {
    let system_prompt = async |agent_id: &str, tool_supported| -> String {
        let workflow = Workflow::default();
        let agent = workflow
            .get_agent(&AgentId::new(agent_id))
            .unwrap()
            .clone()
            .tool_supported(tool_supported);

        // configure custom agent
        let mut setup = Setup::default().add_agent(agent);

        // execute request with orchestrator
        let _ = setup
            .chat("Hello".into(), agent_id.to_string())
            .await
            .unwrap();

        // get the system prompt from the latest context
        let system_prompt = setup.system_prompt().await.unwrap();
        system_prompt
    };

    let system_prompt_with_tool_supported = system_prompt("forge", true).await;
    let system_prompt_without_tool_supported = system_prompt("forge", false).await;
    insta::assert_snapshot!(
        "system_prompt_without_tool_supported_forge_agent",
        system_prompt_without_tool_supported
    );
    insta::assert_snapshot!(
        "system_prompt_with_tool_supported_forge_agent",
        system_prompt_with_tool_supported
    );

    let system_prompt_with_tool_supported = system_prompt("muse", true).await;
    let system_prompt_without_tool_supported = system_prompt("muse", false).await;
    insta::assert_snapshot!(
        "system_prompt_without_tool_supported_muse_agent",
        system_prompt_without_tool_supported
    );
    insta::assert_snapshot!(
        "system_prompt_with_tool_supported_muse_agent",
        system_prompt_with_tool_supported
    );
}
