# Strategy 4: Full Rust Display Engine

Replace Emacs's C display engine (`xdisp.c`, `dispnew.c`, ~40k LOC) with a Rust layout engine that reads buffer data directly and produces GPU-ready glyph batches.

## Current Architecture

```
Emacs redisplay (C, xdisp.c ~30k LOC)
  → glyph matrices (current_matrix)
    → neomacsterm.c extracts glyphs (FFI boundary)
      → FrameGlyphBuffer sent to Rust via crossbeam
        → wgpu renders
```

### Pain Points

- **Double work**: Emacs builds a full CPU-side glyph matrix, then we serialize it again into `FrameGlyphBuffer`.
- **FFI friction**: Every new feature (cursors, borders, images, animations) requires C-side extraction code + Rust-side handling.
- **CPU-centric design**: Emacs redisplay was designed for `XDrawString` / terminal cells, not GPU batched rendering.
- **No GPU awareness**: Layout doesn't know about GPU capabilities (instancing, atlas caching, compute shaders).

## Proposed Architecture

```
Emacs buffer/overlay/face data
  → C serializes LayoutSnapshot
    → Rust reads snapshot directly
      → Rust layout engine (line breaking, wrapping, positioning)
        → GPU-ready glyph batches
          → wgpu renders
```

## What We Keep vs Replace

### Keep (Emacs C/Lisp)

- Buffer management (gap buffer, undo, markers)
- Text properties and overlays (Lisp-level API)
- Window tree management (splits, sizing)
- Face definitions and merging (Lisp-level `defface`)
- Fontset/font selection logic
- Mode-line format evaluation (`format-mode-line`)

### Replace (Rust)

- The display iterator (`struct it` — 550+ fields state machine in `xdisp.c`)
- Line layout (`display_line()` — wrapping, truncation, alignment)
- Glyph production (`produce_glyphs()` — metrics, positioning)
- Matrix management (`dispnew.c` — desired/current diff)
- The extraction layer (`neomacsterm.c` glyph walking)

## Reading Lisp Data from Rust

Rust needs access to:

| Data | Where it lives | Access pattern |
|------|----------------|----------------|
| Buffer text | Gap buffer (`BUF_BYTE_ADDRESS`) | Contiguous reads around gap |
| Text properties | Interval tree on buffer | Walk intervals for face/display/invisible |
| Overlays | Sorted linked lists on buffer | Scan overlays at each position |
| Faces | `face_cache->faces_by_id[]` on frame | Lookup by ID, merge multiple |
| Window geometry | `struct window` fields | Read pixel_left/top/width/height |
| Window-start | Marker on window | Read marker position |
| Font metrics | `struct font` on face | Ascent, descent, average width |

### Approach A: Snapshot (recommended, start here)

At `update_end`, C serializes a **layout snapshot** to Rust:

```rust
struct LayoutSnapshot {
    windows: Vec<WindowSnapshot>,
}

struct WindowSnapshot {
    id: i64,
    bounds: Rect,
    buffer: BufferSnapshot,
    window_start: usize,
    hscroll: i32,
    selected: bool,
}

struct BufferSnapshot {
    text: Vec<u8>,                      // Full buffer text (no gap)
    intervals: Vec<PropertyInterval>,   // Text property spans
    overlays: Vec<OverlaySpan>,         // Active overlays
}

struct PropertyInterval {
    start: usize,
    end: usize,
    face_id: Option<u32>,
    display: Option<DisplaySpec>,
    invisible: bool,
}
```

**Pros**: Clean Rust ownership, no unsafe, thread-safe.
**Cons**: Copies all buffer text every frame (~0.1ms for 1MB buffer).

### Approach B: Shared-memory / FFI read (optimize later)

Rust reads Emacs data structures directly via `unsafe` FFI pointers:

```rust
unsafe fn read_buffer_char(buf: *const EmacsBuffer, pos: usize) -> char {
    // Handle gap buffer directly
}

unsafe fn get_face(frame: *const EmacsFrame, face_id: u32) -> &Face {
    // Read from face_cache->faces_by_id[face_id]
}
```

