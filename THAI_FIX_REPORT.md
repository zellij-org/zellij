# Thai Language Rendering Fix — Zellij Analysis Report

## Problem Summary

Thai text renders incorrectly in Zellij. Combining characters (vowel marks like sara am, sara i, sara ue, and tone marks like mai ek, mai tho) are silently dropped. For example:
- "พร้อม" renders as "พรอม" (sara o mai tho missing)
- "สั่งได้" renders as "สงได" (sara a mai ek, sara o mai tho missing)
- "บันทึก" renders as "บนทก" (sara a, sara uee missing)

## Root Cause Analysis

There are **two compounding issues**, both in the server-side terminal grid code:

### Issue 1: `TerminalCharacter` stores only a single `char`

**File:** `zellij-server/src/panes/terminal_character.rs`, line 926-930

```rust
pub struct TerminalCharacter {
    pub character: char,      // <-- Only ONE codepoint
    pub styles: RcCharacterStyles,
    width: u8,
}
```

A Rust `char` is a single Unicode codepoint (U+0000 to U+10FFFF). Thai grapheme clusters require multiple codepoints: a base consonant + zero or more combining marks. For example, "พร้" is three codepoints:
- U+0E1E (พ, base consonant)
- U+0E23 (ร, consonant used as part of cluster)
- U+0E49 (้, combining mark — sara o mai tho)

The struct simply cannot hold a grapheme cluster — only the base character survives.

There is also a deliberate 16-byte size assertion on x86_64 (line 934-935) that constrains the struct size, which would need to be adjusted.

### Issue 2: Zero-width characters are unconditionally dropped

**File:** `zellij-server/src/panes/grid.rs`, lines 1727-1734

```rust
pub fn add_character(&mut self, terminal_character: TerminalCharacter) {
    let character_width = terminal_character.width();
    // Drop zero-width Unicode/UTF-8 codepoints, like for example Variation Selectors.
    if character_width == 0 {
        return;    // <-- Thai combining marks are dropped here
    }
    // ...
}
```

Thai combining characters (U+0E31, U+0E34-U+0E3A, U+0E47-U+0E4E) have width 0 according to `unicode-width`. This code drops ALL zero-width characters, which is correct for some codepoints (like variation selectors) but wrong for combining marks that should attach to the preceding character.

The comment even acknowledges this breaks grapheme segmentation, referencing issue #1538.

### The Data Flow

In `Grid::print()` (line 3194), each `char` from the VTE parser arrives individually:

```rust
fn print(&mut self, c: char) {
    let c = self.cursor.charsets[self.active_charset].map(c);
    let terminal_character =
        TerminalCharacter::new_styled(c, self.cursor.pending_styles.clone());
    self.set_preceding_character(terminal_character.clone());
    self.add_character(terminal_character);
}
```

When the terminal sends "พร้", the VTE parser calls `print()` three times:
1. `print('พ')` — width 1, stored normally
2. `print('ร')` — width 1, stored normally
3. `print('้')` — width 0, **dropped by `add_character()`**

### Output Path Confirms Single-Char Assumption

In `zellij-server/src/output/mod.rs`, lines 192 and 257:

```rust
vte_output.push(t_character.character);
```

Only a single `char` is pushed per `TerminalCharacter`, confirming the entire pipeline assumes one codepoint per cell.

## Proposed Fix

The fix requires changes across several locations. Here is the approach, modeled after how Alacritty and other terminal emulators handle this:

### Step 1: Extend `TerminalCharacter` to hold combining characters

**File:** `zellij-server/src/panes/terminal_character.rs`

Change the struct to optionally store extra combining codepoints:

```rust
pub struct TerminalCharacter {
    pub character: char,
    pub combining_chars: Option<Box<Vec<char>>>,  // None for most chars (no alloc overhead)
    pub styles: RcCharacterStyles,
    width: u8,
}
```

Using `Option<Box<Vec<char>>>` keeps the common case (ASCII/single codepoint) at the same size — `Option<Box<...>>` is pointer-sized (8 bytes) and `None` is zero-cost. The size assertion will need updating from 16 to 24 bytes.

Add a method to append combining characters:

```rust
impl TerminalCharacter {
    pub fn append_combining(&mut self, c: char) {
        match &mut self.combining_chars {
            Some(chars) => chars.push(c),
            None => self.combining_chars = Some(Box::new(vec![c])),
        }
    }
}
```

### Step 2: Modify `Grid::add_character()` to attach combining marks

**File:** `zellij-server/src/panes/grid.rs`

Instead of unconditionally dropping zero-width characters, check if they are combining marks and attach them to the preceding character:

```rust
pub fn add_character(&mut self, terminal_character: TerminalCharacter) {
    let character_width = terminal_character.width();
    if character_width == 0 {
        // If this is a combining character, attach it to the preceding cell
        if is_combining_character(terminal_character.character) {
            if let Some(prev) = self.get_character_under_cursor_mut_or_preceding() {
                prev.append_combining(terminal_character.character);
            }
        }
        // Non-combining zero-width chars (variation selectors etc.) still dropped
        return;
    }
    // ... rest unchanged
}
```

