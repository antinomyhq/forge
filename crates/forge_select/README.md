# forge_select

A centralized crate for user interaction prompts using dialoguer.

## Purpose

This crate provides a unified interface for terminal user interactions across the forge codebase. It encapsulates all direct dependencies on `dialoguer`, ensuring no other crates need to depend on it directly.

## Features

- **Select prompts**: Choose from a list of options
- **Confirm prompts**: Yes/no questions
- **Input prompts**: Text input from user
- **Multi-select prompts**: Choose multiple options from a list
- **Terminal control**: Manage bracketed paste mode and cursor key modes
- **Consistent theming**: All prompts use a unified color scheme
- **Error handling**: Graceful handling of user interruptions

## Usage

### Select from options

```rust
use forge_select::ForgeSelect;

let options = vec!["Option 1", "Option 2", "Option 3"];
let selected = ForgeSelect::select("Choose an option:", options)
    .with_starting_cursor(1)
    .prompt()?;
```

### Confirm (yes/no)

```rust
use forge_select::ForgeSelect;

let confirmed = ForgeSelect::confirm("Are you sure?")
    .with_default(true)
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

### Terminal Control

#### Bracketed Paste Mode

Bracketed paste mode causes terminals to wrap pasted content with special markers (`~0` and `~1`). You can disable this when needed:

```rust
use forge_select::TerminalControl;

// Manual control
TerminalControl::disable_bracketed_paste()?;
// ... do work ...
TerminalControl::enable_bracketed_paste()?;
```

Or use the RAII guard (recommended) for automatic cleanup:

```rust
use forge_select::BracketedPasteGuard;

{
    let _guard = BracketedPasteGuard::new()?;
    // Bracketed paste is disabled here
    // ... do work ...
} // Automatically re-enabled when guard drops
```

#### Application Cursor Keys Mode

Control how arrow keys behave in the terminal:

```rust
use forge_select::TerminalControl;

// Manual control
TerminalControl::disable_application_cursor_keys()?;
// ... do work ...
TerminalControl::enable_application_cursor_keys()?;
```

Or use the RAII guard (recommended) for automatic cleanup:

```rust
use forge_select::ApplicationCursorKeysGuard;

{
    let _guard = ApplicationCursorKeysGuard::new()?;
    // Application cursor keys are disabled here
    // ... do work ...
} // Automatically re-enabled when guard drops
```

## Design

### Builder Pattern

All prompt types use a builder pattern for configuration:
- Create the builder with `ForgeSelect::select()`, `ForgeSelect::confirm()`, etc.
- Configure options with `.with_*()` methods
- Execute with `.prompt()`

### Ownership vs Clone

Two variants for select operations:
- `select()`: Requires `Clone` for options, useful when you need the list after selection
- `select_owned()`: Takes ownership, no `Clone` required, more efficient

### Theme

All prompts use a consistent `ColorfulTheme` from dialoguer, providing a unified look across the application.

## Integration

This crate is used by:
- `forge_main`: For CLI user interactions
- `forge_infra`: For implementing the `UserInfra` trait

No other crates should depend on `dialoguer` directly - use this crate instead.
