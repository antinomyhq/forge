# 🔧 Fix: Context from Shell Commands

## Problem

When a user executes a command directly in their shell (outside of forge), then asks forge to "fix it", forge doesn't have context about what command was run or what error occurred.

**Example:**
```bash
~/Downloads
❯ rm test
rm: cannot remove 'test': Is a directory

~/Downloads
❯ : fix it
# Forge doesn't know about the `rm test` command or its error output
```

## Root Cause

Forge's context system only tracks:
- Messages within the forge conversation
- Tool calls made through forge
- Files explicitly attached or referenced

Forge does NOT have access to:
- Shell history from the user's terminal
- Commands executed outside forge
- Error messages from external shell sessions

## Solution

### Option 1: Add Shell History to Context (Recommended)

Modify `user_prompt.rs` to include recent shell commands in the additional context:

```rust
// In add_additional_context function
async fn add_additional_context(
    &self,
    mut conversation: Conversation,
) -> anyhow::Result<Conversation> {
    let mut context = conversation.context.take().unwrap_or_default();

    // Add piped input if present
    if let Some(piped_input) = &self.event.additional_context {
        let piped_message = TextMessage {
            role: Role::User,
            content: piped_input.clone(),
            raw_content: None,
            tool_calls: None,
            thought_signature: None,
            reasoning_details: None,
            model: Some(self.agent.model.clone()),
            droppable: true,
        };
        context = context.add_message(ContextMessage::Text(piped_message));
    }

    // NEW: Add recent shell history for context
    // This helps forge understand what the user was doing before asking for help
    if let Ok(history) = self.get_recent_shell_history().await {
        if !history.is_empty() {
            let history_message = TextMessage {
                role: Role::User,
                content: format!("**Recent shell commands:**\n```\n{}\n```", history),
                raw_content: None,
                tool_calls: None,
                thought_signature: None,
                reasoning_details: None,
                model: Some(self.agent.model.clone()),
                droppable: true,
            };
            context = context.add_message(ContextMessage::Text(history_message));
        }
    }

    Ok(conversation.context(context))
}

// Helper function to get recent shell history
async fn get_recent_shell_history(&self) -> anyhow::Result<String> {
    // Try to read from common shell history files
    let history_paths = [
        std::path::PathBuf::from(".bash_history"),
        std::path::PathBuf::from(".zsh_history"),
        std::path::PathBuf::from(".local/share/fish/fish_history"),
    ];

    for path in &history_paths {
        let full_path = self.services.get_environment().cwd.join(path);
        if full_path.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
                // Return last 10 commands
                return Ok(content
                    .lines()
                    .rev()
                    .take(10)
                    .collect::<Vec<_>>()
                    .rev()
                    .join("\n"));
            }
        }
    }

    Ok(String::new())
}
```

### Option 2: Improve Error Messages

Update the system prompt to guide forge when it lacks context:

**Add to system prompt template:**
```markdown
## Handling Vague Requests

If the user says "fix it", "this doesn't work", or similar without clear context:

1. **Ask for clarification**: "Could you share what command you ran and what error you saw?"
2. **Offer to investigate**: "Would you like me to explore the current directory to understand the issue?"
3. **Suggest common scenarios**: "Are you trying to remove a directory? If so, use `rm -r <dir>` instead of `rm <dir>`."

Example response:
"I'd be happy to help! Could you share:
- What command did you run?
- What error message did you see?
- What are you trying to accomplish?

Or I can explore the current directory to investigate. Which would you prefer?"
```

### Option 3: Add `:context` Command

Create a new forge command that explicitly captures recent shell context:

```rust
// In commands/ directory
{
  "name": "context",
  "description": "Show recent shell commands and current directory state",
  "template": "Recent shell activity:\n- Last 5 commands from history\n- Current directory: {{cwd}}\n- Recent files modified\n\nUser request: {{parameters}}"
}
```

User workflow:
```bash
❯ rm test
rm: cannot remove 'test': Is a directory

❯ forge :context fix it
# Now forge has context about the shell session
```

## Implementation Priority

1. **Option 2** (Improve error messages) - Easiest, immediate improvement
2. **Option 1** (Shell history) - Best UX, requires code changes
3. **Option 3** (New command) - Good middle ground

## Testing

### Test Case 1: Vague Request
**Input:** "fix it"
**Expected:** Forge asks for clarification about what needs fixing

### Test Case 2: With Shell History
**Input:** "fix it" (after `rm test` in history)
**Expected:** Forge mentions "I see you tried to run `rm test`. Are you trying to remove a directory?"

### Test Case 3: Clear Request
**Input:** "remove the test directory"
**Expected:** Forge suggests `rm -r test` or `rmdir test`

## Files to Modify

- `crates/forge_app/src/user_prompt.rs` - Add shell history to context
- `crates/forge_app/src/system_prompt.rs` - Add guidance for vague requests
- `crates/forge_app/templates/forge-system-prompt.md` - Update template
- `crates/forge_main/src/cli.rs` - Add `:context` command (Option 3)

## Security Considerations

- Shell history may contain sensitive information (passwords, tokens)
- Only read history files, never write
- Make shell history feature opt-in or clearly visible to user
- Consider filtering common sensitive patterns (PASSWORD=, API_KEY=, etc.)

---

**Time Estimate:** 2-3 hours
**Difficulty:** Medium
**Bounty Value:** $100
