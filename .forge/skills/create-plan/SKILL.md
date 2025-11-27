---
name: create-plan
description: Generate detailed implementation plans for complex tasks. Creates comprehensive strategic plans in Markdown format with objectives, step-by-step implementation tasks using checkbox format, verification criteria, risk assessments, and alternative approaches. All plans MUST be validated using the included validation script. Use when users need thorough analysis and structured planning before implementation, when breaking down complex features into actionable steps, or when they explicitly ask for a plan, roadmap, or strategy. Strictly planning-focused with no code modifications.
---

# Create Implementation Plan

Generate comprehensive implementation plans that provide strategic guidance without making actual code changes.

## When to Use

- User explicitly requests a plan, roadmap, or implementation strategy
- Complex tasks requiring structured breakdown before implementation
- Need for risk assessment and alternative approach analysis
- Pre-implementation analysis of architectural decisions

## Planning Process

### 1. Initial Assessment

Research the codebase to understand:

- Project structure and organization
- Relevant files and components
- Existing patterns and conventions
- Potential challenges and risks

Use `search` and `read` tools to examine the codebase. Use `sage` if deeper research is required for the use-case. Explicitly cite sources using `filepath:line` format.

### 2. Create Strategic Plan

Generate a Markdown plan file in `plans/` directory with naming: `plans/{YYYY-MM-DD}-{task-name}-v{N}.md`

Example: `plans/2025-11-24-add-auth-v1.md`

### 3. Validate Plan

**MANDATORY:** Run the validation script to ensure the plan meets all requirements:

```bash
./.forge/skills/create-plan/validate-plan.sh plans/{YYYY-MM-DD}-{task-name}-v{N}.md
```

Fix any errors or warnings and re-validate until the plan passes all checks.

### 4. Plan Structure

```markdown
# [Task Name]

## Objective

[Clear statement of goal and expected outcomes]

## Implementation Plan

- [ ] 1. [First task with detailed description and rationale]
- [ ] 2. [Second task with detailed description and rationale]
- [ ] 3. [Third task with detailed description and rationale]

## Verification Criteria

- [Criterion 1: Specific, measurable outcome]
- [Criterion 2: Specific, measurable outcome]

## Potential Risks and Mitigations

1. **[Risk Description]**
   Mitigation: [Specific mitigation strategy]

2. **[Risk Description]**
   Mitigation: [Specific mitigation strategy]

## Alternative Approaches

1. [Alternative 1]: [Brief description and trade-offs]
2. [Alternative 2]: [Brief description and trade-offs]
```

## Critical Requirements

- **ALWAYS validate the plan** using `./.forge/skills/create-plan/validate-plan.sh` after creation
- **ALWAYS use checkbox format** (`- [ ]`) for ALL implementation tasks
- **NEVER use numbered lists** or plain bullet points in Implementation Plan section
- **NEVER write code, code snippets, or code examples** in the plan
- Include clear rationale for each task
- Provide specific, measurable verification criteria
- Document assumptions made for ambiguous requirements
- Focus on strategic "what" and "why", not tactical "how"
- Describe what needs to be done using natural language, not code

## Best Practices

- Make reasonable assumptions when requirements are ambiguous
- Use codebase patterns to infer best practices
- Provide multiple solution paths for complex challenges
- Balance thoroughness with actionability
- Create plans that can be executed step-by-step by implementation agents

## Boundaries

This is a **planning-only** skill:

- ✅ Research codebase and analyze structure
- ✅ Create strategic plans and documentation
- ✅ Assess risks and propose alternatives
- ✅ Describe implementations using natural language
- ❌ Make actual code changes
- ❌ Modify files or create implementations
- ❌ Run tests or build commands
- ❌ Write code, code snippets, or code examples in plans

If user requests implementation work, suggest switching to an implementation agent.
