# Create Plan Skill

Tools and scripts for creating and validating implementation plans.

## Files

- `SKILL.md` - Main skill instructions for AI agents
- `validate-plan.sh` - Validates a single plan file
- `validate-all-plans.sh` - Validates all plans in a directory

## Validation Scripts

### validate-plan.sh

Validates the structure and content of a single plan file.

**Usage:**
```bash
./.forge/skills/create-plan/validate-plan.sh plans/2025-11-27-example-v1.md
```

**Checks:**
- ✓ Filename follows convention: `YYYY-MM-DD-task-name-vN.md`
  - Year is reasonable (2020 to current year + 1)
  - Month is valid (01-12)
  - Day is valid (01-31)
  - Task name is meaningful (not generic like "test", "task", "temp")
  - Task name length is reasonable (5-60 characters)
  - Version number is reasonable (not > 50)
  - No uppercase letters or underscores (use lowercase and hyphens only)
- ✓ File is in `plans/` directory
- ✓ All required sections present:
  - Main heading (`# Title`)
  - `## Objective`
  - `## Implementation Plan`
  - `## Verification Criteria`
  - `## Potential Risks and Mitigations`
  - `## Alternative Approaches`
- ✓ Implementation Plan uses checkbox format (`- [ ]`)
- ✓ No numbered lists or plain bullets in Implementation Plan
- ✓ No code blocks (` ``` `) in the plan
- ✓ No code snippets (detects suspicious patterns)
- ✓ No placeholder tasks (TODO, TBD, etc.)
- ✓ **Task quality and density:**
  - Task descriptions are descriptive (≥ 20 characters recommended)
  - Average task length is substantial (30-200 chars recommended)
  - No generic/vague descriptions ("implement feature", "fix bug", etc.)
  - No duplicate or highly similar tasks
  - Consistent and sequential numbering if tasks are numbered
- ✓ Verification criteria have content
- ✓ Risks include mitigations
- ✓ Reasonable number of tasks (3-20)

**Exit Codes:**
- `0` - Validation passed
- `1` - Validation failed (errors found)

### validate-all-plans.sh

Validates all plan files in a directory.

**Usage:**
```bash
# Validate all plans in default directory (plans/)
./.forge/skills/create-plan/validate-all-plans.sh

# Validate plans in custom directory
./.forge/skills/create-plan/validate-all-plans.sh path/to/plans
```

**Exit Codes:**
- `0` - All plans passed validation
- `1` - One or more plans failed validation

## Integration

### Pre-commit Hook

Add to `.git/hooks/pre-commit`:

```bash
#!/bin/bash
# Validate plans before committing

if git diff --cached --name-only | grep -q "^plans/.*\.md$"; then
    echo "Validating modified plans..."
    ./.forge/skills/create-plan/validate-all-plans.sh plans/
    exit $?
fi
```

### CI/CD

Add to your CI pipeline:

```yaml
- name: Validate Plans
  run: ./.forge/skills/create-plan/validate-all-plans.sh plans/
```

## Example Valid Plan

See `SKILL.md` for the complete plan template structure.

## Common Validation Errors

1. **Invalid filename format**: Must follow `YYYY-MM-DD-task-name-vN.md` pattern
   - Use lowercase letters only
   - Use hyphens (not underscores) to separate words
   - Use valid date (month 01-12, day 01-31, year 2020+)
   - Avoid generic names like "test", "task", "temp"
2. **Missing checkboxes**: Use `- [ ]` not `1.` or `-`
3. **Code blocks**: Plans should use natural language, not code
4. **Missing sections**: All required sections must be present
5. **Empty sections**: Sections should have meaningful content
6. **Poor task quality**: Tasks should be descriptive and specific
   - Avoid short descriptions like "Do this", "Fix that", "Update code"
   - Avoid generic descriptions like "implement feature", "add functionality"
   - Include rationale and context in task descriptions
   - Aim for 30-150 characters per task description
