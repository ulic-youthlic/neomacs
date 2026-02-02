//! Buffer switch animation system - smooth transitions between buffers.

use std::time::{Duration, Instant};

/// Buffer transition animation effect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BufferTransitionEffect {
    /// No animation - instant switch
    None,
    /// Simple crossfade
    #[default]
    Crossfade,
    /// Slide left (new comes from right)
    SlideLeft,
    /// Slide right (new comes from left)
    SlideRight,
    /// Slide up (new comes from bottom)
    SlideUp,
    /// Slide down (new comes from top)
    SlideDown,
    /// Scale and fade
    ScaleFade,
    /// Push (new covers old)
    Push,
    /// Blur transition
    Blur,
    /// 3D page curl (book page turn)
    PageCurl,
}

impl BufferTransitionEffect {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" => Self::None,
            "crossfade" | "fade" => Self::Crossfade,
            "slide-left" | "slide" => Self::SlideLeft,
            "slide-right" => Self::SlideRight,
            "slide-up" => Self::SlideUp,
            "slide-down" => Self::SlideDown,
            "scale" | "scale-fade" => Self::ScaleFade,
            "push" | "stack" => Self::Push,
            "blur" => Self::Blur,
            "page" | "page-curl" | "book" => Self::PageCurl,
            _ => Self::Crossfade,
        }
    }
}

/// Easing function for animations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransitionEasing {
    Linear,
    #[default]
    EaseOut,
    EaseIn,
    EaseInOut,
    /// Overshoot then settle (bouncy)
    EaseOutBack,
}

impl TransitionEasing {
    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t * t,
            Self::EaseOut => 1.0 - (1.0 - t).powi(3),
            Self::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Self::EaseOutBack => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
            }
        }
    }
}

/// Direction for directional animations (slide, push)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransitionDirection {
    #[default]
    Left,
    Right,
    Up,
    Down,
}

/// State of an active buffer transition
#[derive(Debug, Clone)]
pub struct BufferTransition {
    /// The effect type
    pub effect: BufferTransitionEffect,
    
    /// Direction for directional effects
    pub direction: TransitionDirection,
    
    /// Animation progress (0.0 = start, 1.0 = complete)
    pub progress: f32,
    
    /// Total duration
    pub duration: Duration,
    
    /// Start time
    pub start_time: Instant,
    
    /// Easing function
    pub easing: TransitionEasing,
    
    /// Is the animation complete?
    pub completed: bool,
    
    /// Old buffer snapshot width
    pub old_width: f32,
    
    /// Old buffer snapshot height
    pub old_height: f32,
}

impl BufferTransition {
    pub fn new(effect: BufferTransitionEffect, direction: TransitionDirection, duration: Duration) -> Self {
        Self {
            effect,
            direction,
            progress: 0.0,
            duration,
            start_time: Instant::now(),
            easing: TransitionEasing::EaseOut,
            completed: false,
            old_width: 0.0,
            old_height: 0.0,
        }
    }
    
    /// Update progress based on elapsed time
    pub fn update(&mut self) -> bool {
        if self.completed {
            return false;
        }
        
        let elapsed = Instant::now().duration_since(self.start_time);
        let raw_progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        
        if raw_progress >= 1.0 {
            self.progress = 1.0;
            self.completed = true;
            return false;
        }
        
        self.progress = self.easing.apply(raw_progress);
        true
    }

    /// Update progress with explicit delta time
    pub fn update_with_dt(&mut self, dt: f32) -> bool {
        if self.completed {
            return false;
        }
        
        let elapsed = Instant::now().duration_since(self.start_time);
        let raw_progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        
        if raw_progress >= 1.0 {
            self.progress = 1.0;
            self.completed = true;
            return false;
        }
        
        self.progress = self.easing.apply(raw_progress);
        true
    }
    
    /// Get the eased progress value
    pub fn eased_progress(&self) -> f32 {
        self.progress
    }
    
    // === Effect-specific calculations ===
    
    /// Get crossfade opacity for old content
    pub fn crossfade_old_opacity(&self) -> f32 {
        1.0 - self.progress
    }
    
    /// Get crossfade opacity for new content  
    pub fn crossfade_new_opacity(&self) -> f32 {
        self.progress
    }
    
    /// Get slide offset for old content
    pub fn slide_old_offset(&self) -> (f32, f32) {
        let offset = self.progress;
        match self.direction {
            TransitionDirection::Left => (-offset * self.old_width, 0.0),
            TransitionDirection::Right => (offset * self.old_width, 0.0),
            TransitionDirection::Up => (0.0, -offset * self.old_height),
            TransitionDirection::Down => (0.0, offset * self.old_height),
        }
    }
    
    /// Get slide offset for new content
    pub fn slide_new_offset(&self) -> (f32, f32) {
        let offset = 1.0 - self.progress;
        match self.direction {
            TransitionDirection::Left => (offset * self.old_width, 0.0),
            TransitionDirection::Right => (-offset * self.old_width, 0.0),
            TransitionDirection::Up => (0.0, offset * self.old_height),
            TransitionDirection::Down => (0.0, -offset * self.old_height),
        }
    }
    
    /// Get scale for old content (scale-fade effect)
    pub fn scale_old(&self) -> f32 {
        1.0 - self.progress * 0.1 // Scale down to 0.9
    }
    
    /// Get scale for new content (scale-fade effect)
    pub fn scale_new(&self) -> f32 {
        0.9 + self.progress * 0.1 // Scale up from 0.9 to 1.0
    }
    
    /// Get blur radius for old content
    pub fn blur_old_radius(&self) -> f32 {
        self.progress * 15.0 // 0 to 15px blur
    }
    