**Pros**: Zero-copy, instant access.
**Cons**: Extremely unsafe, must synchronize with Emacs thread, Emacs struct layout changes break everything.

**Recommendation**: Start with Approach A, optimize to B later for large buffers. The snapshot cost is negligible for typical buffer sizes (<100KB).

## Phased Implementation

### Phase 0: Layout Snapshot Infrastructure

Add C function `neomacs_build_layout_snapshot()` that serializes buffer text + property intervals + overlays into a flat buffer. Send snapshot to Rust alongside (or replacing) `FrameGlyphBuffer`. Rust ignores snapshot initially, still uses old glyph path.

**Scope**: ~500 C, ~300 Rust.

### Phase 1: Monospace ASCII Layout Engine

Build `RustLayoutEngine` that handles the simplest case:

- Fixed-width font, single face, no overlays, no display properties
- Line breaking at window width (or `truncate-lines`)
- Cursor positioning
- Window-start / point tracking

This alone covers ~70% of what you see in a typical coding buffer.

**Scope**: ~1500 Rust. **Difficulty**: Medium.

### Phase 2: Face Resolution

- Read face intervals from snapshot
- Apply face attributes (fg, bg, bold, italic, underline)
- Handle face merging (text property + overlay stacking)
- Use existing cosmic-text for font selection per face

**Scope**: ~800 Rust. **Difficulty**: Medium.

### Phase 3: Display Properties

- `invisible` — skip text ranges
- `display` strings — replace text with overlay strings
- `display` spaces — `(space :align-to N)` alignment
- `before-string` / `after-string` — overlay inserted text
- `line-prefix` / `wrap-prefix` — indentation on wrapped lines

**Scope**: ~2000 Rust. **Difficulty**: Hard.

### Phase 4: Mode-line & Header-line

- Evaluate mode-line format (still in Lisp via `format-mode-line`)
- C sends pre-formatted mode-line string + faces to Rust
- Rust lays out the mode-line row

**Scope**: ~500 Rust. **Difficulty**: Medium.

### Phase 5: Variable-width & Compositions

- Variable-width font support (cosmic-text already handles this)
- Emoji/composition support (already working in current system)
- Ligatures (cosmic-text + rustybuzz)

**Scope**: ~800 Rust. **Difficulty**: Medium.

### Phase 6: Bidi

- Integrate `unicode-bidi` crate for reordering
- Handle mixed LTR/RTL paragraphs
- This is the hardest single feature

**Scope**: ~1500 Rust. **Difficulty**: Very hard.

### Phase 7: Images & Media

- Inline images (already rendered by wgpu, just need layout positioning)
- Video/WebKit (same — just need position from layout)

**Scope**: ~400 Rust. **Difficulty**: Easy.

### Summary

| Phase | LOC (Rust) | Difficulty | Enables |
|-------|-----------|------------|---------|
| 0: Snapshot infra | ~500 C, ~300 Rust | Medium | Foundation |
| 1: Monospace ASCII | ~1500 Rust | Medium | Basic editing |
| 2: Faces | ~800 Rust | Medium | Syntax highlighting |
| 3: Display props | ~2000 Rust | Hard | Packages (company, which-key) |
| 4: Mode-line | ~500 Rust | Medium | Status display |
| 5: Variable-width | ~800 Rust | Medium | Proportional fonts |
| 6: Bidi | ~1500 Rust | Very hard | International text |
| 7: Images/media | ~400 Rust | Easy | Already working |

Total: ~8-10k LOC Rust replacing ~30k LOC C. The reduction comes from:

- No terminal backend code needed
- No X11/GTK drawing code needed
- No incremental matrix diffing (GPU redraws everything)
- Modern Rust text crates handle Unicode/shaping complexity

## What This Unlocks

