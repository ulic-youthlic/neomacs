//! Neo-term: GPU-accelerated terminal emulator for Neomacs.
//!
//! Uses `alacritty_terminal` for VT parsing and terminal state,
//! renders cells directly via the wgpu pipeline.

pub mod colors;
pub mod content;
pub mod view;

pub use content::TerminalContent;
pub use view::{TerminalManager, TerminalView};

/// Unique identifier for a terminal instance.
pub type TerminalId = u32;

/// Terminal display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalMode {
    /// Terminal fills an entire Emacs window/buffer.
    Window,
    /// Terminal is inline within buffer text (like an inline image).
    Inline,
    /// Terminal floats on top of all content (renderer-level compositing).
    Floating,
}
