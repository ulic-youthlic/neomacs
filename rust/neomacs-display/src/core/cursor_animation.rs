//! Cursor animation system - Neovide-style smooth cursor with particle effects.

use std::time::{Duration, Instant};
use std::collections::VecDeque;

/// Cursor animation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorAnimationMode {
    /// No animation - instant cursor movement
    None,
    /// Smooth movement only
    #[default]
    Smooth,
    /// Particles shoot backward (Neovide railgun)
    Railgun,
    /// Comet-like trail follows cursor (Neovide torpedo)
    Torpedo,
    /// Sparkly particles scatter around (Neovide pixiedust)
    Pixiedust,
    /// Shockwave ring expands from cursor (Neovide sonicboom)
    Sonicboom,
    /// Concentric rings emanate outward (Neovide ripple)
    Ripple,
    /// Animated outline glow
    Wireframe,
}

impl CursorAnimationMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" => Self::None,
            "smooth" => Self::Smooth,
            "railgun" => Self::Railgun,
            "torpedo" => Self::Torpedo,
            "pixiedust" => Self::Pixiedust,
            "sonicboom" => Self::Sonicboom,
            "ripple" => Self::Ripple,
            "wireframe" => Self::Wireframe,
            _ => Self::Smooth,
        }
    }
}

/// A single particle in the cursor trail
#[derive(Debug, Clone)]
pub struct Particle {
    /// Current X position
    pub x: f32,
    /// Current Y position  
    pub y: f32,
    /// X velocity (pixels per second)
    pub vx: f32,
    /// Y velocity (pixels per second)
    pub vy: f32,
    /// Current size (radius)
    pub size: f32,
    /// Color (RGBA)
    pub color: [f32; 4],
    /// Time when particle was created
    pub birth_time: Instant,
    /// Particle lifetime
    pub lifetime: Duration,
    /// Initial size (for decay calculation)
    pub initial_size: f32,
}

impl Particle {
    /// Check if particle is still alive
    pub fn is_alive(&self, now: Instant) -> bool {
        now.duration_since(self.birth_time) < self.lifetime
    }
    
    /// Get current age as fraction (0.0 = just born, 1.0 = dead)
    pub fn age_fraction(&self, now: Instant) -> f32 {
        let age = now.duration_since(self.birth_time).as_secs_f32();
        let lifetime = self.lifetime.as_secs_f32();
        (age / lifetime).min(1.0)
    }
    
    /// Update particle position based on velocity
    pub fn update(&mut self, dt: f32) {
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        // Apply friction/drag
        self.vx *= 0.95;
        self.vy *= 0.95;
    }
    
    /// Get current opacity (fades out over lifetime)
    pub fn opacity(&self, now: Instant) -> f32 {
        let age = self.age_fraction(now);
        // Smooth fade out
        (1.0 - age).powi(2)
    }
    
    /// Get current size (shrinks over lifetime)
    pub fn current_size(&self, now: Instant) -> f32 {
        let age = self.age_fraction(now);
        self.initial_size * (1.0 - age * 0.7)
    }
}

/// Ring effect (for sonicboom/ripple)
#[derive(Debug, Clone)]
pub struct Ring {
    /// Center X
    pub x: f32,
    /// Center Y
    pub y: f32,
    /// Current radius
    pub radius: f32,
    /// Expansion speed (pixels per second)
    pub speed: f32,
    /// Color
    pub color: [f32; 4],
    /// Birth time
    pub birth_time: Instant,
    /// Lifetime
    pub lifetime: Duration,
    /// Ring thickness
    pub thickness: f32,
}

impl Ring {
    pub fn is_alive(&self, now: Instant) -> bool {
        now.duration_since(self.birth_time) < self.lifetime
    }
    
    pub fn age_fraction(&self, now: Instant) -> f32 {
        let age = now.duration_since(self.birth_time).as_secs_f32();
        (age / self.lifetime.as_secs_f32()).min(1.0)
    }
    
    pub fn update(&mut self, dt: f32) {
        self.radius += self.speed * dt;
    }
    
    pub fn opacity(&self, now: Instant) -> f32 {
        let age = self.age_fraction(now);
        (1.0 - age).powi(2)
    }
}

/// Trail point for torpedo effect
#[derive(Debug, Clone)]
pub struct TrailPoint {
    pub x: f32,
    pub y: f32,
    pub time: Instant,
}