For detecting combining characters, use the Unicode general category. Characters in the "Mark" category (Mn, Mc, Me) are combining. Thai combining marks are in category Mn (Nonspacing_Mark). You can use the `unicode-general-category` crate or a simple range check:

```rust
fn is_combining_character(c: char) -> bool {
    // Unicode General Category M (Mark): Mn, Mc, Me
    // Quick check for common combining ranges:
    let cp = c as u32;
    matches!(
        unicode_general_category::get_general_category(c),
        unicode_general_category::GeneralCategory::NonspacingMark
        | unicode_general_category::GeneralCategory::SpacingMark
        | unicode_general_category::GeneralCategory::EnclosingMark
    )
}
```

Alternatively, a simpler approach without a new dependency — check specific Unicode ranges:

```rust
fn is_combining_character(c: char) -> bool {
    let cp = c as u32;
    // Common combining mark ranges (covers Thai, Latin diacritics, Arabic, etc.)
    (0x0300..=0x036F).contains(&cp) ||  // Combining Diacritical Marks
    (0x0E31..=0x0E31).contains(&cp) ||  // Thai character sara am
    (0x0E34..=0x0E3A).contains(&cp) ||  // Thai vowel marks
    (0x0E47..=0x0E4E).contains(&cp) ||  // Thai tone marks and other signs
    (0x0EB1..=0x0EB1).contains(&cp) ||  // Lao
    (0x0EB4..=0x0EBC).contains(&cp) ||  // Lao
    (0x1DC0..=0x1DFF).contains(&cp) ||  // Combining Diacritical Marks Supplement
    (0x20D0..=0x20FF).contains(&cp) ||  // Combining Diacritical Marks for Symbols
    (0xFE20..=0xFE2F).contains(&cp)     // Combining Half Marks
    // ... more ranges as needed
}
```

### Step 3: Update `Grid::print()` to also set the preceding character correctly

**File:** `zellij-server/src/panes/grid.rs`

The `set_preceding_character` call should only happen for non-combining characters, so that combining marks know which character to attach to:

```rust
fn print(&mut self, c: char) {
    let c = self.cursor.charsets[self.active_charset].map(c);
    let terminal_character =
        TerminalCharacter::new_styled(c, self.cursor.pending_styles.clone());

    // Only update preceding_char for non-combining characters
    if terminal_character.width() > 0 {
        self.set_preceding_character(terminal_character.clone());
    }

    self.add_character(terminal_character);
}
```

### Step 4: Update output rendering to emit combining characters

**File:** `zellij-server/src/output/mod.rs`, lines 192 and 257

```rust
// Before:
vte_output.push(t_character.character);

// After:
vte_output.push(t_character.character);
if let Some(combining) = &t_character.combining_chars {
    for &c in combining.iter() {
        vte_output.push(c);
    }
}
```

### Step 5: Update Debug impl

**File:** `zellij-server/src/panes/terminal_character.rs`, line 971-974

```rust
impl ::std::fmt::Debug for TerminalCharacter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.character)?;
        if let Some(combining) = &self.combining_chars {
            for c in combining.iter() {
                write!(f, "{}", c)?;
            }
        }
        Ok(())
    }
}
```

### Step 6: Update the size assertion

```rust
// Adjust from 16 to 24 due to Option<Box<Vec<char>>> field
#[cfg(target_arch = "x86_64")]
const _: [(); 24] = [(); std::mem::size_of::<TerminalCharacter>()];
```

## Impact Assessment

- **Memory:** For ASCII/Latin text (the common case), `Option<Box<Vec<char>>>` is `None` which is 8 bytes (pointer-sized). The struct grows from 16 to 24 bytes on x86_64. This is a 50% increase per cell. For a typical 200x50 terminal, that's 10,000 cells * 8 bytes = 80KB additional — negligible.
- **Performance:** The hot path (ASCII characters) adds only a `None` check, which branch prediction handles well.
- **Correctness:** This fix would correctly handle not just Thai, but also:
  - Vietnamese with combining diacritics
  - Arabic combining marks
  - Hebrew niqqud (vowel points)
  - Latin combining diacritics (e.g., decomposed e + acute = é)
  - Devanagari and other Indic scripts

## Alternative Approach: String-based storage

A more comprehensive but invasive approach would be to change `character: char` to `character: CompactString` or similar, storing the full grapheme cluster as a string. This is what Alacritty does with its `Cell` type. However, this would be a larger refactor with more significant memory implications and would require changes throughout the codebase wherever `.character` is accessed.

## Files That Need Changes

1. `/tmp/zellij-fix/zellij-server/src/panes/terminal_character.rs` — struct definition, constructors, Debug impl
2. `/tmp/zellij-fix/zellij-server/src/panes/grid.rs` — `add_character()`, `print()`
3. `/tmp/zellij-fix/zellij-server/src/output/mod.rs` — rendering output (2 locations)
4. `/tmp/zellij-fix/Cargo.toml` — possibly add `unicode-general-category` crate (if using the crate approach)

## Related Issues

- https://github.com/zellij-org/zellij/issues/3667 — Decomposed Unicode characters (same root cause)
- https://github.com/zellij-org/zellij/issues/1538 — Grapheme segmentation (acknowledged in code comment)
