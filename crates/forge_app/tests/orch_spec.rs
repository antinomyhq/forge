use forge_domain::{Agent, ChatCompletionMessage, Content, Template};

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
    // create custom agent with a system prompt
    let custom_prompt = "This is custom prompt for user";
    let custom_agent = Agent::new("custom-agent")
        .system_prompt(Template::new(custom_prompt))
        .model("gpt-4.1")
        .subscribe(vec!["custom-agent/user_task_init".into()]);

    // setup testing environment with the custom agent
    let mut setup = Setup::new(vec![ChatCompletionMessage::assistant(Content::full(
        "Hey, what's up?",
    ))])
    .add_agent(custom_agent);

    // run a chat with the custom agent
    let _ = setup
        .chat("Hello".into(), "custom-agent".into())
        .await
        .unwrap();

    let history = setup.services.get_history().await;
    insta::assert_snapshot!(
        history
            .last()
            .as_ref()
            .unwrap()
            .context
            .as_ref()
            .unwrap()
            .to_text()
    );
}

#[tokio::test]
async fn test_render_system_prompt_with_custom_agent_template() {
    // configure custom agent
    let custom_prompt = "{{> custom-agent.hbs}}";
    let custom_agent_content = "Testing custom agent";
    let custom_agent = Agent::new("custom-agent")
        .system_prompt(Template::new(custom_prompt))
        .subscribe(vec!["custom-agent/user_task_init".into()])
        .model("gpt-4.1");

    let mut setup = Setup::new(vec![ChatCompletionMessage::assistant(Content::full(
        "Hey, what's up?",
    ))]).add_agent(custom_agent);
    setup.services.register_template("custom-agent.hbs", custom_agent_content).await.unwrap();

    let _ = setup.chat("Hello".into(), "custom-agent".into()).await.unwrap();
    let history = setup.services.get_history().await;
    insta::assert_snapshot!(
        history
            .last()
            .as_ref()
            .unwrap()
            .context
            .as_ref()
            .unwrap()
            .to_text()
    );
}
