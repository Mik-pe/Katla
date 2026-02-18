# Text Rendering Improvements

> **LIVE DOCUMENT** - Updated as implementation progresses

## Overview

This document tracks the implementation of "perfect text rendering" in katla_ui,
based on research from egui, cosmic-text, and other font rendering libraries.

## Current Status: Complete ✅

Text renders with subpixel positioning AND stable movement!

## Goals

- Crisp text at any position (subpixel positioning) ✅
- Proper character spacing (kerning) ✅
- Stable panel movement (no wiggling) ✅
- Optional: Advanced text shaping (ligatures, RTL)

---

## Implementation Status

| Phase | Feature | Status | Notes |
|-------|---------|--------|-------|
| 1 | Subpixel Binning | ✅ Complete | 4 bins with consistent metrics |
| 2 | Pair Kerning | ✅ Complete | Adjust spacing between character pairs |
| 3 | Gamma Correction | ✅ Complete | Perceptually uniform text weight |
| 4 | Font Hinting | ⏸️ Blocked | Requires ab_glyph upgrade or different library |
| 5 | Text Shaping | ⏸️ Blocked | Requires cosmic-text or harfrust integration |

**Note**: Phases 4 and 5 require switching from `ab_glyph` to a more sophisticated library like `cosmic-text`, `swash`, or `skrifa` which support font hinting and text shaping. This is a larger refactoring effort.

### Key Fix: Consistent Metrics Across Bins

The wiggling issue was caused by **varying glyph metrics** between subpixel bins. When the subpixel offset shifted the glyph bounds, the `width`, `offset_x`, and `top_offset` all varied slightly.

**Solution**: Calculate ALL metrics from the **unshifted glyph bounds** (position 0,0), not from the subpixel-shifted bounds:

```rust
// Get consistent metrics from unshifted position
let glyph_for_metrics = Glyph {
    id: glyph_id,
    scale: PxScale::from(physical_size),
    position: ab_glyph::point(0.0, 0.0), // No subpixel offset!
};
let metrics_bounds = font.outline_glyph(glyph_for_metrics)...;

// Use consistent metrics for size AND positioning
let width = metrics_bounds.width().ceil() as usize;
let offset_x = metrics_bounds.min.x / scale_factor;
let top_offset = -metrics_bounds.min.y / scale_factor;
```

Now all 4 subpixel bins have **identical size and positioning metrics**, so switching bins doesn't cause visual jumps!

---

## Phase 1: Subpixel Binning ✅

### Problem

Current implementation snaps glyph positions to integer pixels via `round()`.
This causes text to "jump" when animating and loses sharpness at fractional positions.

### Solution

Use 4 subpixel bins (0.0, 0.25, 0.5, 0.75) for horizontal positioning.
Each bin caches a separate version of the glyph, shifted by the subpixel offset.

### Implementation Details

#### SubpixelBin Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubpixelBin {
    Zero,   // 0.0
    One,    // 0.25
    Two,    // 0.5
    Three,  // 0.75
}
```

#### Cache Key Update

**Before:** `(FontId, char, FontSizeKey, ScaleFactorKey)`

**After:** `(FontId, char, FontSizeKey, ScaleFactorKey, SubpixelBin)`

#### Rasterization Changes

1. Calculate subpixel bin from fractional X position
2. Shift glyph by subpixel offset when rasterizing
3. Store in cache with bin as part of key

#### Important: Consistent Positioning

To prevent glyphs from "wobbling" when using different subpixel bins, we calculate
both `offset_x` and `top_offset` from the glyph's bounds at position (0, 0), NOT
from the subpixel-shifted bounds. This ensures all subpixel bins for the same
character have identical positioning metrics while still getting crisp horizontal
rendering from the baked-in subpixel texture offset.

```rust
// Get bounds at origin (0,0) for consistent positioning metrics
let glyph_for_metrics = Glyph {
    id: glyph_id,
    scale: PxScale::from(physical_size),
    position: ab_glyph::point(0.0, 0.0), // No subpixel offset for metrics
};
let metrics_bounds = font.outline_glyph(glyph_for_metrics)
    .map(|g| g.px_bounds())
    .unwrap_or(bounds);

