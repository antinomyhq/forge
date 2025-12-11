    /// Delegate rename operation to shell-plugin for better input handling
    async fn delegate_to_shell_plugin_for_rename(
        &mut self,
        conversation_id: &ConversationId,
        conversation: &Conversation,
    ) -> anyhow::Result<()> {
        eprintln!("DEBUG: Delegating rename to shell-plugin");
        
        // Get current title for display
        let current_title = conversation.title.as_deref().unwrap_or("<untitled>");
        
        // Show current info and instructions
        self.writeln_title(TitleFormat::info(format!(
            "Current conversation: {} ({})",
            current_title, conversation_id
        )))?;
        
        self.writeln_title(TitleFormat::info(
            "To rename this conversation, use your shell with forge plugin:"
        ))?;
        
        self.writeln_title(TitleFormat::info(format!(
            "  forge conversation rename {} \"new title\"",
            conversation_id
        ))?;
        
        self.writeln_title(TitleFormat::info(
            "Or set FORGE_INPUT environment variable:"
        ))?;
        
        self.writeln_title(TitleFormat::info(format!(
            "  FORGE_INPUT=\"new title\" forge conversation rename {}",
            conversation_id
        )))?;
        
        Ok(())
    }
}

impl ForgeUI {
    pub fn new(infra: Arc<dyn Infrastructure>) -> Self {