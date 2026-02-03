# Winit Rendering Pipeline Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Connect Emacs redisplay to render content in winit/wgpu windows so text appears.

**Architecture:** Add `with_device()` constructor to WgpuRenderer for shared device usage. Track current render window in NeomacsDisplay to route drawing calls. Update C code to call window-targeted begin/end_frame functions.

**Tech Stack:** Rust, wgpu, winit, C FFI

---

## Task 1: Add with_device() Constructor to WgpuRenderer

**Files:**
- Modify: `rust/neomacs-display/src/backend/wgpu/renderer.rs`

**Step 1: Add the with_device constructor**

Add after the existing `new()` method (~line 36):

```rust
/// Create a renderer with an existing device and queue.
/// This allows sharing GPU resources across multiple surfaces.
pub fn with_device(
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    width: u32,
    height: u32,
) -> Self {
    Self::create_renderer_internal(device, queue, None, width, height)
}

fn create_renderer_internal(
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    width: u32,
    height: u32,
) -> Self {
    // Create shader module
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Rect Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rect.wgsl").into()),
    });

    // Create uniform buffer
    let uniforms = Uniforms {
        screen_size: [width as f32, height as f32],
        _padding: [0.0; 2],
    };

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Uniform Buffer"),
        contents: bytemuck::cast_slice(&[uniforms]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    // Create bind group layout
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Uniform Bind Group Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    // Create bind group
    let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Uniform Bind Group"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    // Create pipeline layout
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Rect Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    // Determine surface format
    let format = wgpu::TextureFormat::Bgra8UnormSrgb;

    // Create render pipeline
    let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Rect Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[RectVertex::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    // Configure surface if provided
    let surface_config = surface.as_ref().map(|s| {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        s.configure(&device, &config);
        config
    });

    Self {
        device,
        queue,
        surface,
        surface_config,
        rect_pipeline,
        uniform_buffer,
        uniform_bind_group,
        width,
        height,
    }
}
```

**Step 2: Update new_async to use the helper**

Replace the body of `new_async` (~line 38-230) to call the helper after creating device:

```rust
async fn new_async(
    surface: Option<wgpu::Surface<'static>>,
    width: u32,
    height: u32,
) -> Self {
    // Create wgpu instance
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    // Request adapter
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: surface.as_ref(),
            force_fallback_adapter: false,
        })
        .await
        .expect("Failed to find a suitable GPU adapter");

    // Request device and queue
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Neomacs Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
            },
            None,
        )
        .await
        .expect("Failed to create device");

    let device = Arc::new(device);
    let queue = Arc::new(queue);

    Self::create_renderer_internal(device, queue, surface, width, height)
}
```

**Step 3: Build to verify compilation**

Run: `cd rust/neomacs-display && cargo build --features winit-backend`
Expected: Compiles with no errors (warnings OK)

**Step 4: Run tests**

Run: `cargo test --features winit-backend --lib`
Expected: All 30 tests pass

**Step 5: Commit**

```bash
git add rust/neomacs-display/src/backend/wgpu/renderer.rs
git commit -m "feat: add with_device() constructor to WgpuRenderer"
```

---

## Task 2: Create Shared Renderer in init_wgpu_headless

**Files:**
- Modify: `rust/neomacs-display/src/backend/wgpu/backend.rs`

**Step 1: Update init_wgpu_headless to create renderer**

Find `init_wgpu_headless` (~line 143) and add renderer creation after device/queue:

```rust
pub fn init_wgpu_headless(&mut self) -> DisplayResult<()> {
    if self.wgpu_initialized {
        return Ok(());
    }

    // Create wgpu instance
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    // Request adapter without a surface (headless)
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .ok_or_else(|| DisplayError::InitFailed("Failed to find a suitable GPU adapter".to_string()))?;

    // Request device and queue
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("Neomacs Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: Default::default(),
        },
        None,
    ))
    .map_err(|e| DisplayError::InitFailed(format!("Failed to create device: {}", e)))?;

    let device = Arc::new(device);
    let queue = Arc::new(queue);

    // Get preferred surface format
    let format = wgpu::TextureFormat::Bgra8UnormSrgb;

    // Create the shared renderer
    let renderer = WgpuRenderer::with_device(
        device.clone(),
        queue.clone(),
        self.width,
        self.height,
    );

    self.instance = Some(instance);
    self.device = Some(device);
    self.queue = Some(queue);
    self.renderer = Some(renderer);
    self.surface_format = format;
    self.wgpu_initialized = true;
    self.initialized = true;

    log::info!("wgpu initialized in headless mode with shared renderer");
    Ok(())
}
```

**Step 2: Add WgpuRenderer import if needed**

At top of backend.rs, ensure import exists:
```rust
use super::WgpuRenderer;
```

