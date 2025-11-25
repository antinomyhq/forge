# forge_select

A centralized crate for user interaction prompts using cliclack.

## Purpose

This crate provides a unified interface for terminal user interactions across the forge codebase. It encapsulates all direct dependencies on `cliclack`, ensuring no other crates need to depend on it directly.

## Features

- **Select prompts**: Choose from a list of options with **fuzzy search filtering** (type to filter options)
- **Confirm prompts**: Yes/no questions
- **Input prompts**: Text input from user with validation
- **Multi-select prompts**: Choose multiple options from a list with **fuzzy search filtering**
- **Consistent theming**: All prompts use cliclack's polished visual style
- **Error handling**: Graceful handling of user cancellation (ESC key)
- **ANSI stripping**: Automatically strips ANSI codes for better search experience

## Usage

### Select from options

```rust
use forge_select::ForgeSelect;

let options = vec!["Option 1", "Option 2", "Option 3"];
let selected = ForgeSelect::select("Choose an option:", options)
    .starting_cursor(1)
    .max_rows(10)  // Limit visible rows to prevent excessive scrolling
    .prompt()?;
```

**Note:** When using `starting_cursor()` with filter mode in long lists, the view will scroll to show the initial cursor position. Use `max_rows()` to limit scrolling and keep the list manageable.

### Confirm (yes/no)

```rust
use forge_select::ForgeSelect;

let confirmed = ForgeSelect::confirm("Are you sure?")
    .default(true)
    .prompt()?;
```

### Text input

```rust
use forge_select::ForgeSelect;

let name = ForgeSelect::input("Enter your name:")
    .allow_empty(false)
    .with_default("John")
    .prompt()?;
```

### Multi-select

```rust
use forge_select::ForgeSelect;

let options = vec!["Red", "Green", "Blue"];
let selected = ForgeSelect::multi_select("Choose colors:", options)
    .prompt()?;
```

## Design

### Builder Pattern

All prompt types use a builder pattern for configuration:
- Create the builder with `ForgeSelect::select()`, `ForgeSelect::confirm()`, etc.
- Configure options with `.with_*()` methods
- Execute with `.prompt()`

### Return Values

All prompts return `Result<Option<T>>`:
- `Ok(Some(T))` - User made a selection
- `Ok(None)` - User cancelled (pressed ESC)
- `Err(e)` - Terminal interaction error

### Ownership vs Clone

Two variants for select operations:
- `select()`: Requires `Clone` for options, useful when you need the list after selection
- `select_owned()`: Takes ownership, no `Clone` required, more efficient

### Theme

All prompts use cliclack's default theme, providing a polished and consistent look across the application. The theme is global and cannot be customized per-prompt, ensuring visual consistency.

### User Cancellation

Users can cancel any prompt by pressing the ESC key. This returns `Ok(None)` rather than an error, allowing graceful handling of user cancellation.

## Integration

This crate is used by:
- `forge_main`: For CLI user interactions
- `forge_infra`: For implementing the `UserInfra` trait

No other crates should depend on `cliclack` directly - use this crate instead.

## Implementation Notes

- **Fuzzy search is enabled by default** via `.filter_mode()` on all select and multiselect prompts
- Users can type to filter options in real-time with fuzzy matching algorithm
- All prompts automatically strip ANSI escape codes from display strings for better fuzzy search experience
- The select prompt uses cliclack's built-in item system for displaying options
- Input validation is handled via closure-based validators
- All error handling converts `std::io::Error` to `Ok(None)` for cancellation cases
- **Scroll control**: Use `max_rows(n)` to limit the number of visible rows and prevent excessive scrolling in long lists
