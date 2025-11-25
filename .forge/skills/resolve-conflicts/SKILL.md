---
name: resolve-conflicts
description: Resolve Git merge conflicts by intelligently combining changes from both branches. Use when encountering merge conflicts during git merge, rebase, or cherry-pick operations. Specializes in merging imports, tests, lock files (regeneration), configuration files, and handling deleted-but-modified files with backup and analysis.
---

# Git Conflict Resolution

Resolve Git merge conflicts by intelligently combining changes from both branches while preserving the intent of both changes.

## Core Principles

1. **Prefer Both Changes**: Default to keeping both changes unless they directly contradict
2. **Merge, Don't Choose**: Especially for imports, tests, and configuration
3. **Regenerate Lock Files**: Never manually merge lock files
4. **Backup Before Resolving**: For deleted-modified files, create backups first
5. **Validate with Tests**: Always run tests after resolution

## Workflow

### Step 1: Assess the Conflict Situation

Run initial checks to understand the conflict scope:

```bash
git status
```

Identify conflict types:
- Regular file conflicts (both modified)
- Deleted-modified conflicts (one deleted, one modified)
- Lock file conflicts
- Test file conflicts
- Import/configuration conflicts

### Step 2: Handle Deleted-Modified Files

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

### Step 3: Resolve Regular Conflicts

For each conflicted file, apply the appropriate resolution pattern:

#### Imports/Dependencies

**Goal**: Merge all unique imports from both branches.

Read `references/patterns.md` section "Import Conflicts" for detailed examples.

**Quick approach:**
1. Extract all imports from both sides
2. Remove duplicates
3. Group by module/package
4. Follow language-specific style (alphabetize, group std/external/internal)

#### Tests

**Goal**: Include all test cases and test data from both branches.

Read `references/patterns.md` section "Test Conflicts" for detailed examples.

**Quick approach:**
1. Keep all test functions unless they test the exact same thing
2. Merge test fixtures and setup functions
3. Combine assertions from both sides
4. If test names conflict but test different behaviors, rename to clarify

#### Lock Files

**Goal**: Regenerate the lock file to include dependencies from both branches.

**Approach:**
```bash
# Choose either version (doesn't matter which)
git checkout --ours Cargo.lock    # or --theirs

# Regenerate based on updated manifest
cargo update                       # for Cargo.lock
# npm install                      # for package-lock.json
# yarn install                     # for yarn.lock
# bundle install                   # for Gemfile.lock
# poetry lock --no-update          # for poetry.lock

# Stage the regenerated file
git add Cargo.lock
```

#### Configuration Files

**Goal**: Merge configuration values from both branches.

Read `references/patterns.md` section "Configuration File Conflicts" for detailed examples.

**Quick approach:**
1. Include all keys from both sides
2. For conflicting values, choose based on:
   - Newer/more recent value
   - Safer/more conservative value
   - Production requirements
3. Document choice in commit message

#### Code Logic

**Goal**: Understand intent of both changes and combine if possible.

Read `references/patterns.md` section "Code Logic Conflicts" for detailed examples.

**Quick approach:**
1. Analyze what each branch is trying to achieve
2. If changes are orthogonal (different concerns), merge both
3. If changes conflict (same concern, different approach):
   - Review commit messages/PRs for context
   - Choose the approach that matches requirements
   - Test both approaches if unclear
   - Document the decision

#### Struct/Type Definitions

**Goal**: Include all fields from both branches.

**Quick approach:**
1. Merge all fields
2. If field types conflict, analyze which is more appropriate
3. Fix all compilation errors from updated struct
4. Update tests to use new fields

### Step 4: Validate Resolution

After resolving conflicts, validate that all conflicts are resolved:

```bash
.forge/skills/resolve-conflicts/scripts/validate-conflicts.sh
```

This script checks for:
- Remaining conflict markers (<<<<<<<, =======, >>>>>>>)
- Unmerged paths in git status
- Deleted-modified conflicts
- Merge state files

### Step 5: Compile and Test

Build and test to ensure the resolution is correct:

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

### Step 6: Finalize

Once all conflicts are resolved and tests pass:

```bash
# Review the changes
git diff --cached

# Commit with descriptive message
git commit -m "Resolve merge conflicts: [describe key decisions]

- Merged imports from both branches
- Combined test cases
- Regenerated lock files
- [other significant decisions]

Co-Authored-By: ForgeCode <noreply@forgecode.dev>"
```

## Common Patterns Reference

For detailed resolution patterns, read:
- `references/patterns.md` - Comprehensive examples for all conflict types

**Quick pattern lookup:**
- **Imports**: Combine all unique imports, group by module
- **Tests**: Keep all tests unless identical, merge fixtures
- **Lock files**: Choose either version, regenerate with package manager
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

| Conflict Type | Strategy |
|--------------|----------|
| Imports | Merge all, deduplicate, group by module |
| Tests | Keep all, merge fixtures |
| Lock files | Regenerate with package manager |
| Config | Merge keys, choose newer values |
| Code logic | Analyze intent, merge if orthogonal |
| Structs | Include all fields |
| Docs | Combine all sections |
| Deleted-modified | Backup, analyze, apply to new location |
| Binary files | Choose one version |

Remember: The goal is to preserve the intent and functionality of both branches while creating a cohesive merged result. When in doubt, run tests and review with the original authors.