1. **Pixel-level smooth scrolling** — No more line-granular `window-start`. Rust can scroll by arbitrary pixel offsets, interpolating between lines. Feed back the logical position to Emacs lazily.

2. **Sub-frame cursor movement** — Cursor position computed in render thread, animated instantly without waiting for Emacs redisplay cycle.

3. **Parallel layout** — Layout runs on render thread while Emacs executes Lisp. Only need new snapshot when buffer actually changes.

4. **Incremental layout** — Since Rust owns the layout state, it can diff against previous snapshot and only re-layout changed regions (unlike current clear-and-rebuild).

5. **Ligatures everywhere** — cosmic-text + rustybuzz handle OpenType ligatures natively. No need for Emacs composition hacks.

6. **Custom rendering effects** — Since layout and render are in the same language, can do things like animated text insertion, per-character fade-in, elastic overscroll.

## Compatibility Risk

The biggest risk is Emacs packages that depend on redisplay behavior:

- `posframe` — creates child frames with specific pixel positions
- `company-mode` / `corfu` — popup overlays positioned relative to point
- `which-key` — positioned popups
- `org-mode` — heavy use of display properties, invisible text, overlays

**Mitigation**: Keep Emacs's `window-start`, `pos-visible-in-window-p`, `posn-at-point` working by having Rust report layout results back to Emacs. The Lisp API stays the same, only the engine behind it changes.

## Alternative Considered: GPU Compute Layout (Strategy 5)

Rejected. GPU compute shaders for text layout would push line breaking, glyph positioning, and wrapping to the GPU. However:

- **Line breaking is inherently sequential** — can't know where line N+1 starts until line N finishes. GPU parallelism doesn't help.
- **Bidi is impossible on GPU** — the Unicode Bidi Algorithm has deeply sequential state (embedding levels, bracket matching). Not expressible in WGSL.
- **Text shaping must stay on CPU** — HarfBuzz/rustybuzz needs CPU access to font tables for ligatures, kerning, mark attachment.
- **Branching kills GPU perf** — overlays, display properties, invisible text, variable-width fonts all require per-glyph branching. GPUs hate divergent branches.
- **No ecosystem** — no existing references for GPU text layout in editors.
- **The bottleneck doesn't exist** — a typical frame has ~3000-8000 visible glyphs. CPU layout for that is <1ms.

Strategy 4 (CPU Rust layout + GPU render) gives 95% of the performance benefit with 10% of the complexity. Every modern editor (VS Code, Zed, Lapce, Alacritty) uses this architecture.

## TUI Rendering Backend

A major benefit of owning layout in Rust: one layout engine, multiple renderers. The same `RustLayoutEngine` that produces glyph batches for wgpu can also output to a terminal grid, giving us a true TUI Emacs.

### Architecture

```
                              ┌─→ WgpuRenderer (GPU)
LayoutSnapshot → RustLayout ──┤
                              └─→ TuiRenderer (terminal)
```

The layout engine produces a backend-agnostic intermediate representation:

```rust
struct LayoutOutput {
    rows: Vec<LayoutRow>,
}

struct LayoutRow {
    glyphs: Vec<LayoutGlyph>,
    y: f32,
    height: f32,
}

struct LayoutGlyph {
    char: char,
    x: f32,
    width: f32,
    face_id: u32,
    is_cursor: bool,
    // ... other attributes
}
```

The **WgpuRenderer** consumes this as pixel-positioned glyph batches (current path). The **TuiRenderer** maps this to a cell grid:

```rust
struct TuiRenderer {
    terminal: crossterm::Terminal,   // or termwiz / ratatui backend
    grid: Vec<Vec<Cell>>,           // rows × cols cell grid
    prev_grid: Vec<Vec<Cell>>,      // previous frame for diffing
}

struct Cell {
    char: char,
    fg: Color,
    bg: Color,
    attrs: CellAttrs,  // bold, italic, underline, strikethrough
}
```

### How TUI Rendering Works

