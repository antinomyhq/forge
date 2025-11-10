use std::sync::Arc;

use fake::{Fake, Faker};
use forge_domain::{Role, Template};

use crate::TemplateService;
use crate::domain::{Agent, Conversation};
use crate::system_prompt::SystemPrompt;

// Mock template service for testing
struct MockTemplateService {
    templates: Vec<(String, String)>,
}

impl MockTemplateService {
    fn new() -> Self {
        Self { templates: Vec::new() }
    }

    fn add_template(&mut self, name: String, content: String) {
        self.templates.push((name, content));
    }
}

#[async_trait::async_trait]
impl TemplateService for MockTemplateService {
    async fn register_template(&self, _path: std::path::PathBuf) -> anyhow::Result<()> {
        Ok(())
    }

    async fn render_template<V: serde::Serialize + Send + Sync>(
        &self,
        template: Template<V>,
        object: &V,
    ) -> anyhow::Result<String> {
        if template.template.contains("forge-custom-agent-template.md") {
            // Extract custom_rules from the context object
            let json = serde_json::to_string(object)?;
            let ctx: serde_json::Value = serde_json::from_str(&json)?;

            if let Some(custom_rules) = ctx.get("custom_rules").and_then(|v| v.as_str())
                && custom_rules.contains("Custom instruction 1") {
                    return Ok(format!(
                        "<project_guidelines>\n{}\n</project_guidelines>\n<non_negotiable_rules>\n...\n</non_negotiable_rules>",
                        custom_rules
                    ));
                }

            Ok("<non_negotiable_rules>\n...\n</non_negotiable_rules>".to_string())
        } else {
            Ok(template.template)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_system_prompt_include_agents_md_true() {
        let mut template_service = MockTemplateService::new();
        template_service.add_template(
            "test template".to_string(),
            "Test system prompt".to_string(),
        );

        let agent = Agent::new("test-agent")
            .model("test-model")
            .include_agents_md(true)
            .system_prompt(Template::new("test template"));

        let system_prompt = SystemPrompt::new(Arc::new(template_service), Faker.fake(), agent)
            .custom_instructions(vec!["Custom instruction 1".to_string()]);

        let conversation = Conversation::generate();
        let result = system_prompt
            .add_system_message(conversation)
            .await
            .unwrap();

        // Custom instructions should be included when include_agents_md is true
        let context = result.context.unwrap();
        let system_messages: Vec<_> = context
            .messages
            .iter()
            .filter(|msg| msg.has_role(Role::System))
            .collect();
        assert_eq!(system_messages.len(), 2);
        assert!(system_messages[0].to_text().contains("test template"));
        // The non-static block should contain custom instructions
        assert!(
            system_messages[1]
                .to_text()
                .contains("Custom instruction 1")
        );
    }

    #[tokio::test]
    async fn test_system_prompt_include_agents_md_false() {
        let mut template_service = MockTemplateService::new();
        template_service.add_template(
            "test template".to_string(),
            "Test system prompt".to_string(),
        );

        let agent = Agent::new("test-agent")
            .model("test-model")
            .include_agents_md(false)
            .system_prompt(Template::new("test template"));

        let system_prompt = SystemPrompt::new(Arc::new(template_service), Faker.fake(), agent)
            .custom_instructions(vec!["Custom instruction 1".to_string()]);

        let conversation = Conversation::generate();
        let result = system_prompt
            .add_system_message(conversation)
            .await
            .unwrap();

        // Custom instructions should be excluded when include_agents_md is false
        let context = result.context.unwrap();
        let system_messages: Vec<_> = context
            .messages
            .iter()
            .filter(|msg| msg.has_role(Role::System))
            .collect();
        assert_eq!(system_messages.len(), 2);
        assert!(system_messages[0].to_text().contains("test template"));
        // The non-static block should not contain custom instructions (only
        // custom_rules from agent)
        assert!(
            !system_messages[1]
                .to_text()
                .contains("Custom instruction 1")
        );
    }

    #[tokio::test]
    async fn test_system_prompt_include_agents_md_missing() {
        let mut template_service = MockTemplateService::new();
        template_service.add_template(
            "test template".to_string(),
            "Test system prompt".to_string(),
        );

        // Agent without include_agents_md field (should default to true)
        let agent = Agent::new("test-agent")
            .model("test-model")
            .system_prompt(Template::new("test template"));

        let system_prompt = SystemPrompt::new(Arc::new(template_service), Faker.fake(), agent)
            .custom_instructions(vec!["Custom instruction 1".to_string()]);

        let conversation = Conversation::generate();
        let result = system_prompt
            .add_system_message(conversation)
            .await
            .unwrap();

        // Custom instructions should be included when include_agents_md is missing
        // (defaults to true)
        let context = result.context.unwrap();
        let system_messages: Vec<_> = context
            .messages
            .iter()
            .filter(|msg| msg.has_role(Role::System))
            .collect();
        assert_eq!(system_messages.len(), 2);
        assert!(system_messages[0].to_text().contains("test template"));
        // The non-static block should contain custom instructions
        assert!(
            system_messages[1]
                .to_text()
                .contains("Custom instruction 1")
        );
    }

    #[tokio::test]
    async fn test_system_prompt_multiple_custom_instructions() {
        let mut template_service = MockTemplateService::new();
        template_service.add_template(
            "test template".to_string(),
            "Test system prompt".to_string(),
        );

        let agent = Agent::new("test-agent")
            .model("test-model")
            .include_agents_md(true)
            .system_prompt(Template::new("test template"));

        let system_prompt = SystemPrompt::new(Arc::new(template_service), Faker.fake(), agent)
            .custom_instructions(vec![
                "Custom instruction 1".to_string(),
                "Custom instruction 2".to_string(),
                "Custom instruction 3".to_string(),
            ]);

        let conversation = Conversation::generate();
        let result = system_prompt
            .add_system_message(conversation)
            .await
            .unwrap();

        // All custom instructions should be included when include_agents_md is true
        let context = result.context.unwrap();
        let system_messages: Vec<_> = context
            .messages
            .iter()
            .filter(|msg| msg.has_role(Role::System))
            .collect();
        assert_eq!(system_messages.len(), 2);
        assert!(system_messages[0].to_text().contains("test template"));
        // The non-static block should contain all custom instructions
        assert!(
            system_messages[1]
                .to_text()
                .contains("Custom instruction 1")
        );
        assert!(
            system_messages[1]
                .to_text()
                .contains("Custom instruction 2")
        );
        assert!(
            system_messages[1]
                .to_text()
                .contains("Custom instruction 3")
        );
    }

    #[tokio::test]
    async fn test_system_prompt_multiple_custom_instructions_excluded() {
        let mut template_service = MockTemplateService::new();
        template_service.add_template(
            "test template".to_string(),
            "Test system prompt".to_string(),
        );

        let agent = Agent::new("test-agent")
            .model("test-model")
            .include_agents_md(false)
            .system_prompt(Template::new("test template"));

        let system_prompt = SystemPrompt::new(Arc::new(template_service), Faker.fake(), agent)
            .custom_instructions(vec![
                "Custom instruction 1".to_string(),
                "Custom instruction 2".to_string(),
                "Custom instruction 3".to_string(),
            ]);

        let conversation = Conversation::generate();
        let result = system_prompt
            .add_system_message(conversation)
            .await
            .unwrap();

        // All custom instructions should be excluded when include_agents_md is false
        let context = result.context.unwrap();
        let system_messages: Vec<_> = context
            .messages
            .iter()
            .filter(|msg| msg.has_role(Role::System))
            .collect();
        assert_eq!(system_messages.len(), 2);
        assert!(system_messages[0].to_text().contains("test template"));
        // The non-static block should not contain any custom instructions (only
        // custom_rules from agent)
        assert!(
            !system_messages[1]
                .to_text()
                .contains("Custom instruction 1")
        );
        assert!(
            !system_messages[1]
                .to_text()
                .contains("Custom instruction 2")
        );
        assert!(
            !system_messages[1]
                .to_text()
                .contains("Custom instruction 3")
        );
    }
}
