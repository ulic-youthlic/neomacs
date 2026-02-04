# Unified DMA-BUF Media Stack Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement a unified zero-copy media stack where Images, Video, and WebKit all share the same DMA-BUF/ExternalBuffer infrastructure for maximum GPU performance.

**Architecture:** All media sources produce `ExternalBuffer` instances (either `DmaBufBuffer` for zero-copy or `SharedMemoryBuffer` as fallback). The unified import layer converts these to wgpu textures via `vulkan_dmabuf.rs` (HAL zero-copy first, mmap fallback second).

**Tech Stack:** Rust, wgpu, ash (Vulkan), GStreamer (video), WPE-WebKit (browser), image-rs (image decoding)

---

## Current State

### Working Components
- `ExternalBuffer` trait with `to_wgpu_texture()` method
- `DmaBufBuffer` - multi-plane DMA-BUF support (Linux)
- `SharedMemoryBuffer` - CPU fallback with format conversion
- `vulkan_dmabuf.rs` - HAL zero-copy import + mmap fallback
- `ImageCache` - async file/data loading via thread pool (returns RGBA)
- `VideoCache` - GStreamer VA-API with DMA-BUF extraction
- `DmaBufExporter` - WPE-WebKit EGL DMA-BUF export

### Gaps to Fill
1. Image FFI stubs: `load_image_data`, `load_image_argb32`, `load_image_rgb24` return 0
2. No raw pixel data loading path in ImageCache
3. No unified memory budget across media types
4. Images always go CPU→GPU, never use DMA-BUF path

---

## Task 1: Implement Raw Pixel Data Loading in ImageCache

**Files:**
- Modify: `rust/neomacs-display/src/backend/wgpu/image_cache.rs:306-348`

**Step 1: Add new ImageSource variant for raw pixels**

In `image_cache.rs`, find the `ImageSource` enum (around line 99) and add:

```rust
/// Image source
enum ImageSource {
    File(String),
    Data(Vec<u8>),
    /// Raw ARGB32 pixel data (A,R,G,B byte order, 4 bytes per pixel)
    RawArgb32 {
        data: Vec<u8>,
        width: u32,
        height: u32,
        stride: u32,
    },
    /// Raw RGB24 pixel data (R,G,B byte order, 3 bytes per pixel)
    RawRgb24 {
        data: Vec<u8>,
        width: u32,
        height: u32,
        stride: u32,
    },
}
```

**Step 2: Add load_raw_argb32 method**

After `load_data` method (around line 348), add:

```rust
/// Load image from raw ARGB32 pixel data (immediate - no async decode needed)
pub fn load_raw_argb32(
    &mut self,
    data: &[u8],
    width: u32,
    height: u32,
    stride: u32,
) -> u32 {
    let id = self.next_id.fetch_add(1, Ordering::SeqCst);

    // Store dimensions immediately
    self.pending_dimensions.insert(id, ImageDimensions { width, height });

    // Queue for processing (will convert ARGB32 -> RGBA)
    self.states.insert(id, ImageState::Pending);
    let _ = self.decode_tx.send(DecodeRequest {
        id,
        source: ImageSource::RawArgb32 {
            data: data.to_vec(),
            width,
            height,
            stride,
        },
        max_width: 0,
        max_height: 0,
    });

    id
}

/// Load image from raw RGB24 pixel data (immediate - no async decode needed)
pub fn load_raw_rgb24(
    &mut self,
    data: &[u8],
    width: u32,
    height: u32,
    stride: u32,
) -> u32 {
    let id = self.next_id.fetch_add(1, Ordering::SeqCst);

    // Store dimensions immediately
    self.pending_dimensions.insert(id, ImageDimensions { width, height });

    // Queue for processing (will convert RGB24 -> RGBA)
    self.states.insert(id, ImageState::Pending);
    let _ = self.decode_tx.send(DecodeRequest {
        id,
        source: ImageSource::RawRgb24 {
            data: data.to_vec(),
            width,
            height,
            stride,
        },
        max_width: 0,
        max_height: 0,
    });

    id
}
```

