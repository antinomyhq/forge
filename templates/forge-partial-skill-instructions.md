{{!--
================================================================================
DEPRECATED: This partial is no longer used by any built-in agent.

Since Phase 0 of the Claude Code plugins integration (2026-04), the list of
available skills is delivered to the LLM per-turn as a `<system_reminder>`
user-role message produced by the `SkillListingHandler` lifecycle hook
(see `crates/forge_app/src/hooks/skill_listing.rs`). This means:

  - All agents (forge, sage, muse, and any user-defined agent) now discover
    skills automatically without needing to include this partial.
  - New skills created mid-session (e.g. via the `create-skill` workflow)
    become visible on the next turn without requiring a process restart,
    because `SkillCacheInvalidator` clears the skill cache whenever a
    `SKILL.md` file under a `skills/` directory is written or removed.

This file is retained ONLY for backward compatibility with user-authored
custom agent templates that still `{{> forge-partial-skill-instructions.md }}`.
Because `SystemContext.skills` is now always empty at runtime, the
`<available_skills>` block below will silently render as empty for any
template that still uses it — the legacy text above will still be visible
but will not list any skills.

**If you maintain a custom agent template that includes this partial,
please remove the include.** This file will be deleted in a future release.

Note: this comment block uses Handlebars `{{!-- --}}` syntax so it is
stripped at render time and never leaks into the LLM's system prompt.
================================================================================
--}}
## Skill Instructions:

**CRITICAL**: Before attempting any task, ALWAYS check if a skill exists for it in the available_skills list below. Skills are specialized workflows that must be invoked when their trigger conditions match the user's request.

How skills work:

1. **Invocation**: Use the `skill` tool with just the skill name parameter

   - Example: Call skill tool with `{"name": "mock-calculator"}`
   - No additional arguments needed

2. **Response**: The tool returns the skill's details wrapped in `<skill_details>` containing:

   - `<command path="..."><![CDATA[...]]></command>` - The complete SKILL.md file content with the skill's path
   - `<resource>` tags - List of additional resource files available in the skill directory
   - Includes usage guidelines, instructions, and any domain-specific knowledge

3. **Action**: Read and follow the instructions provided in the skill content
   - The skill instructions will tell you exactly what to do and how to use the resources
   - Some skills provide workflows, others provide reference information
   - Apply the skill's guidance to complete the user's task

Examples of skill invocation:

- To invoke calculator skill: use skill tool with name "calculator"
- To invoke weather skill: use skill tool with name "weather"
- For namespaced skills: use skill tool with name "office-suite:pdf"

Important:

- Only invoke skills listed in `<available_skills>` below
- Do not invoke a skill that is already active/loaded
- Skills are not CLI commands - use the skill tool to load them
- After loading a skill, follow its specific instructions to help the user

<available_skills>
{{#each skills}}
<skill>
<name>{{this.name}}</name>
<description>
{{this.description}}
</description>
</skill>
{{/each}}
</available_skills>
