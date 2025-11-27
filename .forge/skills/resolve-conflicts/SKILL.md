---
name: resolve-conflicts
description: Use this skill immediately when the user mentions merge conflicts that need to be resolved. Do not attempt to resolve conflicts directly - invoke this skill first. This skill specializes in providing a structured framework for merging imports, tests, lock files (regeneration), configuration files, and handling deleted-but-modified files with backup and analysis.
---

# Git Conflict Resolution

Resolve Git merge conflicts by intelligently combining changes from both branches while preserving the intent of both changes. This skill follows a plan-first approach: assess conflicts, create a detailed resolution plan, get approval, then execute.

## Core Principles

1. **Plan Before Executing**: Always create a structured resolution plan and get user approval before making changes
2. **Prefer Both Changes**: Default to keeping both changes unless they directly contradict
3. **Merge, Don't Choose**: Especially for imports, tests, and configuration
4. **Regenerate Generated Files**: Never manually merge generated files - always regenerate them from their sources
5. **Backup Before Resolving**: For deleted-modified files, create backups first
6. **Validate with Tests**: Always run tests after resolution
7. **Explain All Resolutions**: For each conflict resolved, provide a one-line explanation of the resolution strategy
8. **Ask When Unclear**: When the correct resolution isn't clear from the diff, present options to the user and ask for their choice

## Workflow

### Step 1: Assess the Conflict Situation

Run initial checks to understand the conflict scope:

```bash
git status
```

Identify and categorize all conflicted files:

- Regular file conflicts (both modified)
- Deleted-modified conflicts (one deleted, one modified)
- Generated file conflicts (lock files, build artifacts, generated code)
- Test file conflicts
- Import/configuration conflicts
- Binary file conflicts

For each conflicted file, gather information:

- File type and purpose
- Nature of the conflict (content, deletion, type change)
- Scope of changes (lines changed, sections affected)
- Whether the file is generated or hand-written

### Step 2: Create Merge Resolution Plan

Based on the assessment, create a structured plan before resolving any conflicts. Present the plan in the following markdown format:

```markdown
## Merge Resolution Plan

### Conflict Summary

- **Total conflicted files**: [N]
- **Deleted-modified conflicts**: [N]
- **Generated files**: [N]
- **Regular conflicts**: [N]

### Resolution Strategy by File

#### 1. [File Path]

**Conflict Type**: [deleted-modified / generated / imports / tests / code logic / config / struct / binary]
**Strategy**: [Brief description of resolution approach]
**Rationale**: [Why this strategy is appropriate]
**Risk**: [Low/Medium/High] - [Brief risk description]
**Action Items**:

- [ ] [Specific action 1]
- [ ] [Specific action 2]

#### 2. [File Path]

...

### Execution Order

1. **Phase 1: Deleted-Modified Files** - Handle deletions and backups first
2. **Phase 2: Generated Files** - Regenerate from source
3. **Phase 3: Low-Risk Merges** - Imports, tests, documentation
4. **Phase 4: High-Risk Merges** - Code logic, configuration, structs
5. **Phase 5: Validation** - Compile, test, verify

### Questions/Decisions Needed

- [ ] **[File/Decision]**: [Question for user] (Options: 1, 2, 3)

### Validation Steps

- [ ] Run conflict validation script
- [ ] Compile project
- [ ] Run test suite
- [ ] Manual verification of high-risk changes
```

**Present this plan to the user** and wait for their approval before proceeding with resolution. If there are any unclear conflicts where you need user input, list them in the "Questions/Decisions Needed" section.

**For a complete example plan**, see `references/sample-plan.md`.

### Step 3: Handle Deleted-Modified Files

**Execute this phase only after the plan is approved.**

If there are deleted-but-modified files (status: DU, UD, DD, UA, AU):

```bash
.forge/skills/resolve-conflicts/scripts/handle-deleted-modified.sh
```

This script will:

- Create timestamped backups of modified content
- Analyze potential relocation targets
- Generate analysis reports for each file
- Automatically resolve the deletion status

Review the backup directory and analysis files to understand where changes should be applied.

### Step 4: Execute Resolution Plan

**Follow the execution order defined in your plan.** For each conflicted file, apply the appropriate resolution pattern according to your plan. **For every conflict you resolve, provide a one-line explanation** of how you're resolving it.

As you complete each action item in your plan, mark it as done and report progress to the user.

#### When Resolution is Unclear

