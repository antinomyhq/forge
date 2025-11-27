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

1. **Missing checkboxes**: Use `- [ ]` not `1.` or `-`
2. **Code blocks**: Plans should use natural language, not code
3. **Missing sections**: All required sections must be present
4. **Empty sections**: Sections should have meaningful content
5. **Incorrect filename**: Must follow `YYYY-MM-DD-task-name-vN.md` pattern
