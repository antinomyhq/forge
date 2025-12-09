# Conflict Resolution Patterns

This document provides detailed patterns for resolving specific types of conflicts.

**Important**: For each conflict you resolve, provide a one-line explanation of your resolution strategy. When the correct resolution isn't clear from the diff, present numbered options to the user.

## Import Conflicts

When both branches modify import statements, merge both sets of imports:

### Pattern: Combine and Deduplicate

```
<<<<<<< HEAD
import { foo, bar } from './module';
import { baz } from './other';
=======
import { foo, qux } from './module';
import { newThing } from './another';
>>>>>>> branch
```

**Resolution:** Merge all unique imports, group by module:

```
import { foo, bar, qux } from './module';
import { baz } from './other';
import { newThing } from './another';
```

### Rust Imports

```
<<<<<<< HEAD
use std::collections::HashMap;
use crate::domain::User;
=======
use std::collections::HashSet;
use crate::domain::Account;
>>>>>>> branch
```

**Resolution:**

```
use std::collections::{HashMap, HashSet};
use crate::domain::{Account, User};
```

**Key principles:**
- Combine all unique imports
- Remove duplicates
- Follow language-specific style (group by module, alphabetize)
- Preserve any re-exports or aliases from both sides

**One-line explanation example**: "Merging imports by combining unique imports from both branches and grouping by module."

## Test Conflicts

Tests should almost always include both changes, as tests are additive.

### Pattern: Merge Test Cases

```
<<<<<<< HEAD
#[test]
fn test_user_creation() { ... }

#[test]
fn test_user_validation() { ... }
=======
#[test]
fn test_user_creation() { ... }

#[test]
fn test_user_deletion() { ... }
>>>>>>> branch
```

**Resolution:** Include all tests (assuming test_user_creation is identical):

```
#[test]
fn test_user_creation() { ... }

#[test]
fn test_user_validation() { ... }

#[test]
fn test_user_deletion() { ... }
```

### Test Setup/Fixtures Conflicts

When both branches modify test fixtures, merge the changes:

```
<<<<<<< HEAD
fn setup() -> TestContext {
    TestContext {
        user: create_test_user(),
        admin: create_admin(),
    }
}
=======
fn setup() -> TestContext {
    TestContext {
        user: create_test_user(),
        database: init_test_db(),
    }
}
>>>>>>> branch
```

**Resolution:**

```
fn setup() -> TestContext {
    TestContext {
        user: create_test_user(),
        admin: create_admin(),
        database: init_test_db(),
    }
}
```

**Key principles:**
- Keep all test cases unless they test the exact same thing
- Merge test fixtures and setup functions
- If test names conflict but test different things, rename one
- Preserve all assertions from both sides

**One-line explanation example**: "Including all test cases from both branches and merging test fixtures."

## Lock File Conflicts

Lock files (Cargo.lock, package-lock.json, yarn.lock, etc.) should be regenerated rather than manually resolved.

### Pattern: Regenerate Lock File

```bash
# For Cargo.lock
git checkout --theirs Cargo.lock  # or --ours, either works
cargo update  # or cargo build

# For package-lock.json
git checkout --theirs package-lock.json
npm install

# For yarn.lock
git checkout --theirs yarn.lock
yarn install

# For Gemfile.lock
git checkout --theirs Gemfile.lock
bundle install

# For poetry.lock
git checkout --theirs poetry.lock
poetry lock --no-update
```

**Key principles:**
- Always regenerate, never manually merge
- Choose either version (--ours or --theirs), doesn't matter
- Run the package manager's update/install command
- The result will include dependencies from both branches

**One-line explanation example**: "Regenerating lock file with package manager to include dependencies from both branches."

## Configuration File Conflicts

Configuration files often need careful merging of both changes.

### Pattern: Merge Configuration Values

```yaml
<<<<<<< HEAD
server:
  port: 8080
  timeout: 30
  max_connections: 100
=======
server:
  port: 8080
  timeout: 60
  enable_https: true
>>>>>>> branch
```

**Resolution:**

```yaml
server:
  port: 8080
  timeout: 60  # Prefer the newer/safer value
  max_connections: 100
  enable_https: true
```

**Key principles:**
- Include all configuration keys from both sides
- When same key has different values, choose based on:
  - Newer value (if timestamp available)
  - Safer/more conservative value
  - Production-ready value
  - Document the choice in commit message

**One-line explanation example**: "Merging all config keys and choosing incoming value for 'timeout' as it's more recent."

**When to ask the user**: If conflicting values have significant implications (e.g., security settings, API endpoints), present options:
```
Config conflict in config.yaml for key 'timeout':

**Option 1**: Keep current value (30 seconds)
**Option 2**: Keep incoming value (60 seconds)
**Option 3**: Provide a different value

Please select an option.
```

