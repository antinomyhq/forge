use forge_domain::{ChatCompletionMessage, Content, Workflow};
use insta::assert_snapshot;

use crate::orch_spec::orch_runner::Setup;

#[tokio::test]
async fn test_system_prompt() {
    let ctx = Setup::init_forge_task("This is a test")
        .workflow(Workflow::default())
        .mock_assistant_responses(vec![ChatCompletionMessage::assistant(Content::full(
            "Sure",
        ))])
        .run()
        .await;
    let system_prompt = ctx.system_prompt().unwrap();
    assert_snapshot!(system_prompt);
}

#[tokio::test]
async fn test_system_prompt_tool_supported() {
    let ctx = Setup::init_forge_task("This is a test")
        .workflow(
            Workflow::default()
                .tool_supported(true)
                .custom_rules("Do it nicely"),
        )
        .files(vec![
            "/users/john/foo.txt".to_string(),
            "/users/jason/bar.txt".to_string(),
        ])
        .mock_assistant_responses(vec![ChatCompletionMessage::assistant(Content::full(
            "Sure",
        ))])
        .run()
        .await;
    let system_prompt = ctx.system_prompt().unwrap();
    assert_snapshot!(system_prompt);
}
