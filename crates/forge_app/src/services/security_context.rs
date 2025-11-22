use std::path::PathBuf;

use forge_domain::AgentId;

/// Context for determining security permissions for inline command processing
#[derive(Debug, Clone)]
pub enum PromptContext {
    System,
    CommandGeneration,
    GitOperation,
    CustomCommand,
    WorkflowCommand,
    Agent(AgentId), /* Added for sub-agent calls
                     * UserPrompt intentionally omitted - parsed in UI */
}

/// Security context for inline command processing
#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub prompt_context: PromptContext,
    pub cwd: PathBuf,
    pub restricted: bool,
    pub allowed_commands: Option<Vec<String>>,
    pub depth_limit: Option<u8>, // For agent delegation
}

impl SecurityContext {
    pub fn new(
        prompt_context: PromptContext,
        cwd: PathBuf,
        restricted: bool,
        allowed_commands: Option<Vec<String>>,
    ) -> Self {
        Self {
            prompt_context,
            cwd,
            restricted,
            allowed_commands,
            depth_limit: None,
        }
    }

    pub fn with_depth_limit(mut self, depth_limit: u8) -> Self {
        self.depth_limit = Some(depth_limit);
        self
    }
}

/// Security rules for different prompt contexts
impl SecurityContext {
    /// Create security context for system prompts
    pub fn system(cwd: PathBuf) -> Self {
        Self::new(
            PromptContext::System,
            cwd,
            true,
            Some(vec![
                "git".to_string(),
                "cat".to_string(),
                "ls".to_string(),
                "find".to_string(),
            ]),
        )
    }

    /// Create security context for command generation
    pub fn command_generation(cwd: PathBuf) -> Self {
        Self::new(
            PromptContext::CommandGeneration,
            cwd,
            true,
            Some(vec![
                "which".to_string(),
                "type".to_string(),
                "man".to_string(),
            ]),
        )
    }

    /// Create security context for git operations
    pub fn git_operation(cwd: PathBuf) -> Self {
        Self::new(
            PromptContext::GitOperation,
            cwd,
            true,
            Some(vec!["git".to_string()]),
        )
    }

    /// Create security context for custom commands
    pub fn custom_command(cwd: PathBuf, allowed_commands: Vec<String>) -> Self {
        Self::new(
            PromptContext::CustomCommand,
            cwd,
            true,
            Some(allowed_commands),
        )
    }

    /// Create security context for workflow commands
    pub fn workflow_command(cwd: PathBuf, allowed_commands: Vec<String>) -> Self {
        Self::new(
            PromptContext::WorkflowCommand,
            cwd,
            true,
            Some(allowed_commands),
        )
    }

    /// Create security context for agent delegation
    pub fn agent_delegation(
        cwd: PathBuf,
        agent_id: AgentId,
        allowed_commands: Vec<String>,
    ) -> Self {
        Self::new(
            PromptContext::Agent(agent_id),
            cwd,
            true,
            Some(allowed_commands),
        )
        .with_depth_limit(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_context_system() {
        let context = SecurityContext::system(PathBuf::from("/test"));
        assert!(matches!(context.prompt_context, PromptContext::System));
        assert!(context.restricted);
        let allowed_commands = context.allowed_commands.unwrap();
        assert_eq!(allowed_commands.len(), 4);
        assert!(allowed_commands.contains(&"git".to_string()));
    }

    #[test]
    fn test_security_context_command_generation() {
        let context = SecurityContext::command_generation(PathBuf::from("/test"));
        assert!(matches!(
            context.prompt_context,
            PromptContext::CommandGeneration
        ));
        assert!(context.restricted);
        assert_eq!(context.allowed_commands.unwrap().len(), 3);
    }

    #[test]
    fn test_security_context_git_operation() {
        let context = SecurityContext::git_operation(PathBuf::from("/test"));
        assert!(matches!(
            context.prompt_context,
            PromptContext::GitOperation
        ));
        assert!(context.restricted);
        assert_eq!(context.allowed_commands.unwrap(), vec!["git".to_string()]);
    }

    #[test]
    fn test_security_context_custom_command() {
        let allowed = vec!["echo".to_string(), "pwd".to_string()];
        let context = SecurityContext::custom_command(PathBuf::from("/test"), allowed.clone());
        assert!(matches!(
            context.prompt_context,
            PromptContext::CustomCommand
        ));
        assert!(context.restricted);
        assert_eq!(context.allowed_commands.unwrap(), allowed);
    }

    #[test]
    fn test_security_context_agent_delegation() {
        let agent_id = AgentId::new("test-agent");
        let allowed = vec!["ls".to_string()];
        let context = SecurityContext::agent_delegation(
            PathBuf::from("/test"),
            agent_id.clone(),
            allowed.clone(),
        );

        assert!(matches!(context.prompt_context, PromptContext::Agent(id) if id == agent_id));
        assert!(context.restricted);
        assert_eq!(context.allowed_commands.unwrap(), allowed);
        assert_eq!(context.depth_limit, Some(3));
    }

    #[test]
    fn test_security_context_with_depth_limit() {
        let context = SecurityContext::system(PathBuf::from("/test")).with_depth_limit(5);
        assert_eq!(context.depth_limit, Some(5));
    }
}
