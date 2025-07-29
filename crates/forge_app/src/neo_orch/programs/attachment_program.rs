use derive_builder::Builder;
use derive_setters::Setters;
use forge_domain::{AttachmentContent, ContextMessage, ModelId};
use forge_template::Element;

use crate::neo_orch::events::{AgentAction, AgentCommand};
use crate::neo_orch::program::Program;
use crate::neo_orch::state::AgentState;

#[derive(Setters, Builder)]
#[setters(strip_option, into)]
pub struct AttachmentProgram {
    model_id: ModelId,
}

impl Program for AttachmentProgram {
    type State = AgentState;
    type Action = AgentAction;
    type Success = AgentCommand;
    type Error = anyhow::Error;

    fn update(
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        // Only process attachments when receiving a ChatEvent action
        if let AgentAction::ChatEvent(event) = action
            && !event.attachments.is_empty()
        {
            // Get the model_id to use for context messages
            let model_id = &self.model_id;

            // Process each attachment and add it to the context
            for attachment in &event.attachments {
                let context_message = match &attachment.content {
                    AttachmentContent::Image(image) => ContextMessage::Image(image.clone()),
                    AttachmentContent::FileContent {
                        content,
                        start_line,
                        end_line,
                        total_lines,
                    } => {
                        let elm = Element::new("file_content")
                            .attr("path", &attachment.path)
                            .attr("start_line", start_line)
                            .attr("end_line", end_line)
                            .attr("total_lines", total_lines)
                            .cdata(content);

                        ContextMessage::user(elm, model_id.clone().into())
                    }
                };

                state.context = state.context.clone().add_message(context_message);
            }
        }

        Ok(AgentCommand::Empty)
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Attachment, Event, Image, TemplateId};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_builder_creates_program_with_model_id() {
        let model_id = ModelId::new("test-model");
        let fixture = AttachmentProgramBuilder::default()
            .model_id(model_id.clone())
            .build()
            .unwrap();
        let actual = &fixture.model_id;
        let expected = &model_id;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_processes_file_attachments() {
        let fixture = AttachmentProgramBuilder::default()
            .model_id(ModelId::new("test-model"))
            .build()
            .unwrap();
        let mut state = AgentState::default();

        let attachment = Attachment {
            path: "/test/file.txt".to_string(),
            content: AttachmentContent::FileContent {
                content: "test file content".to_string(),
                start_line: 1,
                end_line: 10,
                total_lines: 10,
            },
        };

        let event = Event::new("test_message", Some("test message")).attachments(vec![attachment]);
        let action = AgentAction::ChatEvent(event);

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 1;
        assert_eq!(actual_messages_count, expected_messages_count);

        let first_message = state
            .context
            .messages
            .first()
            .expect("Should have at least one message");
        match first_message {
            forge_domain::ContextMessage::Text(content_message) => {
                let actual_role = &content_message.role;
                let expected_role = &forge_domain::Role::User;
                assert_eq!(actual_role, expected_role);

                let actual_content = &content_message.content;
                assert!(actual_content.contains("file_content"));
                assert!(actual_content.contains("/test/file.txt"));
                assert!(actual_content.contains("test file content"));
            }
            _ => panic!("Expected a text message with user role"),
        }
    }

    #[test]
    fn test_update_processes_image_attachments() {
        let fixture = AttachmentProgramBuilder::default()
            .model_id(ModelId::new("test-model"))
            .build()
            .unwrap();
        let mut state = AgentState::default();

        let image = Image::new_base64(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==".to_string(),
            "image/png"
        );

        let attachment = Attachment {
            path: "/test/image.png".to_string(),
            content: AttachmentContent::Image(image.clone()),
        };

        let event = Event::new("test_message", Some("test message")).attachments(vec![attachment]);
        let action = AgentAction::ChatEvent(event);

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 1;
        assert_eq!(actual_messages_count, expected_messages_count);

        let first_message = state
            .context
            .messages
            .first()
            .expect("Should have at least one message");
        match first_message {
            forge_domain::ContextMessage::Image(image_msg) => {
                let actual_url = image_msg.url();
                let expected_url = image.url();
                assert_eq!(actual_url, expected_url);
            }
            _ => panic!("Expected an image message"),
        }
    }

    #[test]
    fn test_update_processes_multiple_attachments() {
        let fixture = AttachmentProgramBuilder::default()
            .model_id(ModelId::new("test-model"))
            .build()
            .unwrap();
        let mut state = AgentState::default();

        let file_attachment = Attachment {
            path: "/test/file.txt".to_string(),
            content: AttachmentContent::FileContent {
                content: "test file content".to_string(),
                start_line: 1,
                end_line: 10,
                total_lines: 10,
            },
        };

        let image = Image::new_base64(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==".to_string(),
            "image/png"
        );

        let image_attachment = Attachment {
            path: "/test/image.png".to_string(),
            content: AttachmentContent::Image(image),
        };

        let event = Event::new("test_message", Some("test message"))
            .attachments(vec![file_attachment, image_attachment]);
        let action = AgentAction::ChatEvent(event);

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 2;
        assert_eq!(actual_messages_count, expected_messages_count);
    }

    #[test]
    fn test_update_skips_when_no_attachments() {
        let fixture = AttachmentProgramBuilder::default()
            .model_id(ModelId::new("test-model"))
            .build()
            .unwrap();
        let mut state = AgentState::default();

        let event = Event::new("test_message", Some("test message"));
        let action = AgentAction::ChatEvent(event);

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 0;
        assert_eq!(actual_messages_count, expected_messages_count);
    }

    #[test]
    fn test_update_ignores_non_chat_event_actions() {
        let fixture = AttachmentProgramBuilder::default()
            .model_id(ModelId::new("test-model"))
            .build()
            .unwrap();
        let mut state = AgentState::default();

        let action =
            AgentAction::RenderResult { id: TemplateId::new(1020), content: "test".to_string() };

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 0;
        assert_eq!(actual_messages_count, expected_messages_count);
    }

    #[test]
    fn test_update_returns_empty_action() {
        let fixture = AttachmentProgramBuilder::default()
            .model_id(ModelId::new("test-model"))
            .build()
            .unwrap();
        let mut state = AgentState::default();

        let attachment = Attachment {
            path: "/test/file.txt".to_string(),
            content: AttachmentContent::FileContent {
                content: "test file content".to_string(),
                start_line: 1,
                end_line: 10,
                total_lines: 10,
            },
        };

        let event = Event::new("test_message", Some("test message")).attachments(vec![attachment]);
        let action = AgentAction::ChatEvent(event);

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_uses_default_model_id_when_using_builder() {
        let fixture = AttachmentProgramBuilder::default()
            .model_id(ModelId::new("default"))
            .build()
            .unwrap();
        let mut state = AgentState::default();

        let attachment = Attachment {
            path: "/test/file.txt".to_string(),
            content: AttachmentContent::FileContent {
                content: "test file content".to_string(),
                start_line: 1,
                end_line: 10,
                total_lines: 10,
            },
        };

        let event = Event::new("test_message", Some("test message")).attachments(vec![attachment]);
        let action = AgentAction::ChatEvent(event);

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 1;
        assert_eq!(actual_messages_count, expected_messages_count);
    }

    #[test]
    fn test_update_preserves_existing_context_state() {
        let fixture = AttachmentProgramBuilder::default()
            .model_id(ModelId::new("test-model"))
            .build()
            .unwrap();
        let mut state = AgentState::default();

        // Set some initial context state
        state.context = state.context.clone().max_tokens(100usize);

        let attachment = Attachment {
            path: "/test/file.txt".to_string(),
            content: AttachmentContent::FileContent {
                content: "test file content".to_string(),
                start_line: 1,
                end_line: 10,
                total_lines: 10,
            },
        };

        let event = Event::new("test_message", Some("test message")).attachments(vec![attachment]);
        let action = AgentAction::ChatEvent(event);

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        let actual_max_tokens = state.context.max_tokens;
        let expected_max_tokens = Some(100);
        assert_eq!(actual_max_tokens, expected_max_tokens);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 1;
        assert_eq!(actual_messages_count, expected_messages_count);
    }
}