**Step 3: Handle new sources in decoder thread**

In `decoder_thread_pooled` (around line 175), update the match:

```rust
let result = match request.source {
    ImageSource::File(path) => {
        Self::decode_file(&path, request.max_width, request.max_height)
    }
    ImageSource::Data(data) => {
        Self::decode_data(&data, request.max_width, request.max_height)
    }
    ImageSource::RawArgb32 { data, width, height, stride } => {
        Self::convert_argb32_to_rgba(&data, width, height, stride)
    }
    ImageSource::RawRgb24 { data, width, height, stride } => {
        Self::convert_rgb24_to_rgba(&data, width, height, stride)
    }
};
```

**Step 4: Add conversion functions**

After `process_image` (around line 265), add:

```rust
/// Convert ARGB32 (A,R,G,B byte order) to RGBA
fn convert_argb32_to_rgba(
    data: &[u8],
    width: u32,
    height: u32,
    stride: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        let row_start = (y * stride) as usize;
        for x in 0..width {
            let pixel_start = row_start + (x * 4) as usize;
            if pixel_start + 4 <= data.len() {
                let a = data[pixel_start];
                let r = data[pixel_start + 1];
                let g = data[pixel_start + 2];
                let b = data[pixel_start + 3];
                // RGBA order
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(a);
            }
        }
    }

    Some((width, height, rgba))
}

/// Convert RGB24 (R,G,B byte order) to RGBA
fn convert_rgb24_to_rgba(
    data: &[u8],
    width: u32,
    height: u32,
    stride: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        let row_start = (y * stride) as usize;
        for x in 0..width {
            let pixel_start = row_start + (x * 3) as usize;
            if pixel_start + 3 <= data.len() {
                let r = data[pixel_start];
                let g = data[pixel_start + 1];
                let b = data[pixel_start + 2];
                // RGBA order with full alpha
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(255);
            }
        }
    }

    Some((width, height, rgba))
}
```

**Step 5: Run tests**

```bash
cd rust/neomacs-display && cargo test image_cache
```

**Step 6: Commit**

```bash
git add rust/neomacs-display/src/backend/wgpu/image_cache.rs
git commit -m "feat(image): add raw pixel data loading (ARGB32, RGB24)"
```

---

## Task 2: Expose Raw Pixel Loading in Renderer

**Files:**
- Modify: `rust/neomacs-display/src/backend/wgpu/renderer.rs`

**Step 1: Find the renderer's image loading methods**

Search for `load_image_file` in renderer.rs to find where it delegates to image_cache.

**Step 2: Add load_image_argb32 method**

```rust
/// Load image from raw ARGB32 pixel data
pub fn load_image_argb32(&mut self, data: &[u8], width: u32, height: u32, stride: u32) -> u32 {
    self.image_cache.load_raw_argb32(data, width, height, stride)
}

/// Load image from raw RGB24 pixel data
pub fn load_image_rgb24(&mut self, data: &[u8], width: u32, height: u32, stride: u32) -> u32 {
    self.image_cache.load_raw_rgb24(data, width, height, stride)
}
```

**Step 3: Commit**

```bash
git add rust/neomacs-display/src/backend/wgpu/renderer.rs
git commit -m "feat(renderer): expose raw pixel image loading"
```

---

## Task 3: Implement FFI Functions for Raw Pixel Loading

**Files:**
- Modify: `rust/neomacs-display/src/ffi.rs:1291-1307`

**Step 1: Replace load_image_argb32 stub**

Find the stub at line 1291 and replace with:

