---
name: init
description: Initialize or update AGENTS.md with project contribution guidelines
---
# STEP 1: ANALYZE THE CODEBASE

**Always start by analyzing the project** to understand:
- Build system
- Test framework and conventions
- Code organization patterns and architecture
- Existing conventions in the codebase

Read key files to understand the project structure and patterns.

# STEP 2: CHECK EXISTING DOCUMENTATION

- Check if `.cursor/rules/` or `.cursorrules` exists - extract important development rules
- Check if `.github/copilot-instructions.md` exists - extract important development rules  
- Check if `README.md` exists - extract development/contribution guidelines (NOT project description)
- Check if `AGENTS.md` exists - use it as a starting point to improve upon

# STEP 3: CREATE/UPDATE AGENTS.MD

Create a concise AGENTS.md file focused on HOW TO CONTRIBUTE to this codebase, not what the project does.

## CRITICAL CONSTRAINTS:
- **HARD LIMIT: Maximum 250 lines total** - if file exceeds this, remove content until under limit
- Focus on contribution workflow, not project explanation
- Assume reader is already familiar with the tech stack
- No sections explaining "what is X" - only "how to work with X"
- Brevity over completeness - capture only the essentials

## REQUIRED CONTENT:

## 1. Common Commands
Essential commands for development workflow:
- How to run tests (all tests, single test, specific module)
- How to build/compile (dev, release, verification)
- How to lint/format code
- Any project-specific dev commands

## 2. Architecture Overview
**Critical patterns only** - architectural rules that prevent bugs:
- Project structure and layer dependencies
- Key design patterns used (with 1 short example if complex)
- Important data flows (1-2 sentences each)
- Any non-obvious architectural decisions

## 3. Contribution Guidelines
Project-specific development practices:
- Error handling patterns
- Test writing conventions
- Code organization patterns
- Documentation requirements
- Any project-specific coding standards

## 4. Development Notes
- Git commit conventions
- Important anti-patterns to avoid
- When to ask before making changes

## STRICT RULES:
- NO detailed component/module/class lists that can be discovered by reading code
- NO exhaustive inventories of files, tools, or features
- NO examples longer than 10 lines
- NO generic advice that applies to any project (e.g., "write tests", "use version control")
- NO descriptions of what the software does or its features
- NO repetition - be concise and direct
- If updating existing AGENTS.md, preserve project-specific knowledge while removing bloat

## OUTPUT FORMAT:
Begin with:

```
# AGENTS.md

This file provides guidance to coding agents (like yourself) when working with code in this repository.
            