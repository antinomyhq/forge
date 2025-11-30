---
name: init
description: Initialize or update AGENTS.md with project-specific patterns and insights
---

Analyze this codebase and create an AGENTS.md file with guidelines for AI agents working in this repository.

## Target Location
{{#if parameters}}
**Use the exact path provided by the user:** `{{parameters}}`

Rules:
- If path ends with `.md` → use it as-is
- If path does NOT end with `.md` → append `/AGENTS.md`
- Create any missing parent directories
{{else}}
Create AGENTS.md in the current directory
{{/if}}

## What to Include

1. **Architecture & Boundaries** - Rules that prevent architectural violations:
   - What dependency rules must developers follow? (not how the system is organized)
   - What would break the architecture if a developer violated it?
   - What imports/dependencies are forbidden? Why?

2. **Build/Verification Commands** - How to build, test, lint. Include non-obvious commands or anti-patterns (e.g., "NEVER run X").

3. **Code Patterns to Follow** - Constraints developers must respect when writing code:
   - What patterns must be followed when adding new code? (not how existing code works)
   - How should components be structured? (services, classes, modules)
   - Are there conventions that prevent bugs? (error handling, validation, testing)
   - What are the anti-patterns to avoid?
   - What design principles guide contributions?

4. **Critical Conventions** - Rules that must be followed to avoid bugs:
   - How should errors be handled when crossing boundaries?
   - What testing patterns are required? (structure, assertions, fixtures)
   - What documentation is required? (when/where/what to document)
   - Are there refactoring rules or gotchas?
   - Are there VCS/Git conventions?

## What NOT to Include

- Generic advice ("write tests", "use git", "follow style guide")
- Obvious practices visible in any file
- **Compiler-catchable issues**: Circular dependencies, unused imports, type errors
- **Linter rules**: Language-specific lint suppressions, formatting, style preferences
- **Basic programming concepts**: Standard patterns that any experienced developer knows
- **Business logic**: How features work, what the system does, system behavior
- **Implementation specifics**: Concrete class/function/struct names, field definitions
- **Feature descriptions**: What tools do, how compaction works, what agents exist
- **Inventory lists**: Don't list every struct, enum, function, or file
- **Technology-specific basics**: Language-specific conventions everyone should know
- Project descriptions (focus on HOW to contribute, not WHAT it does)
- Made-up information not found in the codebase
- Summary sections or conclusions

## Discovery Process

1. Check for existing AGENTS.md - if found, preserve its insights
2. Check `.cursor/rules/`, `.cursorrules`, `.github/copilot-instructions.md` for project-specific rules
3. Check README.md for important architectural notes
4. Analyze the codebase for patterns

## Format

Start with:
```markdown
# Agent Guidelines

This document contains guidelines and best practices for AI agents working with this codebase.
```