```rust
/// Load an image from raw ARGB32 pixel data
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_argb32(
    handle: *mut NeomacsDisplay,
    data: *const u8,
    width: c_int,
    height: c_int,
    stride: c_int,
) -> u32 {
    if handle.is_null() || data.is_null() || width <= 0 || height <= 0 || stride <= 0 {
        return 0;
    }
    let display = &mut *handle;

    let data_len = (stride * height) as usize;
    let data_slice = std::slice::from_raw_parts(data, data_len);

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer_mut() {
            return renderer.load_image_argb32(
                data_slice,
                width as u32,
                height as u32,
                stride as u32,
            );
        }
    }
    0
}
```

**Step 2: Replace load_image_rgb24 stub**

Find the stub at line 1301 and replace with:

```rust
/// Load an image from raw RGB24 pixel data
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_rgb24(
    handle: *mut NeomacsDisplay,
    data: *const u8,
    width: c_int,
    height: c_int,
    stride: c_int,
) -> u32 {
    if handle.is_null() || data.is_null() || width <= 0 || height <= 0 || stride <= 0 {
        return 0;
    }
    let display = &mut *handle;

    let data_len = (stride * height) as usize;
    let data_slice = std::slice::from_raw_parts(data, data_len);

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer_mut() {
            return renderer.load_image_rgb24(
                data_slice,
                width as u32,
                height as u32,
                stride as u32,
            );
        }
    }
    0
}
```

**Step 3: Replace load_image_data stub**

Find the stub at line 1273 and replace with:

```rust
/// Load an image from raw bytes (encoded image format)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_data(
    handle: *mut NeomacsDisplay,
    data: *const u8,
    len: usize,
) -> u32 {
    if handle.is_null() || data.is_null() || len == 0 {
        return 0;
    }
    let display = &mut *handle;

    let data_slice = std::slice::from_raw_parts(data, len);

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer_mut() {
            return renderer.load_image_data(data_slice, 0, 0);
        }
    }
    0
}
```

**Step 4: Build and test**

```bash
cd rust/neomacs-display && cargo build
```

**Step 5: Commit**

```bash
git add rust/neomacs-display/src/ffi.rs
git commit -m "feat(ffi): implement raw pixel image loading functions"
```

---

## Task 4: Update C-side to Use GPU Path First

**Files:**
- Modify: `src/neomacsterm.c` (around line 1406, `neomacs_get_or_load_image` function)

**Step 1: Read current implementation**

The current code tries pixmap data first (which calls stub functions), then falls back to file path.

**Step 2: Reverse priority - try file path first**

Find the `neomacs_get_or_load_image` function and reorder:

```c
static uint32_t
neomacs_get_or_load_image (struct glyph *glyph, struct image *img)
{
  /* Check if we already have this image loaded */
  if (img->pixmap != 0)
    return img->pixmap;

  /* Priority 1: Try GPU file path (async, most efficient) */
  if (img->file && STRINGP (img->file))
    {
      const char *path = SSDATA (img->file);
      int width = img->width;
      int height = img->height;

      uint32_t id;
      if (width > 0 && height > 0)
        id = neomacs_display_load_image_file_scaled (display_handle, path, width, height);
      else
        id = neomacs_display_load_image_file (display_handle, path);

      if (id != 0)
        {
          img->pixmap = id;
          return id;
        }
    }

  /* Priority 2: Try raw pixel data if pixmap_data available */
  /* This is for images that were decoded by Emacs (cairo, etc) */
  /* TODO: Get pixel data from Emacs image backend */

  return 0;
}
```

**Step 3: Build and test**

```bash
make -j$(nproc)
```

**Step 4: Test inline images**

```bash
./test/manual/run-gpu-image-test.sh
```

**Step 5: Commit**

```bash
git add src/neomacsterm.c
git commit -m "feat(image): prioritize GPU file path for inline images"
```

---

## Task 5: Add DMA-BUF Import Path for Large Images (Optional Zero-Copy)

**Files:**
- Modify: `rust/neomacs-display/src/backend/wgpu/image_cache.rs`