**Step 3: Build to verify**

Run: `cargo build --features winit-backend`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add rust/neomacs-display/src/backend/wgpu/backend.rs
git commit -m "feat: create shared renderer in init_wgpu_headless"
```

---

## Task 3: Add current_render_window_id to NeomacsDisplay

**Files:**
- Modify: `rust/neomacs-display/src/ffi.rs`

**Step 1: Add field to NeomacsDisplay struct**

Find `pub struct NeomacsDisplay` (~line 37) and add the field:

```rust
pub struct NeomacsDisplay {
    backend_type: BackendType,
    tty_backend: Option<TtyBackend>,
    #[cfg(feature = "winit-backend")]
    winit_backend: Option<WinitBackend>,
    #[cfg(feature = "winit-backend")]
    event_loop: Option<winit::event_loop::EventLoop<crate::backend::wgpu::UserEvent>>,
    scene: Scene,
    frame_glyphs: FrameGlyphBuffer,
    use_hybrid: bool,
    animations: AnimationManager,
    current_row_y: i32,
    current_row_x: i32,
    current_row_height: i32,
    current_row_ascent: i32,
    current_row_is_overlay: bool,
    current_window_id: i32,
    in_frame: bool,
    frame_counter: u64,
    current_render_window_id: u32,  // NEW: 0 = legacy, >0 = winit window
}
```

**Step 2: Initialize field in neomacs_display_init**

Find the `Box::new(NeomacsDisplay { ... })` (~line 93) and add:

```rust
    let mut display = Box::new(NeomacsDisplay {
        backend_type: backend,
        tty_backend: None,
        #[cfg(feature = "winit-backend")]
        winit_backend: None,
        #[cfg(feature = "winit-backend")]
        event_loop: None,
        scene: Scene::new(800.0, 600.0),
        frame_glyphs: FrameGlyphBuffer::with_size(800.0, 600.0),
        use_hybrid,
        animations: AnimationManager::new(),
        current_row_y: -1,
        current_row_x: 0,
        current_row_height: 0,
        current_row_ascent: 0,
        current_row_is_overlay: false,
        current_window_id: -1,
        in_frame: false,
        frame_counter: 0,
        current_render_window_id: 0,  // NEW
    });
```

**Step 3: Build to verify**

Run: `cargo build --features winit-backend`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add rust/neomacs-display/src/ffi.rs
git commit -m "feat: add current_render_window_id tracking to NeomacsDisplay"
```

---

## Task 4: Update begin_frame_window and end_frame_window

**Files:**
- Modify: `rust/neomacs-display/src/ffi.rs`

**Step 1: Update begin_frame_window to set current_render_window_id**

Find `neomacs_display_begin_frame_window` (~line 2492) and update:

```rust
#[no_mangle]
pub extern "C" fn neomacs_display_begin_frame_window(
    handle: *mut NeomacsDisplay,
    window_id: u32,
) {
    let display = unsafe { &mut *handle };

    // Set current render target
    display.current_render_window_id = window_id;

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        backend.begin_frame_for_window(window_id);
    }
}
```

**Step 2: Update end_frame_window to reset current_render_window_id**

Find `neomacs_display_end_frame_window` (~line 2508) and update:

```rust
#[no_mangle]
pub extern "C" fn neomacs_display_end_frame_window(
    handle: *mut NeomacsDisplay,
    window_id: u32,
) {
    let display = unsafe { &mut *handle };

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        backend.end_frame_for_window(window_id);
    }

    // Reset current render target
    display.current_render_window_id = 0;
}
```

**Step 3: Build to verify**

Run: `cargo build --features winit-backend`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add rust/neomacs-display/src/ffi.rs
git commit -m "feat: track current render window in begin/end_frame_window"
```

---

## Task 5: Add get_target_scene Helper Function

**Files:**
- Modify: `rust/neomacs-display/src/ffi.rs`

**Step 1: Add helper function**

Add near top of ffi.rs, after the NeomacsDisplay impl block (~line 65):

```rust
/// Get the target scene for drawing operations.
/// Returns the window's scene if rendering to a winit window,
/// otherwise returns the legacy display scene.
#[cfg(feature = "winit-backend")]
fn get_target_scene(display: &mut NeomacsDisplay) -> &mut Scene {
    if display.current_render_window_id > 0 {
        if let Some(ref mut backend) = display.winit_backend {
            if let Some(scene) = backend.get_scene_mut(display.current_render_window_id) {
                return scene;
            }
        }
    }
    &mut display.scene
}

