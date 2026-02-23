use forge_domain::{ChatCompletionMessage, CommandOutput, Content, FinishReason, Workflow};
use insta::assert_snapshot;

use crate::ShellOutput;
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
    let shell_output = ShellOutput {
        output: CommandOutput {
            stdout:
                "src/main.rs\nsrc/lib.rs\nsrc/utils.rs\nREADME.md\nCargo.toml\ntests/integration.rs"
                    .to_string(),
            stderr: String::new(),
            command: "git ls-files".to_string(),
            exit_code: Some(0),
        },
        shell: "/bin/bash".to_string(),
        description: None,
    };

    let mut ctx = TestContext::default()
        .workflow(
            Workflow::default()
                .tool_supported(true)
                .custom_rules("Do it nicely"),
        )
        .files(vec![
            forge_domain::File { path: "src".to_string(), is_dir: true },
            forge_domain::File { path: "tests".to_string(), is_dir: true },
            forge_domain::File { path: "Cargo.toml".to_string(), is_dir: false },
            forge_domain::File { path: "README.md".to_string(), is_dir: false },
        ])
        .mock_shell_outputs(vec![shell_output])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant(Content::full("Sure"))
                .finish_reason(FinishReason::Stop),
        ]);

    ctx.run("This is a test").await.unwrap();

    let system_messages = ctx.output.system_messages().unwrap().join("\n\n");
    assert_snapshot!(system_messages);
}

#[tokio::test]
async fn test_system_prompt_with_extensions() {
    let shell_output = ShellOutput {
        output: CommandOutput {
            stdout: include_str!("../fixtures/git_ls_files_mixed.txt").to_string(),
            stderr: String::new(),
            command: "git ls-files".to_string(),
            exit_code: Some(0),
        },
        shell: "/bin/bash".to_string(),
        description: None,
    };

    let mut ctx = TestContext::default()
        .workflow(Workflow::default().tool_supported(true))
        .mock_shell_outputs(vec![shell_output])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant(Content::full("Sure"))
                .finish_reason(FinishReason::Stop),
        ]);

    ctx.run("This is a test").await.unwrap();

    let system_messages = ctx.output.system_messages().unwrap().join("\n\n");
    assert_snapshot!(system_messages);
}

#[tokio::test]
async fn test_system_prompt_with_extensions_truncated() {
    // Create 20 different file extensions to test truncation
    let mut files = Vec::new();
    for i in 1..=20 {
        // Each extension gets 21-i files (so ext1 has most, ext20 has least)
        for j in 0..(21 - i) {
            files.push(format!("file{}.ext{}", j, i));
        }
    }
    let stdout = files.join("\n");

    let shell_output = ShellOutput {
        output: CommandOutput {
            stdout,
            stderr: String::new(),
            command: "git ls-files".to_string(),
            exit_code: Some(0),
        },
        shell: "/bin/bash".to_string(),
        description: None,
    };

    let mut ctx = TestContext::default()
        .workflow(Workflow::default().tool_supported(true))
        .mock_shell_outputs(vec![shell_output])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant(Content::full("Sure"))
                .finish_reason(FinishReason::Stop),
        ]);

    ctx.run("This is a test").await.unwrap();

    let system_messages = ctx.output.system_messages().unwrap().join("\n\n");
    assert_snapshot!(system_messages);
}

#[tokio::test]
async fn test_system_prompt_with_files_no_extensions() {
    // Test files with no extension (common in projects like Makefile, LICENSE, etc.)
    let shell_output = ShellOutput {
        output: CommandOutput {
            stdout: "LICENSE\nREADME\nMakefile\nDockerfile\n.gitignore\nsrc/main\nsrc/lib"
                .to_string(),
            stderr: String::new(),
            command: "git ls-files".to_string(),
            exit_code: Some(0),
        },
        shell: "/bin/bash".to_string(),
        description: None,
    };

    let mut ctx = TestContext::default()
        .workflow(Workflow::default().tool_supported(true))
        .files(vec![
            forge_domain::File { path: "LICENSE".to_string(), is_dir: false },
            forge_domain::File { path: "README".to_string(), is_dir: false },
            forge_domain::File { path: "Makefile".to_string(), is_dir: false },
            forge_domain::File { path: "Dockerfile".to_string(), is_dir: false },
            forge_domain::File { path: "src".to_string(), is_dir: true },
        ])
        .mock_shell_outputs(vec![shell_output])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant(Content::full("Sure"))
                .finish_reason(FinishReason::Stop),
        ]);

    ctx.run("This is a test").await.unwrap();

    let system_messages = ctx.output.system_messages().unwrap().join("\n\n");
    assert_snapshot!(system_messages);
}

#[tokio::test]
async fn test_system_prompt_with_mixed_file_types() {
    // Test with a variety of common file types
    let shell_output = ShellOutput {
        output: CommandOutput {
            stdout: "src/index.js\nsrc/App.tsx\nsrc/styles.css\nsrc/api.go\nsrc/main.rs\ntests/test.py\ndocs/readme.md\nconfig.json\n.env\ndocker-compose.yml\npackage-lock.json\nCargo.toml\nMakefile\nREADME.md"
                .to_string(),
            stderr: String::new(),
            command: "git ls-files".to_string(),
            exit_code: Some(0),
        },
        shell: "/bin/bash".to_string(),
        description: None,
    };

    let mut ctx = TestContext::default()
        .workflow(Workflow::default().tool_supported(true))
        .files(vec![
            forge_domain::File { path: "docs".to_string(), is_dir: true },
            forge_domain::File { path: "src".to_string(), is_dir: true },
            forge_domain::File { path: "tests".to_string(), is_dir: true },
            forge_domain::File { path: ".env".to_string(), is_dir: false },
            forge_domain::File { path: "Cargo.toml".to_string(), is_dir: false },
            forge_domain::File { path: "Makefile".to_string(), is_dir: false },
            forge_domain::File { path: "README.md".to_string(), is_dir: false },
            forge_domain::File { path: "config.json".to_string(), is_dir: false },
            forge_domain::File { path: "docker-compose.yml".to_string(), is_dir: false },
            forge_domain::File { path: "package-lock.json".to_string(), is_dir: false },
        ])
        .mock_shell_outputs(vec![shell_output])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant(Content::full("Sure"))
                .finish_reason(FinishReason::Stop),
        ]);

    ctx.run("This is a test").await.unwrap();

    let system_messages = ctx.output.system_messages().unwrap().join("\n\n");
    assert_snapshot!(system_messages);
}