When you cannot determine the correct resolution from the diff alone (these should already be listed in your plan's "Questions/Decisions Needed" section):

1. **Present the conflict** to the user with the conflicting code from both sides
2. **Provide numbered options** for resolution (Option 1, Option 2, etc.)
3. **Explain each option** clearly with what it would do
4. **Ask the user to choose** an option number or provide additional information
5. **Remember their choice** and apply similar reasoning to subsequent related conflicts

**Example interaction:**

```
I found a conflict in src/main.rs where both branches modify the `calculate_price` function:

<<<<<<< HEAD (Current Branch)
fn calculate_price(item: &Item) -> f64 {
    item.base_price * (1.0 + item.tax_rate)
}
=======
fn calculate_price(item: &Item) -> f64 {
    item.base_price + item.tax_amount
}
>>>>>>> feature-branch (Incoming Branch)

I'm not sure which calculation is correct. Please select an option:

**Option 1**: Keep current branch (multiplies base_price by tax_rate)
**Option 2**: Keep incoming branch (adds tax_amount to base_price)
**Option 3**: Keep both approaches with a new parameter
**Option 4**: Provide more context to help me decide

Please respond with "Option 1", "Option 2", "Option 3", or "Option 4", or provide additional information.
```

Once the user responds, apply their decision and similar logic to related conflicts.

#### Resolution Patterns

For each conflicted file, apply the appropriate resolution pattern:

#### Imports/Dependencies

**Goal**: Merge all unique imports from both branches.

**One-line explanation**: "Merging imports by combining unique imports from both branches, removing duplicates, and grouping by module."

Read `references/patterns.md` section "Import Conflicts" for detailed examples.

**Quick approach:**

1. Extract all imports from both sides
2. Remove duplicates
3. Group by module/package
4. Follow language-specific style (alphabetize, group std/external/internal)

#### Tests

**Goal**: Include all test cases and test data from both branches.

**One-line explanation**: "Merging tests by including all test cases from both branches, combining fixtures, and renaming if necessary to avoid conflicts."

Read `references/patterns.md` section "Test Conflicts" for detailed examples.

**Quick approach:**

1. Keep all test functions unless they test the exact same thing
2. Merge test fixtures and setup functions
3. Combine assertions from both sides
4. If test names conflict but test different behaviors, rename to clarify

#### Generated Files

**Goal**: Regenerate any generated files to include changes from both branches.

**One-line explanation**: "Resolving generated file by regenerating it from source files to incorporate changes from both branches."

**Recognition**: A file is generated if it:

- Is produced by a build tool, compiler, or code generator
- Has a source file or configuration that defines it
- Contains headers/comments indicating it's auto-generated
- Is listed in `.gitattributes` as generated
- Common examples: lock files, protobuf outputs, GraphQL schema files, compiled assets, auto-generated docs

**Approach:**

1. **Identify the generation source**: Determine what command or tool generates the file
2. **Choose either version** temporarily (doesn't matter which):

   ```bash
   git checkout --ours <generated-file>    # or --theirs
   ```

3. **Regenerate from source**: Run the appropriate generation command:

   ```bash
   # Package manager lock files
   cargo update                       # for Cargo.lock
   npm install                        # for package-lock.json
   yarn install                       # for yarn.lock
   bundle install                     # for Gemfile.lock
   poetry lock --no-update            # for poetry.lock

   # Code generation
   protoc ...                         # for protobuf files
   graphql-codegen                    # for GraphQL generated code
   make generate                      # for Makefile-based generation
   npm run generate                   # for npm script-based generation

   # Build artifacts
   npm run build                      # for compiled/bundled assets
   cargo build                        # for Rust build artifacts
   ```

4. **Stage the regenerated file**:
   ```bash
   git add <generated-file>
   ```

**When unsure if a file is generated**: Check for auto-generation markers in the file header, or ask the user if you should regenerate or manually merge the file.

#### Configuration Files

**Goal**: Merge configuration values from both branches.

**One-line explanation**: "Merging configuration by including all keys from both branches and choosing appropriate values for conflicts."

Read `references/patterns.md` section "Configuration File Conflicts" for detailed examples.

**Quick approach:**

1. Include all keys from both sides
2. For conflicting values, choose based on:
   - Newer/more recent value
   - Safer/more conservative value
   - Production requirements
3. Document choice in commit message

**When unclear**: Ask the user which configuration value to prefer (current vs incoming)

#### Code Logic

**Goal**: Understand intent of both changes and combine if possible.

**One-line explanation**: "Resolving code logic by analyzing intent: merging if changes are orthogonal, or choosing one approach if they conflict."

Read `references/patterns.md` section "Code Logic Conflicts" for detailed examples.

**Quick approach:**

1. Analyze what each branch is trying to achieve
2. If changes are orthogonal (different concerns), merge both
3. If changes conflict (same concern, different approach):
   - Review commit messages/PRs for context
   - Choose the approach that matches requirements
   - Test both approaches if unclear
   - Document the decision

**When unclear**: Present both approaches as options to the user with context about what each does

#### Struct/Type Definitions

**Goal**: Include all fields from both branches.

**One-line explanation**: "Merging struct by including all fields from both branches and choosing appropriate types for any conflicting field definitions."

**Quick approach:**

1. Merge all fields
2. If field types conflict, analyze which is more appropriate
3. Fix all compilation errors from updated struct
4. Update tests to use new fields

**When unclear**: Ask the user which type definition is correct if field types conflict

### Step 5: Validate Resolution

After completing all resolution phases in your plan, validate that all conflicts are resolved:

```bash
.forge/skills/resolve-conflicts/scripts/validate-conflicts.sh
```

This script checks for:

- Remaining conflict markers (<<<<<<<, =======, >>>>>>>)
- Unmerged paths in git status
- Deleted-modified conflicts
- Merge state files

### Step 6: Compile and Test

Build and test to ensure the resolution is correct (as defined in your plan's validation steps):

```bash
# For Rust projects
cargo test

# For other projects, use appropriate test command
# npm test
# pytest
# etc.
```

If tests fail:

1. Review the failure - is it from merged code or conflict resolution?
2. Check if both branches' tests pass individually
3. Fix integration issues between the merged changes
4. Re-run tests until all pass

### Step 7: Finalize

Once all conflicts are resolved and tests pass, review your completed plan and commit:

```bash
# Review the changes
git diff --cached

# Commit with descriptive message that references the plan
git commit -m "Resolve merge conflicts: [describe key decisions]

Executed merge resolution plan:
- [Phase 1 summary]
- [Phase 2 summary]
- [Phase 3+ summaries]

Key decisions:
- Merged imports from both branches
- Combined test cases
- Regenerated lock files
- [other significant decisions from plan]

Co-Authored-By: ForgeCode <noreply@forgecode.dev>"
```

## Decision Tracking

When you ask the user to choose between options, track their decision and apply similar reasoning to subsequent conflicts:

**Example scenario:**

1. First conflict: User chooses Option 1 (prefer current branch's validation logic)
2. Second similar conflict: Apply the same reasoning (prefer current branch's validation approach)
3. Mention: "Resolving by keeping current branch's approach (consistent with your earlier choice)"

**Key principles:**

- Remember user preferences within the same conflict resolution session
- Apply consistent patterns when conflicts are similar
- Mention the consistency: "Following the same pattern as before..."
- Ask again if a new conflict is sufficiently different from previous ones

## Common Patterns Reference

For detailed resolution patterns, read:

- `references/patterns.md` - Comprehensive examples for all conflict types

**Quick pattern lookup:**

- **Imports**: Combine all unique imports, group by module
- **Tests**: Keep all tests unless identical, merge fixtures
- **Generated files**: Choose either version, regenerate from source
- **Config**: Merge all keys, choose newer/safer values for conflicts
- **Code**: Analyze intent, merge if orthogonal, choose one if conflicting
- **Structs**: Include all fields from both branches
- **Docs**: Combine all documentation sections

## Special Scenarios

### Binary Files in Conflict

Binary files cannot be merged. Choose one version:

```bash
git checkout --ours path/to/binary    # keep our version
# or
git checkout --theirs path/to/binary  # keep their version
```

### Mass Rename/Refactoring Conflicts

If one branch renamed/refactored many files while another modified them:

1. Accept the rename/refactoring (structural change)
2. Apply the modifications to the new structure
3. Use backups from `handle-deleted-modified.sh` to guide the application

### Submodule Conflicts

```bash
# Check submodule status
git submodule status

# Update to the correct commit
cd path/to/submodule
git checkout <desired-commit>
cd ../..
git add path/to/submodule
```

## Troubleshooting

### "Both Added" Conflicts (AA)

Both branches added a new file with the same name but different content:

1. Review both versions
2. If they serve the same purpose, merge their content
3. If they serve different purposes, rename one

### Whitespace-Only Conflicts

If conflicts are only whitespace differences:

```bash
git merge -Xignore-space-change <branch>
```

### Persistent Conflict Markers

If validation shows conflict markers but you think you resolved them:

1. Search for the exact marker strings: `git grep -n "<<<<<<< HEAD"`
2. Some markers might be in strings or comments - resolve those too
3. Check for hidden characters or encoding issues

### Tests Fail After Resolution

1. Test each branch individually to confirm they pass
2. The failure is likely from interaction between the merged changes
3. Debug the interaction issue, not the individual changes
4. Update code to make both changes work together

## Quick Reference Card

| Conflict Type    | Strategy                                | One-line Explanation Template                                                                                    |
| ---------------- | --------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Imports          | Merge all, deduplicate, group by module | "Merging imports by combining unique imports from both branches and grouping by module"                          |
| Tests            | Keep all, merge fixtures                | "Including all test cases from both branches and combining test fixtures"                                        |
| Generated files  | Regenerate from source                  | "Regenerating [file] from source to include changes from both branches"                                          |
| Config           | Merge keys, choose newer values         | "Merging all config keys and choosing [current/incoming] value for [key]"                                        |
| Code logic       | Analyze intent, merge if orthogonal     | "Merging both changes as they address different concerns" OR "Choosing [current/incoming] approach for [reason]" |
| Structs          | Include all fields                      | "Including all fields from both branches in struct definition"                                                   |
| Docs             | Combine all sections                    | "Combining documentation from both branches"                                                                     |
| Deleted-modified | Backup, analyze, apply to new location  | "Applying modifications to new location after file was moved/renamed"                                            |
| Binary files     | Choose one version                      | "Keeping [current/incoming] version of binary file"                                                              |

**Remember:**

- Always provide a one-line explanation for each conflict resolution
- When unclear, present numbered options to the user
- Track user decisions and apply consistently to similar conflicts
- The goal is to preserve the intent and functionality of both branches while creating a cohesive merged result
