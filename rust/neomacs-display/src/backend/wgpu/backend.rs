//! Winit window and event handling backend.

use crate::core::error::DisplayResult;
use crate::core::scene::Scene;
use crate::backend::DisplayBackend;

/// Winit-based window and input backend.
pub struct WinitBackend {
    initialized: bool,
    width: u32,
    height: u32,
}

impl WinitBackend {
    pub fn new() -> Self {
        Self {
            initialized: false,
            width: 800,
            height: 600,
        }
    }
}

impl Default for WinitBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayBackend for WinitBackend {
    fn init(&mut self) -> DisplayResult<()> {
        self.initialized = true;
        Ok(())
    }

    fn shutdown(&mut self) {
        self.initialized = false;
    }

    fn render(&mut self, _scene: &Scene) -> DisplayResult<()> {
        Ok(())
    }

    fn present(&mut self) -> DisplayResult<()> {
        Ok(())
    }

    fn name(&self) -> &'static str {
        "winit-wgpu"
    }

    fn is_initialized(&self) -> bool {
        self.initialized
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    fn set_vsync(&mut self, _enabled: bool) {
        // Will be implemented with wgpu surface
    }
}
