# Color Support in Forge

This document describes the color support features added to Forge to address visibility issues on light terminal backgrounds.

## Problem

On terminals with light backgrounds, Forge's bright ANSI colors (bright-yellow, bright-white, light-grey) were difficult to read or completely invisible, making the tool unusable for users with light themes.

## Solution

We've implemented a comprehensive color configuration system that:

1. **Supports Industry Standards**: Respects the `NO_COLOR` environment variable (see https://no-color.org/)
2. **Provides CLI Control**: Adds `--color` and `--no-color` flags for explicit control
3. **Improves Contrast**: Uses enhanced color functions with better contrast for light backgrounds
4. **Auto-detects Terminals**: Automatically detects if output is going to a terminal

## Usage

### Environment Variables

- `NO_COLOR`: Set to any non-empty value to disable colors (industry standard)
- `FORGE_COLOR`: Set to `always`, `auto`, or `never` to control color output

```bash
# Disable colors using NO_COLOR (recommended)
NO_COLOR=1 forge

# Disable colors using FORGE_COLOR
FORGE_COLOR=never forge

# Force colors even when not outputting to a terminal
FORGE_COLOR=always forge > output.txt
```

### CLI Flags

```bash
# Disable colors
forge --no-color

# Explicit color control
forge --color=never    # Never use colors
forge --color=auto     # Use colors only when outputting to terminal (default)
forge --color=always   # Always use colors

# The --no-color flag is equivalent to --color=never
forge --no-color
```

### Priority Order

Color settings are applied in this priority order (highest to lowest):

1. CLI flags (`--color`, `--no-color`)
2. `FORGE_COLOR` environment variable
3. `NO_COLOR` environment variable
4. Auto-detection (default behavior)

## Enhanced Colors

The implementation includes enhanced color functions that provide better contrast on both light and dark backgrounds:

- **Yellow**: Uses a darker yellow (#b8860b) instead of bright yellow
- **White**: Uses dark gray (#374151) instead of white on light backgrounds
- **Dimmed**: Uses medium gray (#6b7280) for better readability
- **Other colors**: Adjusted for better visibility across different terminal themes

## Implementation Details

### Architecture

- `forge_display::color` module: Core color configuration and enhanced color functions
- Global color configuration: Thread-safe singleton using `std::sync::OnceLock`
- Enhanced color functions: Provide better contrast alternatives to standard colors

### Files Modified

- `crates/forge_display/src/color.rs`: New color configuration module
- `crates/forge_display/src/title.rs`: Updated to use enhanced colors
- `crates/forge_spinner/src/lib.rs`: Updated to use enhanced colors
- `crates/forge_main/src/cli.rs`: Added color CLI flags
- `crates/forge_main/src/ui.rs`: Initialize color configuration
- Various other files: Updated color usage throughout the codebase

### Testing

The color system includes comprehensive tests covering:

- Color mode parsing from strings
- Color configuration behavior
- Terminal detection
- Color application logic

## Compatibility

This implementation maintains full backward compatibility:

- Default behavior unchanged (colors on terminals, no colors when piped)
- Existing color output preserved for users who don't change settings
- Standard environment variables respected
- No breaking changes to existing APIs

## Examples

### Testing Different Scenarios

```bash
# Test with light terminal theme
forge --color=always

# Test without colors
forge --no-color

# Test with NO_COLOR standard
NO_COLOR=1 forge

# Test auto-detection
forge | cat  # Should have no colors
forge        # Should have colors (if terminal supports them)
```

### Integration with CI/CD

```bash
# In CI environments, colors are automatically disabled when not outputting to a terminal
forge build

# Force colors in CI if needed (for tools that support ANSI)
forge --color=always build

# Explicitly disable colors in CI
NO_COLOR=1 forge build
```

## Future Enhancements

Potential future improvements:

1. **Theme Detection**: Automatically detect light vs dark terminal themes
2. **Custom Color Schemes**: Allow users to define custom color palettes
3. **Configuration File**: Support for persistent color preferences
4. **More Color Options**: Additional color customization options

## Related Issues

This implementation addresses the reported issue where Forge was unusable on light terminal backgrounds due to poor color contrast.