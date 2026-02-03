//! wgpu GPU-accelerated scene renderer.

use crate::core::scene::Scene;

/// GPU-accelerated renderer using wgpu.
pub struct WgpuRenderer {
    // Will be populated in later tasks
}

impl WgpuRenderer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn render(&mut self, _scene: &Scene) {
        // Stub - will be implemented
    }
}

impl Default for WgpuRenderer {
    fn default() -> Self {
        Self::new()
    }
}
