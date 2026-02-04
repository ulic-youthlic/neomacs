# HiDPI Scaling Implementation Plan

## Overview

Implement full HiDPI support based on the design in `2026-02-04-hidpi-scaling-design.md`.

## Tasks

### 1. Add scale_factor to event structure
**File:** `rust/neomacs-display/src/backend/wgpu/events.rs`
- Add `scale_factor: f32` field to `NeomacsInputEvent` struct
- Update C header definition comment

### 2. Store scale_factor in RenderThread
**File:** `rust/neomacs-display/src/render_thread.rs`
- Add `scale_factor: f64` to `RenderApp` struct
- Initialize from `window.scale_factor()` after window creation
- Handle `WindowEvent::ScaleFactorChanged` event
- Send resize event with scale_factor when it changes

### 3. Convert resize events to logical coordinates
**File:** `rust/neomacs-display/src/render_thread.rs`
- In `WindowEvent::Resized` handler, convert physical → logical
- Include scale_factor in resize event sent to Emacs

### 4. Convert mouse coordinates to logical
**File:** `rust/neomacs-display/src/render_thread.rs`
- In `CursorMoved` handler, divide by scale_factor
- In `MouseInput` handler, use logical mouse position

### 5. Add scale_factor to C display info
**File:** `src/neomacsterm.h`
- Add `double scale_factor;` to `struct neomacs_display_info`

### 6. Handle scale_factor in resize events
**File:** `src/neomacsterm.c`
- Extract scale_factor from resize event
- Store in `dpyinfo->scale_factor`
- Update `dpyinfo->resx` and `dpyinfo->resy`

### 7. Scale glyph positions in FFI
**File:** `rust/neomacs-display/src/ffi.rs`
- Get scale_factor from display state
- Multiply glyph x/y positions by scale_factor
- Pass scaled font size to rasterizer

### 8. Update glyph atlas cache key
**File:** `rust/neomacs-display/src/backend/wgpu/glyph_atlas.rs`
- Include scale_factor in `GlyphCacheKey`
- Rasterize glyphs at physical (scaled) size

### 9. Test and verify
- Run on 200% scaled display
- Verify logical window size reported
- Verify crisp text rendering
- Verify mouse click accuracy

## Implementation Order

1. Tasks 1-2: Infrastructure (event struct, RenderThread state)
2. Tasks 3-4: Coordinate conversion (resize, mouse)
3. Tasks 5-6: C side handling
4. Tasks 7-8: Glyph rendering at scale
5. Task 9: Testing

## Dependencies

- Task 3 depends on Task 2 (need scale_factor stored)
- Task 6 depends on Task 5 (need struct field)
- Tasks 7-8 depend on Task 2 (need scale_factor available)