    /// Get blur radius for new content
    pub fn blur_new_radius(&self) -> f32 {
        (1.0 - self.progress) * 15.0 // 15px to 0 blur
    }
    
    /// Get page curl parameters
    /// Returns (curl_progress, curl_angle, shadow_opacity)
    pub fn page_curl_params(&self) -> (f32, f32, f32) {
        let curl_progress = self.progress;
        // Angle goes from 0 to PI as page turns
        let curl_angle = self.progress * std::f32::consts::PI;
        // Shadow is strongest in the middle of the turn
        let shadow_opacity = (self.progress * std::f32::consts::PI).sin() * 0.5;
        (curl_progress, curl_angle, shadow_opacity)
    }
}

/// Buffer transition animator - manages transition state and snapshot
#[derive(Debug)]
pub struct BufferTransitionAnimator {
    /// Default effect for transitions
    pub default_effect: BufferTransitionEffect,
    
    /// Default duration
    pub default_duration: Duration,
    
    /// Currently active transition (if any)
    pub active_transition: Option<BufferTransition>,
    
    /// Whether we have a snapshot of the old buffer
    pub has_snapshot: bool,
    
    /// Snapshot texture ID (managed externally)
    pub snapshot_id: u32,
    
    /// Auto-detect buffer switches
    pub auto_detect: bool,
    
    /// Last content hash (for auto-detection)
    last_content_hash: u64,
}

impl Default for BufferTransitionAnimator {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferTransitionAnimator {
    pub fn new() -> Self {
        Self {
            default_effect: BufferTransitionEffect::Crossfade,
            default_duration: Duration::from_millis(200),
            active_transition: None,
            has_snapshot: false,
            snapshot_id: 0,
            auto_detect: true,
            last_content_hash: 0,
        }
    }
    
    /// Start a transition with default settings
    pub fn start_transition(&mut self) {
        self.start_transition_with(self.default_effect, TransitionDirection::Left);
    }
    
    /// Start a transition with specific effect and direction
    pub fn start_transition_with(&mut self, effect: BufferTransitionEffect, direction: TransitionDirection) {
        if effect == BufferTransitionEffect::None {
            self.active_transition = None;
            return;
        }
        
        self.active_transition = Some(BufferTransition::new(
            effect,
            direction,
            self.default_duration,
        ));
    }
    
    /// Request snapshot capture (call before buffer switch)
    pub fn request_snapshot(&mut self) {
        self.has_snapshot = false; // Will be set true when snapshot is captured
    }
    
    /// Mark snapshot as captured
    pub fn snapshot_captured(&mut self, width: f32, height: f32) {
        self.has_snapshot = true;
        if let Some(ref mut transition) = self.active_transition {
            transition.old_width = width;
            transition.old_height = height;
        }
    }
    
    /// Update the active transition
    /// Returns true if transition is still active (needs redraw)
    pub fn update(&mut self) -> bool {
        if let Some(ref mut transition) = self.active_transition {
            let still_active = transition.update();
            if !still_active {
                self.active_transition = None;
                self.has_snapshot = false;
            }
            still_active
        } else {
            false
        }
    }

    /// Update with explicit delta time
    pub fn update_with_dt(&mut self, dt: f32) -> bool {
        if let Some(ref mut transition) = self.active_transition {
            let still_active = transition.update_with_dt(dt);
            if !still_active {
                self.active_transition = None;
                self.has_snapshot = false;
            }
            still_active
        } else {
            false
        }
    }
    
    /// Check if a transition is currently active
    pub fn is_active(&self) -> bool {
        self.active_transition.is_some()
    }
    
    /// Get the current transition (if any)
    pub fn get_transition(&self) -> Option<&BufferTransition> {
        self.active_transition.as_ref()
    }
    
    /// Set default effect
    pub fn set_default_effect(&mut self, effect: BufferTransitionEffect) {
        self.default_effect = effect;
    }
    
    /// Set default duration
    pub fn set_default_duration(&mut self, duration: Duration) {
        self.default_duration = duration;
    }
    
    /// Simple hash for content change detection
    pub fn update_content_hash(&mut self, hash: u64) -> bool {
        let changed = hash != self.last_content_hash && self.last_content_hash != 0;
        self.last_content_hash = hash;
        changed
    }
}

/// Page curl shader parameters for GPU rendering
#[derive(Debug, Clone, Copy)]
pub struct PageCurlParams {
    /// Curl progress (0.0 = flat, 1.0 = fully turned)
    pub progress: f32,
    /// Curl cylinder radius
    pub radius: f32,
    /// Corner being lifted (0=bottom-right, 1=top-right, 2=bottom-left, 3=top-left)
    pub corner: u32,
    /// Page width
    pub width: f32,
    /// Page height
    pub height: f32,
    /// Shadow intensity
    pub shadow: f32,
    /// Backside darkening
    pub backside_darken: f32,
}

impl Default for PageCurlParams {
    fn default() -> Self {
        Self {
            progress: 0.0,
            radius: 50.0,
            corner: 0, // bottom-right
            width: 800.0,
            height: 600.0,
            shadow: 0.3,
            backside_darken: 0.2,
        }
    }
}

impl PageCurlParams {
    /// Update params based on animation progress
    pub fn from_progress(progress: f32, width: f32, height: f32) -> Self {
        Self {
            progress,
            radius: 30.0 + progress * 40.0, // Radius increases as page lifts
            corner: 0,
            width,
            height,
            shadow: (progress * std::f32::consts::PI).sin() * 0.4,
            backside_darken: 0.15,
        }
    }
}
