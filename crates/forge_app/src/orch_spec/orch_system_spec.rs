use forge_domain::{ChatCompletionMessage, Content, FinishReason, Workflow};
use insta::assert_snapshot;

use crate::orch_spec::orch_runner::TestContext;

#[tokio::test]
async fn test_system_prompt() {
    let mut ctx = TestContext::default()
        .workflow(Workflow::default().tool_supported(false))
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant(Content::full("Sure"))
                .finish_reason(FinishReason::Stop),
        ]);

    ctx.run("This is a test").await.unwrap();
    let system_messages = ctx.output.system_messages().unwrap().join("\n\n");
    assert_snapshot!(system_messages);
}

#[tokio::test]
async fn test_system_prompt_tool_supported() {
    let mut ctx = TestContext::default()
        .workflow(
            Workflow::default()
                .tool_supported(true)
                .custom_rules("Do it nicely"),
        )
        .files(vec![
            forge_domain::File { path: "/users/john/foo.txt".to_string(), is_dir: false },
            forge_domain::File { path: "/users/jason/bar.txt".to_string(), is_dir: false },
        ])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant(Content::full("Sure"))
                .finish_reason(FinishReason::Stop),
        ]);

    ctx.run("This is a test").await.unwrap();

    let system_messages = ctx.output.system_messages().unwrap().join("\n\n");
    assert_snapshot!(system_messages);
}
