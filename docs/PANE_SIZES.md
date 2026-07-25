# Pane Size Configuration Guide

This guide explains how to configure pane sizes in Zellij, covering both CLI commands and KDL layouts.

## Quick Reference

**CLI format:** `zellij run --size WIDTH,HEIGHT COMMAND`
- **Fixed dimensions:** `--size 80,24 echo "hello"` (exact columns/rows)
- **Percentage dimensions:** `--size 50%,50% echo "hello"` (flexible percentages)

**KDL format:**
```kdl
layout {
    pane split_direction="Horizontal" {
        pane size "30%"   // percentage - flexible with terminal resize
        pane size "25"    // fixed - exact 25 cols, doesn't resize
    }
}
```

## Size Types

### Fixed Sizes (`size "N"` or `--size N,M`)
- **Exact dimensions:** Creates panes with specific column/row counts (e.g., `size "25"` = exactly 25 columns)
- **Non-flexible:** Does NOT resize when terminal is resized - maintains absolute pixel count
- **Use case:** Status bars, fixed-width tools, terminals that need exact sizing

### Percentage Sizes (`size "N%"` or `--size N%,M%`)
- **Flexible dimensions:** Creates panes proportional to terminal size (e.g., `size "50%"` = half the width)
- **Resize-aware:** Automatically scales when terminal window changes size
- **Use case:** Main editor panes, flexible layouts that adapt to screen

## CLI vs Layout Precedence Rules

### When Both Specify Size
If you specify a custom pane size in BOTH the `--size` CLI argument AND a KDL layout file:

1. **CLI takes precedence** - The `zellij run --size 80,24 echo "hello"` command creates an 80×24 fixed pane regardless of what's in your layout file
2. **Layout is ignored for that pane** - Only applies to new panes created via CLI commands

### When Only One Specifies Size
- **CLI-only:** Layout defaults apply (flexible percentages based on split direction)
- **Layout-only:** KDL size attribute controls the pane behavior

## Common Patterns

### Pattern 1: Fixed Status Bar + Flexible Editor
```kdl
layout {
    pane split_direction="Horizontal" {
        pane size "3"   // fixed 3-row status bar
        pane            // flexible - takes remaining space (default %)
    }
}
```

### Pattern 2: Split Terminal with Fixed Width
```kdl
zellij run --size 40,24 code .       // fixed 40-column editor
zellij run --size 60%,50% vim file   // flexible half-screen terminal
```

### Pattern 3: Floating Panes (Always Flexible)
Floating panes use percentage constraints by design. The `x`, `y`, `width`, `height` attributes in floating pane KDL will preserve their constraint type:
```kdl
floating_panes {
    pane x "10%" y "10%" width "30%" height "20%"  // all percentages - flexible!
}
```

## Known Limitations

### Mixed Size Types
When a layout contains BOTH fixed and percentage panes:
- **Terminal resize** will stretch percentage panes but leave fixed panes at their absolute size
- This can create gaps/overlaps if the total doesn't match terminal dimensions
- The cassowary constraint solver handles this gracefully, but expect some layout drift

### Floating Panes with Percentage
Floating panes specified with percentages (e.g., `width "50%"`) remain flexible at runtime. They DO NOT convert to fixed constraints - they resize proportionally when the viewport changes.

## Best Practices

1. **Use fixed sizes for UI elements** (status bars, tab bars) that shouldn't change
2. **Use percentages for content areas** (editors, terminals) that should adapt to screen size
3. **Test with different terminal sizes** - verify your layout works at 80×24, 120×40, etc.
4. **Document fixed constraints** in your layout comments so others understand why certain panes don't resize

## Migration from Old Behavior

Previously (before PR-2804):
- All sizes defaulted to flexible percentages
- No way to specify exact dimensions

After PR-2804:
- Explicit `size` attribute controls behavior
- CLI `--size` argument provides fine-grained control
- Constraint type preserved through layout parsing and runtime application