// Both offsets use consistent metrics (NOT affected by subpixel bin)
let offset_x = metrics_bounds.min.x / scale_factor;
let top_offset = -metrics_bounds.min.y / scale_factor;
```

#### Rendering Logic

When rendering, we:
1. Calculate subpixel bin from the **text start position** (all chars share same bin)
2. Floor the start position to an integer
3. Render each character at `floor_start + accumulated_offset + glyph.offset_x`
4. The subpixel offset is baked into the glyph texture

**Key insight from egui**: If you calculate the bin per-character, each character
can "jump" to a different bin independently when the panel moves, causing a
"wiggling" effect. By using the text start position, all characters shift together.

#### Critical: Relative Positioning

All character positions must be calculated RELATIVE to the floored start position.
If each character's position is rounded independently, they "jump" at different
times as the panel moves:

```rust
// WRONG: Each character rounds independently - causes wiggling
let glyph_left = cursor_x + glyph.offset_x;
let pos_x = glyph_left.round(); // Each char rounds differently!

// CORRECT: All positions relative to fixed start point
let (floor_x, subpixel_bin) = SubpixelBin::new(position.x());
let start_x = floor_x as f32;
let pos_x = start_x + cursor_offset + glyph.offset_x; // Relative to fixed point
```

### Files Modified

- `katla_ui/src/text/mod.rs` - SubpixelBin implementation, cache key update
- `katla_ui/src/context.rs` - Use subpixel-aware positioning in draw_text()

### Progress

- [x] Add SubpixelBin enum
- [x] Update cache key structure
- [x] Modify get_or_rasterize to accept subpixel bin
- [x] Update draw_text to calculate and use subpixel bins
- [x] Add unit tests for SubpixelBin
- [x] Fix vertical wobble: use consistent glyph metrics for vertical positioning
- [x] Fix character wiggling: calculate positions relative to floored start position

---

## Phase 2: Pair Kerning ✅

### Problem

Character pairs like "AV", "Te", "Wo" have incorrect spacing because
the current implementation doesn't consider kerning tables.

### Solution

Use ab_glyph's built-in kerning support to adjust spacing between
adjacent characters.

### Implementation Details

```rust
pub fn get_kerning(
    &self,
    font_id: FontId,
    left: char,
    right: char,
    size: f32,
    scale_factor: f32,
) -> f32 {
    let Some(font) = self.fonts.get(&font_id) else {
        return 0.0;
    };

    let left_id = font.glyph_id(left);
    let right_id = font.glyph_id(right);
    let unscaled_kern = font.kern_unscaled(left_id, right_id);

    let physical_size = size * scale_factor;
    let scaled_kern = unscaled_kern * physical_size / font.units_per_em().unwrap_or(1.0);
    scaled_kern / scale_factor
}
```

### Files Modified

- `katla_ui/src/text/mod.rs` - Added get_kerning() method, updated measure_text()
- `katla_ui/src/context.rs` - Updated draw_text() to apply kerning

### Progress

- [x] Add get_kerning() method to FontSystem
- [x] Update draw_text() to apply kerning between characters
- [x] Update measure_text() to include kerning
- [x] Test compilation and existing tests

---

## Phase 3: Gamma Correction

### Problem

Alpha values for text rendering aren't perceptually uniform.
Text can appear too thin or too thick depending on blending.

### Solution

Apply gamma-aware alpha correction when rasterizing or blending.

```rust
pub fn alpha_from_coverage(coverage: f32) -> f32 {
    coverage.powf(1.0 / 1.45)
}
```

### Progress

- [ ] Add gamma correction functions
- [ ] Apply in glyph rasterization or shader

---

## Phase 4: Font Hinting

### Problem

Glyph stems don't align to pixel grid, causing blurry rendering.

### Solution

Enable ab_glyph's hinting support to snap stems to pixel boundaries.

### Progress

- [ ] Enable hinting in DrawSettings
- [ ] Test with various fonts and sizes

---

## Phase 5: Text Shaping (Future)

### Problem

No support for ligatures (fi, fl), RTL scripts, or complex text layout.

### Solution Options

1. **cosmic-text integration** - Full shaping pipeline
2. **Simple ligature table** - Basic common ligatures only

### Progress

- [ ] Evaluate options
- [ ] Implement chosen solution

---

## Research References

- **egui** - `SubpixelBin` pattern, `alpha_from_coverage`, skrifa + vello_cpu
- **cosmic-text** - Same `SubpixelBin`, harfrust for shaping, swash for rasterization
- **ab_glyph** - Current library, supports kerning, hinting available via DrawSettings

---

## Testing

Visual testing should cover:
- Text at various fractional positions
- Animation of text position
- Common kerning pairs: AV, Te, Wo, To, Ya
- Different font sizes
- Scale factor changes (DPI scaling)