#[cfg(not(feature = "winit-backend"))]
fn get_target_scene(display: &mut NeomacsDisplay) -> &mut Scene {
    &mut display.scene
}
```

**Step 2: Build to verify**

Run: `cargo build --features winit-backend`
Expected: Compiles successfully (warning about unused function is OK for now)

**Step 3: Commit**

```bash
git add rust/neomacs-display/src/ffi.rs
git commit -m "feat: add get_target_scene helper for window-aware drawing"
```

---

## Task 6: Route Drawing Functions to Target Scene

**Files:**
- Modify: `rust/neomacs-display/src/ffi.rs`

**Step 1: Update neomacs_display_add_window**

Find `neomacs_display_add_window` and update to use get_target_scene:

```rust
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_add_window(...) {
    // ... existing validation code ...

    let display = &mut *handle;
    let scene = get_target_scene(display);

    // Use scene instead of display.scene
    scene.add_window(...);
}
```

**Step 2: Update neomacs_display_begin_row**

Find and update similarly - replace `display.scene` with `get_target_scene(display)`.

**Step 3: Update neomacs_display_add_glyph_to_row**

Find and update similarly.

**Step 4: Update neomacs_display_end_row**

Find and update similarly.

**Step 5: Build to verify**

Run: `cargo build --features winit-backend`
Expected: Compiles successfully

**Step 6: Commit**

```bash
git add rust/neomacs-display/src/ffi.rs
git commit -m "feat: route drawing functions through get_target_scene"
```

---

## Task 7: Update C Code to Call Window-Targeted Functions

**Files:**
- Modify: `src/neomacsterm.c`

**Step 1: Update neomacs_update_begin**

Find `neomacs_update_begin` (~line 515) and update:

```c
static void
neomacs_update_begin (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);

  if (dpyinfo && dpyinfo->display_handle)
    {
      if (windows_or_buffers_changed)
        neomacs_display_clear_all_borders (dpyinfo->display_handle);

      /* Use window-targeted begin_frame if we have a winit window */
      struct neomacs_output *output = FRAME_OUTPUT_DATA (f);
      if (output && output->window_id > 0)
        neomacs_display_begin_frame_window (dpyinfo->display_handle, output->window_id);
      else
        neomacs_display_begin_frame (dpyinfo->display_handle);
    }
}
```

**Step 2: Update neomacs_update_end**

Find `neomacs_update_end` (~line 532) and update:

```c
static void
neomacs_update_end (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);
  int result = 0;

  if (dpyinfo && dpyinfo->display_handle)
    {
      /* Use window-targeted end_frame if we have a winit window */
      struct neomacs_output *output = FRAME_OUTPUT_DATA (f);
      if (output && output->window_id > 0)
        neomacs_display_end_frame_window (dpyinfo->display_handle, output->window_id);
      else
        result = neomacs_display_end_frame (dpyinfo->display_handle);
    }

  /* If Rust cleared glyphs due to layout change (result=1), mark windows inaccurate
     so Emacs will resend all content on the next frame.  */
  if (result == 1)
    {
      Lisp_Object tail, frame;
      FOR_EACH_FRAME (tail, frame)
        {
          struct frame *fr = XFRAME (frame);
          if (FRAME_NEOMACS_P (fr))
            {
              struct window *w;
              FOR_EACH_WINDOW (fr, w)
                {
                  w->must_be_updated_p = true;
                }
            }
        }
    }
}
```

**Step 3: Build Emacs**

Run: `make -j8`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/neomacsterm.c
git commit -m "feat: call window-targeted begin/end_frame for winit windows"
```

---

## Task 8: Test the Rendering Pipeline

**Step 1: Build everything**

```bash
cd rust/neomacs-display && cargo build --release --features winit-backend
cd ../.. && make -j8
```

**Step 2: Run Emacs and check window**

```bash
RUST_LOG=neomacs_display=info ./src/emacs -Q &
sleep 3
import -window root /tmp/render-test.png
kill %1
```

**Step 3: Verify screenshot shows Emacs content**

Check `/tmp/render-test.png` - should show scratch buffer and modeline.

**Step 4: If content not visible, add debug logging**

Add to `end_frame_for_window` in backend.rs:
```rust
log::info!("end_frame_for_window: window_id={}, scene has {} windows",
           window_id, state.scene.windows.len());
```

**Step 5: Final commit if all works**

```bash
git add -A
git commit -m "feat: complete winit rendering pipeline connection"
```

---

## Summary

| Task | Description |
|------|-------------|
| 1 | Add `with_device()` to WgpuRenderer |
| 2 | Create shared renderer in `init_wgpu_headless` |
| 3 | Add `current_render_window_id` field |
| 4 | Update begin/end_frame_window to track window |
| 5 | Add `get_target_scene` helper |
| 6 | Route drawing functions to target scene |
| 7 | Update C code for window-targeted calls |
| 8 | Test the complete pipeline |