This task adds an optional zero-copy path for images that can be loaded directly into GPU memory. For most images, the existing async decode path is sufficient since image decoding is CPU-bound. However, for very large images or when images come from DMA-BUF sources (screencapture, etc.), this provides a zero-copy alternative.

**Step 1: Add DmaBufBuffer import method**

```rust
#[cfg(target_os = "linux")]
use super::external_buffer::DmaBufBuffer;

impl ImageCache {
    /// Import image from DMA-BUF (zero-copy if supported)
    #[cfg(target_os = "linux")]
    pub fn import_dmabuf(
        &mut self,
        dmabuf: DmaBufBuffer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (width, height) = dmabuf.dimensions();

        // Try zero-copy import
        if let Some(texture) = dmabuf.to_wgpu_texture(device, queue) {
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("DMA-BUF Image Bind Group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });

            let memory_size = (width * height * 4) as usize;
            self.total_memory += memory_size;

            self.textures.insert(id, CachedImage {
                texture,
                view,
                bind_group,
                width,
                height,
                memory_size,
            });
            self.states.insert(id, ImageState::Ready);

            log::info!("Imported DMA-BUF image {} ({}x{}) zero-copy", id, width, height);
        } else {
            self.states.insert(id, ImageState::Failed("DMA-BUF import failed".into()));
            log::warn!("DMA-BUF import failed for image {}", id);
        }

        id
    }
}
```

**Step 2: Commit**

```bash
git add rust/neomacs-display/src/backend/wgpu/image_cache.rs
git commit -m "feat(image): add optional DMA-BUF zero-copy import path"
```

---

## Task 6: Unified Memory Budget Management

**Files:**
- Create: `rust/neomacs-display/src/backend/wgpu/media_budget.rs`
- Modify: `rust/neomacs-display/src/backend/wgpu/mod.rs`

**Step 1: Create media_budget.rs**