/// Cursor animation state
#[derive(Debug)]
pub struct CursorAnimator {
    /// Animation mode
    pub mode: CursorAnimationMode,
    
    /// Target cursor position (from Emacs)
    pub target_x: f32,
    pub target_y: f32,
    pub target_width: f32,
    pub target_height: f32,
    
    /// Current animated cursor position
    pub current_x: f32,
    pub current_y: f32,
    pub current_width: f32,
    pub current_height: f32,
    
    /// Cursor color
    pub color: [f32; 4],
    
    /// Cursor style (0=box, 1=bar, 2=underline, 3=hollow)
    pub style: u8,
    
    /// Is cursor visible (for blink)
    pub visible: bool,
    
    /// Blink state
    blink_on: bool,
    last_blink_toggle: Instant,
    blink_interval: Duration,
    
    /// Animation speed (higher = faster)
    pub animation_speed: f32,
    
    /// Particle system
    pub particles: Vec<Particle>,
    
    /// Ring effects
    pub rings: Vec<Ring>,
    
    /// Trail points for torpedo
    pub trail: VecDeque<TrailPoint>,
    max_trail_length: usize,
    
    /// Last update time
    last_update: Instant,
    
    /// Last position (for detecting movement)
    last_target_x: f32,
    last_target_y: f32,
    
    /// Particle settings
    particle_count: u32,
    particle_lifetime: Duration,
    particle_speed: f32,
    particle_size: f32,
    
    /// Glow intensity (0.0 - 1.0)
    pub glow_intensity: f32,
    
    /// Whether animation is active (cursor is moving)
    animating: bool,
}

