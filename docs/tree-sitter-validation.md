# Tree-sitter Validation Feature

This document describes the tree-sitter validation functionality that was restored from PR #2053 and is now configurable via compile-time feature flags and runtime environment variables.

## Overview

Tree-sitter validation provides local syntax checking for various programming languages without requiring network calls. This feature was removed in PR #2053 in favor of remote validation, but has been restored as an optional feature that can be enabled when needed.

## Configuration

### Compile-time Feature Flag

To enable tree-sitter validation support, compile with the `tree_sitter_validation` feature:

```bash
# Build with tree-sitter support
cargo build --features tree_sitter_validation

# Install with tree-sitter support
cargo install --features tree_sitter_validation --path .
```

### Runtime Environment Variable

When compiled with tree-sitter support, enable it at runtime using the `FORGE_USE_TREE_SITTER` environment variable:

```bash
# Enable tree-sitter validation
export FORGE_USE_TREE_SITTER=1
forge [command]

# Or enable for single command
FORGE_USE_TREE_SITTER=1 forge [command]

# Valid values: 1, true, TRUE
```

## Usage Scenarios

### Scenario 1: Tree-sitter Available and Enabled
```bash
# Compile with feature
cargo build --features tree_sitter_validation

# Run with tree-sitter enabled
FORGE_USE_TREE_SITTER=1 forge create file.rs
```
**Result:** Uses local tree-sitter validation for fast syntax checking.

### Scenario 2: Tree-sitter Available but Disabled
```bash
# Compile with feature
cargo build --features tree_sitter_validation

# Run without environment variable
forge create file.rs
```
**Result:** Uses remote validation (default behavior).

### Scenario 3: Tree-sitter Requested but Not Available
```bash
# Compile without feature
cargo build

# Run with environment variable
FORGE_USE_TREE_SITTER=1 forge create file.rs
```
**Result:** Shows warning and falls back to remote validation.

### Scenario 4: Standard Remote Validation
```bash
# Compile without feature
cargo build

# Run normally
forge create file.rs
```
**Result:** Uses remote validation (original behavior).

## Supported Languages

Tree-sitter validation supports the following programming languages:

- **Rust** - `.rs` files
- **Python** - `.py` files  
- **TypeScript** - `.ts`, `.tsx` files
- **JavaScript** - `.js`, `.jsx` files
- **C/C++** - `.c`, `.cpp`, `.cc`, `.cxx` files
- **C#** - `.cs` files
- **Java** - `.java` files
- **Go** - `.go` files
- **PHP** - `.php` files
- **Ruby** - `.rb` files
- **Swift** - `.swift` files
- **Kotlin** - `.kt`, `.kts` files
- **Dart** - `.dart` files
- **HTML** - `.html`, `.htm` files
- **JSON** - `.json` files
- **YAML** - `.yml`, `.yaml` files
- **TOML** - `.toml` files
- **Bash** - `.sh`, `.bash` files
- **PowerShell** - `.ps1` files
- **SQL** - `.sql` files
- **Markdown** - `.md` files

## Performance Benefits

### Tree-sitter Validation (Local)
- **Speed:** Instant syntax validation
- **Network:** No internet connection required
- **Privacy:** Code never leaves your machine
- **Reliability:** Works offline

### Remote Validation
- **Speed:** Depends on network latency
- **Network:** Requires internet connection
- **Privacy:** Code sent to external service
- **Reliability:** Depends on service availability

## Implementation Details

### Architecture

The feature uses a wrapper pattern to choose between validation implementations:

1. **ValidationRepositoryWrapper** - Enum that selects appropriate validator
2. **TreeSitterValidationRepository** - Local tree-sitter implementation
3. **ForgeValidationRepository** - Remote validation implementation
4. **ValidationRepositoryFactory** - Factory for creating appropriate validator

### Conditional Compilation

The tree-sitter code is only compiled when the `tree_sitter_validation` feature is enabled:

```rust
#[cfg(feature = "tree_sitter_validation")]
pub struct TreeSitterValidationRepository;
```

### Environment Variable Logic

```rust
let use_tree_sitter = match std::env::var("FORGE_USE_TREE_SITTER").as_deref() {
    Ok("1") | Ok("true") | Ok("TRUE") => true,
    _ => false,
};
```

## Troubleshooting

### Tree-sitter Not Working

1. **Check compilation:**
   ```bash
   cargo build --features tree_sitter_validation
   ```

2. **Check environment variable:**
   ```bash
   echo $FORGE_USE_TREE_SITTER
   ```

3. **Check warnings:**
   Look for warning messages about tree-sitter not being available.

### Compilation Issues

If you encounter compilation errors with tree-sitter:

1. **Clean build:**
   ```bash
   cargo clean
   cargo build --features tree_sitter_validation
   ```

2. **Update dependencies:**
   ```bash
   cargo update
   ```

3. **Check Rust version:**
   Ensure you're using a compatible Rust version.

## Migration from Remote-only Validation

If you want to migrate from the default remote validation to tree-sitter:

1. **Update build scripts:**
   Add `--features tree_sitter_validation` to your build commands

2. **Set environment variable:**
   ```bash
   export FORGE_USE_TREE_SITTER=1
   ```

3. **Test functionality:**
   Verify that syntax validation works as expected

## Backward Compatibility

This feature maintains full backward compatibility:

- **Default behavior unchanged:** Remote validation remains the default
- **No breaking changes:** Existing code continues to work
- **Optional feature:** Can be enabled/disabled as needed
- **Graceful fallback:** Falls back to remote validation when tree-sitter is unavailable

## Development Notes

### Adding New Language Support

To add support for a new language:

1. Add the tree-sitter grammar dependency to `crates/forge_repo/Cargo.toml`
2. Update the `get_language()` function in `tree_sitter_impl.rs`
3. Add the language to the supported languages list in this documentation

### Testing

Run tests with tree-sitter enabled:

```bash
cargo test --features tree_sitter_validation
```

Run tests without tree-sitter:

```bash
cargo test
```