```rust
//! Unified memory budget management for all media caches.
//!
//! Provides a shared memory budget across images, video frames, and WebKit surfaces.
//! Each cache reports its usage; eviction is coordinated centrally.

use std::collections::BTreeMap;

/// Media type for priority ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MediaType {
    /// Static images (lowest priority - can be reloaded)
    Image,
    /// Video frames (medium priority - can be re-decoded)
    Video,
    /// WebKit surfaces (highest priority - expensive to recreate)
    WebKit,
}

/// Entry in the media budget tracker
#[derive(Debug)]
pub struct BudgetEntry {
    pub media_type: MediaType,
    pub id: u32,
    pub size_bytes: usize,
    pub last_access: u64,
}

/// Unified media budget manager
pub struct MediaBudget {
    /// Maximum total memory in bytes (default: 256MB)
    max_memory: usize,
    /// Current total memory usage
    current_memory: usize,
    /// All tracked entries, ordered by (media_type, last_access)
    entries: BTreeMap<(MediaType, u64, u32), BudgetEntry>,
    /// Global access counter
    access_counter: u64,
}

impl MediaBudget {
    /// Create with default 256MB budget
    pub fn new() -> Self {
        Self::with_limit(256 * 1024 * 1024)
    }

    /// Create with custom memory limit
    pub fn with_limit(max_memory: usize) -> Self {
        Self {
            max_memory,
            current_memory: 0,
            entries: BTreeMap::new(),
            access_counter: 0,
        }
    }

    /// Register a new media item
    pub fn register(&mut self, media_type: MediaType, id: u32, size_bytes: usize) {
        self.access_counter += 1;
        let entry = BudgetEntry {
            media_type,
            id,
            size_bytes,
            last_access: self.access_counter,
        };
        self.entries.insert((media_type, self.access_counter, id), entry);
        self.current_memory += size_bytes;

        log::trace!(
            "MediaBudget: registered {:?}:{} ({}KB), total={}MB/{}MB",
            media_type, id, size_bytes / 1024,
            self.current_memory / (1024 * 1024),
            self.max_memory / (1024 * 1024)
        );
    }

    /// Unregister a media item
    pub fn unregister(&mut self, media_type: MediaType, id: u32) {
        // Find and remove the entry
        let key = self.entries.iter()
            .find(|(_, e)| e.media_type == media_type && e.id == id)
            .map(|(k, _)| *k);

        if let Some(key) = key {
            if let Some(entry) = self.entries.remove(&key) {
                self.current_memory = self.current_memory.saturating_sub(entry.size_bytes);
            }
        }
    }

    /// Touch an entry (update last access time)
    pub fn touch(&mut self, media_type: MediaType, id: u32) {
        // Find current entry
        let old_key = self.entries.iter()
            .find(|(_, e)| e.media_type == media_type && e.id == id)
            .map(|(k, _)| *k);

        if let Some(old_key) = old_key {
            if let Some(mut entry) = self.entries.remove(&old_key) {
                self.access_counter += 1;
                entry.last_access = self.access_counter;
                self.entries.insert((media_type, self.access_counter, id), entry);
            }
        }
    }

    /// Get items to evict to make room for new_size bytes
    /// Returns list of (media_type, id) to evict
    pub fn get_eviction_candidates(&self, new_size: usize) -> Vec<(MediaType, u32)> {
        let mut candidates = Vec::new();
        let target = self.current_memory + new_size;

        if target <= self.max_memory {
            return candidates;
        }

        let mut freed = 0usize;
        let need_to_free = target - self.max_memory;

        // Iterate in order: lowest priority first (Image), then oldest
        for ((media_type, _, id), entry) in &self.entries {
            if freed >= need_to_free {
                break;
            }
            candidates.push((*media_type, *id));
            freed += entry.size_bytes;
        }

        candidates
    }

    /// Check if we're over budget
    pub fn is_over_budget(&self) -> bool {
        self.current_memory > self.max_memory
    }

    /// Get current memory usage
    pub fn current_usage(&self) -> usize {
        self.current_memory
    }

    /// Get max memory limit
    pub fn max_limit(&self) -> usize {
        self.max_memory
    }
}

impl Default for MediaBudget {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_eviction_order() {
        let mut budget = MediaBudget::with_limit(100);

        // Register items in different order
        budget.register(MediaType::WebKit, 1, 30);
        budget.register(MediaType::Image, 2, 20);
        budget.register(MediaType::Video, 3, 25);
        budget.register(MediaType::Image, 4, 15);

        // Need to evict to make room for 50 bytes
        let candidates = budget.get_eviction_candidates(50);

        // Should evict Images first (lowest priority), oldest first
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0], (MediaType::Image, 2)); // oldest image
        assert_eq!(candidates[1], (MediaType::Image, 4)); // second image
    }
}
```

**Step 2: Add to mod.rs**

```rust
pub mod media_budget;
```

**Step 3: Commit**

```bash
git add rust/neomacs-display/src/backend/wgpu/media_budget.rs
git add rust/neomacs-display/src/backend/wgpu/mod.rs
git commit -m "feat(media): add unified memory budget management"
```

---

## Summary

| Task | Description | Priority |
|------|-------------|----------|
| 1 | Raw pixel data loading in ImageCache | High |
| 2 | Expose raw pixel loading in Renderer | High |
| 3 | Implement FFI functions | High |
| 4 | Update C-side to use GPU path first | High |
| 5 | DMA-BUF import for large images | Medium |
| 6 | Unified memory budget | Medium |

After completing these tasks:
- Inline images will work via GPU path
- ARGB32/RGB24 pixel data can be loaded directly
- All media types share the same DMA-BUF import infrastructure
- Memory is managed across all media caches

## Testing

After implementation:

```bash
# Build
make -j$(nproc)

# Test inline images
./test/manual/run-gpu-image-test.sh

# Or manually in neomacs:
# M-x find-file /path/to/test.png
# Or insert image in scratch buffer
```