## Code Logic Conflicts

When both branches modify the same function, carefully analyze the intent.

### Pattern: Sequential Changes

If changes are independent and can coexist:

```
<<<<<<< HEAD
fn process(data: &str) -> Result<String> {
    let cleaned = data.trim();
    validate(cleaned)?;
    Ok(cleaned.to_uppercase())
}
=======
fn process(data: &str) -> Result<String> {
    let cleaned = data.trim();
    if cleaned.is_empty() {
        return Err(Error::EmptyInput);
    }
    Ok(cleaned.to_uppercase())
}
>>>>>>> branch
```

**Resolution:** Combine both validations:

```
fn process(data: &str) -> Result<String> {
    let cleaned = data.trim();
    if cleaned.is_empty() {
        return Err(Error::EmptyInput);
    }
    validate(cleaned)?;
    Ok(cleaned.to_uppercase())
}
```

**One-line explanation**: "Merging both validations as they check different conditions (emptiness and validation)."

### Pattern: Conflicting Logic

If changes represent different approaches:

```
<<<<<<< HEAD
fn calculate_price(item: &Item) -> f64 {
    item.base_price * (1.0 + item.tax_rate)
}
=======
fn calculate_price(item: &Item) -> f64 {
    item.base_price + item.tax_amount
}
>>>>>>> branch
```

**Resolution:** Analyze which approach is correct:
- Review PR/commit messages for context
- Check which calculation matches business requirements
- Consider running tests with both approaches
- Choose one and document why in commit message

**When to ask the user**: Present this as options when the correct approach isn't clear:

```
Code logic conflict in calculate_price function:

<<<<<<< HEAD (Current Branch)
fn calculate_price(item: &Item) -> f64 {
    item.base_price * (1.0 + item.tax_rate)
}
=======
fn calculate_price(item: &Item) -> f64 {
    item.base_price + item.tax_amount
}
>>>>>>> feature-branch (Incoming Branch)

These represent different calculation methods:

**Option 1**: Keep current branch - calculates tax as percentage (base_price * tax_rate)
**Option 2**: Keep incoming branch - uses pre-calculated tax amount (base_price + tax_amount)
**Option 3**: Ask you to clarify the correct business logic

Please select an option.
```

**One-line explanation example**: "Choosing current branch approach as it calculates tax dynamically based on rate (per user selection)."

## Struct/Type Definition Conflicts

Merge all fields from both branches.

### Pattern: Merge Struct Fields

```
<<<<<<< HEAD
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub created_at: DateTime,
}
=======
pub struct User {
    pub id: i64,
    pub name: String,
    pub role: UserRole,
    pub updated_at: DateTime,
}
>>>>>>> branch
```

**Resolution:**

```
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub role: UserRole,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}
```

**Key principles:**
- Include all fields from both sides
- If field types conflict, analyze which is more appropriate
- Update all usages of the struct accordingly
- Fix compilation errors after merging

**One-line explanation example**: "Including all fields from both branches in User struct."

**When to ask the user**: If the same field has different types:
```
Struct conflict - field 'role' has different types:

**Option 1**: Keep current type (role: String)
**Option 2**: Keep incoming type (role: UserRole enum)
**Option 3**: Provide more context

Please select an option.
```

## Documentation Conflicts

Merge all documentation improvements.

### Pattern: Combine Documentation

```
<<<<<<< HEAD
/// Processes user input and returns validated data.
/// 
/// # Arguments
/// * `input` - The raw user input
=======
/// Processes user input and returns validated data.
/// 
/// # Errors
/// Returns `Error::InvalidInput` if validation fails
>>>>>>> branch
```

**Resolution:**

```
/// Processes user input and returns validated data.
/// 
/// # Arguments
/// * `input` - The raw user input
/// 
/// # Errors
/// Returns `Error::InvalidInput` if validation fails
```

**Key principles:**
- Preserve all documentation sections
- If descriptions conflict, choose the more accurate/detailed one
- Keep all examples from both sides
- Maintain consistent formatting

**One-line explanation example**: "Combining all documentation sections from both branches."

## Deleted File Special Cases

### Pattern: File Renamed/Moved

If file was deleted on one branch but modified on another, and there's a similar new file:

1. Check if file was renamed: `git log --follow --diff-filter=R -- <file>`
2. Apply modifications to the new location
3. Remove the old file

### Pattern: File Legitimately Deleted

If file deletion was intentional (feature removed, refactored):

1. Review the modifications from the other branch
2. Determine if any changes are still relevant
3. If yes, apply to the appropriate new location
4. If no, accept the deletion

### Pattern: Accidental Deletion

If file should not have been deleted:

1. Restore the file from the branch that kept it
2. Apply any additional modifications
3. Verify tests pass