impl Default for CursorAnimator {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorAnimator {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            mode: CursorAnimationMode::Smooth,
            target_x: 0.0,
            target_y: 0.0,
            target_width: 8.0,
            target_height: 16.0,
            current_x: 0.0,
            current_y: 0.0,
            current_width: 8.0,
            current_height: 16.0,
            color: [1.0, 1.0, 1.0, 1.0],
            style: 0,
            visible: true,
            blink_on: true,
            last_blink_toggle: now,
            blink_interval: Duration::from_millis(530),
            animation_speed: 15.0, // Neovide default-ish
            particles: Vec::with_capacity(100),
            rings: Vec::with_capacity(10),
            trail: VecDeque::with_capacity(50),
            max_trail_length: 40,
            last_update: now,
            last_target_x: 0.0,
            last_target_y: 0.0,
            particle_count: 15,
            particle_lifetime: Duration::from_millis(400),
            particle_speed: 200.0,
            particle_size: 4.0,
            glow_intensity: 0.3,
            animating: false,
        }
    }
    
    /// Set cursor target position (called when Emacs updates cursor)
    pub fn set_target(&mut self, x: f32, y: f32, width: f32, height: f32, style: u8, color: [f32; 4]) {
        let moved = (self.target_x - x).abs() > 0.5 || (self.target_y - y).abs() > 0.5;
        
        self.last_target_x = self.target_x;
        self.last_target_y = self.target_y;
        self.target_x = x;
        self.target_y = y;
        self.target_width = width;
        self.target_height = height;
        self.style = style;
        self.color = color;
        
        if moved {
            self.on_cursor_move();
        }
    }
    
    /// Called when cursor moves - spawn effects
    fn on_cursor_move(&mut self) {
        self.animating = true;
        
        // Reset blink when cursor moves
        self.blink_on = true;
        self.last_blink_toggle = Instant::now();
        
        let now = Instant::now();
        let dx = self.target_x - self.last_target_x;
        let dy = self.target_y - self.last_target_y;
        let distance = (dx * dx + dy * dy).sqrt();
        
        if distance < 1.0 {
            return;
        }
        
        // Spawn effects based on mode
        match self.mode {
            CursorAnimationMode::None | CursorAnimationMode::Smooth => {}
            
            CursorAnimationMode::Railgun => {
                self.spawn_railgun_particles(dx, dy, distance);
            }
            
            CursorAnimationMode::Torpedo => {
                self.add_trail_point();
            }
            
            CursorAnimationMode::Pixiedust => {
                self.spawn_pixiedust_particles();
            }
            
            CursorAnimationMode::Sonicboom => {
                self.spawn_sonicboom();
            }
            
            CursorAnimationMode::Ripple => {
                self.spawn_ripple();
            }
            
            CursorAnimationMode::Wireframe => {
                // Wireframe is rendered differently, no particles
            }
        }
    }
    
    fn spawn_railgun_particles(&mut self, dx: f32, dy: f32, distance: f32) {
        let now = Instant::now();
        let norm_dx = -dx / distance; // Opposite direction
        let norm_dy = -dy / distance;
        
        // Spawn particles at current position shooting backward
        for i in 0..self.particle_count {
            let angle_offset = (i as f32 / self.particle_count as f32 - 0.5) * 0.8;
            let cos_a = angle_offset.cos();
            let sin_a = angle_offset.sin();
            
            // Rotate direction by angle offset
            let vx = (norm_dx * cos_a - norm_dy * sin_a) * self.particle_speed;
            let vy = (norm_dx * sin_a + norm_dy * cos_a) * self.particle_speed;
            
            // Add some randomness
            let rand_factor = 0.5 + (i as f32 * 7.13).sin().abs() * 0.5;
            
            self.particles.push(Particle {
                x: self.current_x + self.current_width / 2.0,
                y: self.current_y + self.current_height / 2.0,
                vx: vx * rand_factor,
                vy: vy * rand_factor,
                size: self.particle_size * rand_factor,
                color: self.color,
                birth_time: now,
                lifetime: Duration::from_millis((self.particle_lifetime.as_millis() as f32 * rand_factor) as u64),
                initial_size: self.particle_size * rand_factor,
            });
        }
    }
    
    fn spawn_pixiedust_particles(&mut self) {
        let now = Instant::now();
        
        for i in 0..self.particle_count {
            // Random direction
            let angle = (i as f32 * 2.39996) % (2.0 * std::f32::consts::PI); // Golden angle
            let speed = self.particle_speed * (0.3 + (i as f32 * 3.14).sin().abs() * 0.7);
            
            self.particles.push(Particle {
                x: self.current_x + self.current_width / 2.0,
                y: self.current_y + self.current_height / 2.0,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
                size: self.particle_size * 0.7,
                color: [
                    self.color[0],
                    self.color[1], 
                    self.color[2],
                    self.color[3] * 0.8,
                ],
                birth_time: now,
                lifetime: self.particle_lifetime,
                initial_size: self.particle_size * 0.7,
            });
        }
    }
    
    fn add_trail_point(&mut self) {
        self.trail.push_back(TrailPoint {
            x: self.current_x + self.current_width / 2.0,
            y: self.current_y + self.current_height / 2.0,
            time: Instant::now(),
        });
        
        while self.trail.len() > self.max_trail_length {
            self.trail.pop_front();
        }
    }
    
    fn spawn_sonicboom(&mut self) {
        let now = Instant::now();
        self.rings.push(Ring {
            x: self.target_x + self.target_width / 2.0,
            y: self.target_y + self.target_height / 2.0,
            radius: 5.0,
            speed: 300.0,
            color: self.color,
            birth_time: now,
            lifetime: Duration::from_millis(300),
            thickness: 3.0,
        });
    }
    
    fn spawn_ripple(&mut self) {
        let now = Instant::now();
        // Spawn multiple concentric rings
        for i in 0..3 {
            self.rings.push(Ring {
                x: self.target_x + self.target_width / 2.0,
                y: self.target_y + self.target_height / 2.0,
                radius: 2.0 + i as f32 * 8.0,
                speed: 150.0 - i as f32 * 20.0,
                color: self.color,
                birth_time: now,
                lifetime: Duration::from_millis(400 + i as u64 * 50),
                thickness: 2.0,
            });
        }
    }
    
    /// Update animation state - call each frame
    /// Returns true if animation is still active (needs redraw)
    pub fn update(&mut self) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;
        
        // Update cursor blink
        if now.duration_since(self.last_blink_toggle) >= self.blink_interval {
            self.blink_on = !self.blink_on;
            self.last_blink_toggle = now;
        }
        
        // Smooth cursor movement (exponential interpolation)
        if self.mode != CursorAnimationMode::None {
            let factor = 1.0 - (-self.animation_speed * dt).exp();
            
            self.current_x += (self.target_x - self.current_x) * factor;
            self.current_y += (self.target_y - self.current_y) * factor;
            self.current_width += (self.target_width - self.current_width) * factor;
            self.current_height += (self.target_height - self.current_height) * factor;
            
            // Check if we've reached the target
            let dx = (self.target_x - self.current_x).abs();
            let dy = (self.target_y - self.current_y).abs();
            if dx < 0.5 && dy < 0.5 {
                self.current_x = self.target_x;
                self.current_y = self.target_y;
                self.animating = false;
            }
        } else {
            // No animation - instant movement
            self.current_x = self.target_x;
            self.current_y = self.target_y;
            self.current_width = self.target_width;
            self.current_height = self.target_height;
            self.animating = false;
        }
        
        // Update particles
        for particle in &mut self.particles {
            particle.update(dt);
        }
        self.particles.retain(|p| p.is_alive(now));
        
        // Update rings
        for ring in &mut self.rings {
            ring.update(dt);
        }
        self.rings.retain(|r| r.is_alive(now));
        
        // Update trail (remove old points)
        let trail_lifetime = Duration::from_millis(200);
        self.trail.retain(|p| now.duration_since(p.time) < trail_lifetime);
        
        // Add trail point for torpedo while moving
        if self.mode == CursorAnimationMode::Torpedo && self.animating {
            self.add_trail_point();
        }
        
        // Return true if any animation is active
        self.animating || !self.particles.is_empty() || !self.rings.is_empty() || !self.trail.is_empty()
    }
    
    /// Get cursor visibility (considering blink)
    pub fn is_visible(&self) -> bool {
        self.visible && self.blink_on
    }
    
    /// Check if cursor is currently animating
    pub fn is_animating(&self) -> bool {
        self.animating || !self.particles.is_empty() || !self.rings.is_empty()
    }
    
    /// Set animation mode
    pub fn set_mode(&mut self, mode: CursorAnimationMode) {
        self.mode = mode;
        // Clear effects when changing mode
        self.particles.clear();
        self.rings.clear();
        self.trail.clear();
    }
    
    /// Set animation speed (higher = faster cursor movement)
    pub fn set_animation_speed(&mut self, speed: f32) {
        self.animation_speed = speed.max(1.0).min(100.0);
    }
    
    /// Set particle count for effects
    pub fn set_particle_count(&mut self, count: u32) {
        self.particle_count = count.max(1).min(100);
    }

    /// Update with explicit delta time (for external time management)
    pub fn update_with_dt(&mut self, dt: f32) -> bool {
        let now = Instant::now();
        
        // Update cursor blink
        if now.duration_since(self.last_blink_toggle) >= self.blink_interval {
            self.blink_on = !self.blink_on;
            self.last_blink_toggle = now;
        }
        
        // Smooth cursor movement (exponential interpolation)
        if self.mode != CursorAnimationMode::None {
            let factor = 1.0 - (-self.animation_speed * dt).exp();
            
            self.current_x += (self.target_x - self.current_x) * factor;
            self.current_y += (self.target_y - self.current_y) * factor;
            self.current_width += (self.target_width - self.current_width) * factor;
            self.current_height += (self.target_height - self.current_height) * factor;
            
            // Check if we've reached the target
            let dx = (self.target_x - self.current_x).abs();
            let dy = (self.target_y - self.current_y).abs();
            if dx < 0.5 && dy < 0.5 {
                self.current_x = self.target_x;
                self.current_y = self.target_y;
                self.animating = false;
            }
        } else {
            // No animation - instant movement
            self.current_x = self.target_x;
            self.current_y = self.target_y;
            self.current_width = self.target_width;
            self.current_height = self.target_height;
            self.animating = false;
        }
        
        // Update particles
        for particle in &mut self.particles {
            particle.update(dt);
        }
        self.particles.retain(|p| p.is_alive(now));
        
        // Update rings
        for ring in &mut self.rings {
            ring.update(dt);
        }
        self.rings.retain(|r| r.is_alive(now));
        
        // Update trail (remove old points)
        let trail_lifetime = Duration::from_millis(200);
        self.trail.retain(|p| now.duration_since(p.time) < trail_lifetime);
        
        // Add trail point for torpedo while moving
        if self.mode == CursorAnimationMode::Torpedo && self.animating {
            self.add_trail_point();
        }
        
        // Return true if any animation is active
        self.animating || !self.particles.is_empty() || !self.rings.is_empty() || !self.trail.is_empty()
    }
}
