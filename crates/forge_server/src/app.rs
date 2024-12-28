use derive_more::derive::From;
use derive_setters::Setters;
use forge_prompt::Prompt;
use forge_provider::{
    FinishReason, Message, ModelId, Request, Response, ToolResult, ToolUse, ToolUsePart,
};
use serde::Serialize;

use crate::runtime::Application;
use crate::template::MessageTemplate;
use crate::Result;

#[derive(Debug, From)]
pub enum Action {
    UserMessage(ChatRequest),
    FileReadResponse(Vec<FileResponse>),
    AssistantResponse(Response),
    ToolResponse(ToolResult),
}

#[derive(Debug, Clone)]
pub struct FileResponse {
    pub path: String,
    pub content: String,
}

#[derive(Default, Debug, serde::Deserialize, Clone, Setters)]
#[setters(into)]
pub struct ChatRequest {
    pub message: String,
    pub model: ModelId,
}

#[derive(Debug, Clone, PartialEq, derive_more::From)]
pub enum Command {
    #[from(ignore)]
    FileRead(Vec<String>),
    AssistantMessage(#[from] Request),
    UserMessage(#[from] ChatResponse),
    ToolUse(#[from] ToolUse),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChatResponse {
    Text(String),
    ToolUseStart(ToolUsePart),
    ToolUseEnd(ToolResult),
    Complete,
    Fail(String),
}

#[derive(Default, Debug, Clone, Serialize, Setters)]
#[serde(rename_all = "camelCase")]
#[setters(strip_option)]
pub struct App {
    // The main objective that the user is trying to achieve
    pub user_objective: Option<MessageTemplate>,

    // A temp buffer used to store the assistant response (streaming mode only)
    pub assistant_buffer: String,

    // A temp buffer used to store the tool use parts (streaming mode only)
    pub tool_use_part: Vec<ToolUsePart>,

    // Keep context at the end so that debugging the Serialized format is easier
    pub context: Request,
}

impl App {
    pub fn new(context: Request) -> Self {
        Self {
            context,
            user_objective: None,
            tool_use_part: Vec::new(),
            assistant_buffer: "".to_string(),
        }
    }
}

impl Application for App {
    type Action = Action;
    type Error = crate::Error;
    type Command = Command;

    fn update(mut self, action: impl Into<Action>) -> Result<(Self, Vec<Command>)> {
        let action = action.into();
        let mut commands = Vec::new();
        match action {
            Action::UserMessage(chat) => {
                let prompt = Prompt::parse(chat.message.clone())
                    .unwrap_or(Prompt::new(chat.message.clone()));

                self.context = self.context.model(chat.model.clone());

                if self.user_objective.is_none() {
                    self.user_objective = Some(MessageTemplate::task(prompt.to_string()));
                }

                if prompt.files().is_empty() {
                    self.context = self.context.add_message(Message::user(chat.message));
                    commands.push(Command::AssistantMessage(self.context.clone()))
                } else {
                    commands.push(Command::FileRead(prompt.files()))
                }
            }
            Action::FileReadResponse(files) => {
                if let Some(message) = self.user_objective.clone() {
                    for fr in files.into_iter() {
                        self.context = self.context.add_message(
                            message
                                .clone()
                                .append(MessageTemplate::file(fr.path, fr.content)),
                        );
                    }

                    commands.push(Command::AssistantMessage(self.context.clone()))
                }
            }
            Action::AssistantResponse(response) => {
                self.assistant_buffer
                    .push_str(response.message.content.as_str());

                if response.finish_reason.is_some() {
                    self.context = self
                        .context
                        .add_message(Message::assistant(self.assistant_buffer.clone()));
                    self.assistant_buffer.clear();
                }

                if !response.tool_use.is_empty() && self.tool_use_part.is_empty() {
                    if let Some(tool_use_part) = response.tool_use.first() {
                        commands.push(Command::UserMessage(ChatResponse::ToolUseStart(
                            tool_use_part.clone(),
                        )))
                    }
                }

                self.tool_use_part.extend(response.tool_use);

                if let Some(FinishReason::ToolUse) = response.finish_reason {
                    let tool_use = ToolUse::try_from_parts(self.tool_use_part.clone())?;
                    self.tool_use_part.clear();

                    // since tools is used, clear the tool_raw_arguments.
                    commands.push(Command::ToolUse(tool_use));
                }

                commands.push(Command::UserMessage(ChatResponse::Text(
                    response.message.content,
                )));
            }
            Action::ToolResponse(tool_result) => {
                let message = if tool_result.is_error {
                    format!(
                        "An error occurred while processing the tool, {}",
                        tool_result.tool_name.as_str()
                    )
                } else {
                    format!(
                        "TOOL Result for {}\n{}",
                        tool_result.tool_name.as_str(),
                        tool_result.content
                    )
                };

                self.context = self
                    .context
                    .add_message(Message::user(message))
                    .add_tool_result(tool_result.clone());

                commands.push(Command::AssistantMessage(self.context.clone()));
                commands.push(Command::UserMessage(ChatResponse::ToolUseEnd(tool_result)));
            }
        };
        Ok((self, commands))
    }
}

#[cfg(test)]
mod tests {
    use forge_provider::{Message, UseId};
    use forge_tool::ToolName;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::template::Tag;

    #[test]
    fn test_user_message_action() {
        let app = App::default();

        let chat_request = ChatRequest {
            message: "Hello, world!".to_string(),
            model: ModelId::default(),
        };

        let action = Action::UserMessage(chat_request.clone());
        let (app, command) = app.update(action).unwrap();

        assert_eq!(&app.context.model, &ModelId::default());
        assert!(command.contains(&Command::AssistantMessage(app.context.clone())));
    }

    #[test]
    fn test_file_load_response_action() {
        let app = App::default().user_objective(MessageTemplate::new(
            Tag { name: "test".to_string(), attributes: vec![] },
            "Test message".to_string(),
        ));

        let files = vec![FileResponse {
            path: "test_path.txt".to_string(),
            content: "Test content".to_string(),
        }];

        let action = Action::FileReadResponse(files.clone());
        let (updated_app, command) = app.update(action).unwrap();

        assert!(updated_app.context.messages[0]
            .content()
            .contains(&files[0].path));
        assert!(updated_app.context.messages[0]
            .content()
            .contains(&files[0].content));

        assert!(command.contains(&Command::AssistantMessage(updated_app.context.clone())));
    }

    #[test]
    fn test_assistant_response_action_with_tool_use() {
        let app = App::default();

        let response = Response {
            message: Message::assistant("Tool response"),
            tool_use: vec![forge_provider::ToolUsePart {
                use_id: None,
                name: Some(ToolName::from("test_tool")),
                argument_part: r#"{"key": "value"}"#.to_string(),
            }],
            finish_reason: Some(FinishReason::ToolUse),
        };

        let action = Action::AssistantResponse(response);
        let (_, command) = app.update(action).unwrap();

        assert!(command.contains(&Command::UserMessage(ChatResponse::Text(
            "Tool response".to_string()
        ))));

        assert!(command.contains(&Command::ToolUse(forge_provider::ToolUse {
            use_id: None,
            name: ToolName::from("test_tool"),
            arguments: json!({"key": "value"}),
        })));
    }

    #[test]
    fn test_tool_response_action() {
        let app = App::default();

        let tool_response = json!({
            "key": "value",
            "nested": {
                "key": "value"
            }
        });
        let tool_result = ToolResult {
            tool_use_id: None,
            tool_name: ToolName::from("test_tool"),
            content: tool_response.clone(),
            is_error: false,
        };
        let action = Action::ToolResponse(tool_result.clone());

        let (app, command) = app.update(action).unwrap();

        assert_eq!(
            app.context.messages[0].content(),
            format!(
                "{}\n{}",
                "TOOL Result for test_tool", r#"{"key":"value","nested":{"key":"value"}}"#
            )
        );

        assert!(command.contains(&Command::AssistantMessage(app.context.clone())));
        assert!(command.contains(&Command::UserMessage(
            ChatResponse::ToolUseEnd(tool_result,)
        )));
    }

    #[test]
    fn test_use_tool_when_finish_reason_present() {
        let app = App::default();
        let response = Response {
            message: Message::assistant("Tool response"),
            tool_use: vec![forge_provider::ToolUsePart {
                use_id: Some(UseId::from("test_use_id")),
                name: Some(ToolName::from("fs_list")),
                argument_part: r#"{"path": "."}"#.to_string(),
            }],
            finish_reason: Some(FinishReason::ToolUse),
        };

        let action = Action::AssistantResponse(response);
        let (app, command) = app.update(action).unwrap();

        assert!(app.tool_use_part.is_empty());

        assert!(command.contains(&Command::ToolUse(forge_provider::ToolUse {
            use_id: Some(UseId::from("test_use_id")),
            name: ToolName::from("fs_list"),
            arguments: json!({"path": "."}),
        })));

        assert!(command.contains(&Command::UserMessage(ChatResponse::Text(
            "Tool response".to_string()
        ))));
    }

    #[test]
    fn test_should_not_use_tool_when_finish_reason_not_present() {
        let app = App::default();
        let action = Action::AssistantResponse(Response {
            message: Message::assistant("Tool response"),
            tool_use: vec![forge_provider::ToolUsePart {
                use_id: None,
                name: Some(ToolName::from("fs_list")),
                argument_part: r#"{"path": "."}"#.to_string(),
            }],
            finish_reason: None,
        });
        let (app, command) = app.update(action).unwrap();

        assert!(!app.tool_use_part.is_empty());
        assert!(command.contains(&Command::UserMessage(ChatResponse::Text(
            "Tool response".to_string()
        ))));
    }

    #[test]
    fn test_should_set_user_objective_only_once() {
        let app = App::default();
        let request_0 = ChatRequest::default().message("Hello");
        let request_1 = ChatRequest::default().message("World");

        let (app, _) = app.update(Action::UserMessage(request_0)).unwrap();
        let (app, _) = app.update(Action::UserMessage(request_1)).unwrap();

        assert_eq!(
            app.user_objective,
            Some(MessageTemplate::task("Hello".to_string()))
        );

        assert_ne!(
            app.user_objective,
            Some(MessageTemplate::task("World".to_string()))
        );

        assert_eq!(app.context.messages.len(), 2);
        assert_eq!(app.context.messages[0].content(), "Hello");
        assert_eq!(app.context.messages[1].content(), "World");
    }
    #[test]
    fn test_should_not_set_user_objective_if_already_set() {
        let app = App::default().user_objective(MessageTemplate::task("Initial Objective".to_string()));
        let request = ChatRequest::default().message("New Objective");

        let (app, _) = app.update(Action::UserMessage(request)).unwrap();

        assert_eq!(
            app.user_objective,
            Some(MessageTemplate::task("Initial Objective".to_string()))
        );
    }

    #[test]
    fn test_should_handle_file_read_response_with_multiple_files() {
        let app = App::default().user_objective(MessageTemplate::new(
            Tag { name: "test".to_string(), attributes: vec![] },
            "Test message".to_string(),
        ));

        let files = vec![
            FileResponse {
                path: "file1.txt".to_string(),
                content: "Content 1".to_string(),
            },
            FileResponse {
                path: "file2.txt".to_string(),
                content: "Content 2".to_string(),
            },
        ];

        let action = Action::FileReadResponse(files.clone());
        let (updated_app, command) = app.update(action).unwrap();

        assert!(updated_app.context.messages[0]
            .content()
            .contains(&files[0].path));
        assert!(updated_app.context.messages[0]
            .content()
            .contains(&files[0].content));
        assert!(updated_app.context.messages[1]
            .content()
            .contains(&files[1].path));
        assert!(updated_app.context.messages[1]
            .content()
            .contains(&files[1].content));

        assert!(command.contains(&Command::AssistantMessage(updated_app.context.clone())));
    }

    #[test]
    fn test_should_handle_assistant_response_with_no_tool_use() {
        let app = App::default();

        let response = Response {
            message: Message::assistant("Assistant response"),
            tool_use: vec![],
            finish_reason: Some(FinishReason::EndTurn),
        };

        let action = Action::AssistantResponse(response);
        let (app, command) = app.update(action).unwrap();

        assert!(app.tool_use_part.is_empty());
        assert!(command.contains(&Command::UserMessage(ChatResponse::Text(
            "Assistant response".to_string()
        ))));
    }

    #[test]
    fn test_should_handle_tool_response_with_error() {
        let app = App::default();

        let tool_result = ToolResult {
            tool_use_id: None,
            tool_name: ToolName::from("test_tool"),
            content: json!({"error": "Something went wrong"}),
            is_error: true,
        };
        let action = Action::ToolResponse(tool_result.clone());

        let (app, command) = app.update(action).unwrap();

        assert_eq!(
            app.context.messages[0].content(),
            "An error occurred while processing the tool, test_tool"
        );

        assert!(command.contains(&Command::AssistantMessage(app.context.clone())));
        assert!(command.contains(&Command::UserMessage(
            ChatResponse::ToolUseEnd(tool_result)
        )));
    }
}