1. **Layout**: `RustLayoutEngine` produces `LayoutOutput` with pixel coordinates
2. **Quantize**: TuiRenderer maps pixel positions to cell grid (divide by cell width/height)
3. **Diff**: Compare current grid against previous grid
4. **Emit**: Output only changed cells via ANSI escape sequences

### Terminal Features

| Feature | GPU (wgpu) | TUI (terminal) |
|---------|-----------|----------------|
| Text rendering | Glyph atlas + shader | ANSI escape sequences |
| Colors | 32-bit RGBA linear | 256-color / 24-bit truecolor |
| Bold/italic | Font variant selection | SGR attributes |
| Underline | Custom pixel drawing | SGR underline (wavy if supported) |
| Images | GPU texture | Sixel / Kitty graphics protocol |
| Cursor | Animated, blinking | Terminal cursor escape |
| Smooth scroll | Pixel-level | Line-level (or pixel with Kitty) |
| Ligatures | Full OpenType | Not possible (cell grid) |
| Variable-width | Full support | Monospace only |
| Box drawing | SDF rounded rects | Unicode box characters |
| Video/WebKit | Inline rendering | Not supported |
| Mouse | Full pixel tracking | Cell-level tracking |
| DPI scaling | Automatic | Terminal handles it |
| Performance | 120fps GPU | 60fps terminal refresh |

### Crate Choices

- **crossterm** — Cross-platform terminal manipulation (input, output, raw mode). Mature, widely used.
- **ratatui** — TUI framework built on crossterm. Provides widget abstractions, but we may only need the backend layer since we have our own layout.
- **termwiz** — Alternative from wezterm project. Better Kitty graphics protocol support.

### TUI-Specific Considerations

**Cell grid quantization**: The layout engine works in pixel coordinates. For TUI, we quantize:
```rust
let col = (glyph.x / cell_width).floor() as usize;
let row = (glyph.y / cell_height).floor() as usize;
```

Wide characters (CJK) occupy 2 cells. The layout engine already knows character widths from cosmic-text; TUI renderer uses `unicode-width` crate to determine cell count.

**Color mapping**: Layout faces use 32-bit sRGB colors. TUI renderer maps to:
- 24-bit truecolor (most modern terminals)
- 256-color palette (fallback)
- 16-color ANSI (minimal fallback)

Detection via `COLORTERM=truecolor` environment variable or terminfo capabilities.

**Inline images**: Modern terminals support image protocols:
- **Kitty graphics protocol** — pixel-perfect, widely supported
- **Sixel** — older but broadly compatible
- **iTerm2 inline images** — macOS terminals

The TUI renderer can optionally support these for `IMAGE_GLYPH` layout items.

**Diffing for performance**: Unlike GPU (clear-and-rebuild each frame), terminals are slow to redraw. The TUI renderer must diff current vs previous grid and only emit changes. This is standard practice (ncurses, crossterm, ratatui all do this).

### Implementation Phase

TUI backend fits as an additional phase after Phase 1 (monospace ASCII):

**Phase 1.5: TUI Renderer**

- Implement `TuiRenderer` that consumes `LayoutOutput`
- Cell grid quantization from pixel coordinates
- ANSI escape sequence output via crossterm
- Grid diffing for incremental updates
- Basic face → SGR attribute mapping
- Cursor display via terminal cursor

**Scope**: ~1200 Rust. **Difficulty**: Medium.

This phase can proceed in parallel with Phases 2-7 since TUI and GPU renderers consume the same layout output. Each layout feature (faces, display props, bidi) automatically works in both renderers once the layout engine supports it.

### Use Cases

- **SSH sessions** — Full Emacs over SSH without X11 forwarding or GPU
- **Containers / CI** — Emacs in Docker, headless servers
- **Low-resource machines** — No GPU required
- **Terminal multiplexers** — Works inside tmux, screen, zellij
- **Accessibility** — Screen readers work with terminal output
- **Testing** — Deterministic text output for layout regression tests
