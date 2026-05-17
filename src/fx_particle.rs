//! Particle pool driven by burst events at the cursor.
//!
//! Every burst takes an [`FxParams`] from the user config and pulls
//! values directly — particle count, spawn velocity, spread cone,
//! gravity, lifetime, base radius, twinkle. No bundled "style"
//! presets; each parameter is a slider in the settings UI.
//!
//! Shape and palette overrides apply per-particle and are independent
//! of the motion params.
//!
//! No `rand` dependency — a wrapping LCG seed pseudo-randomises angle
//! / speed / radius jitter so consecutive bursts look varied without
//! pulling in an extra crate.
//!
//! hanabifx では唯一の sabitori view から `view()` 経由で参照される。 単一の
//! 全画面 click-through 透明オーバーレイ上に乗る。

use crate::config::{FxPalette, FxParams, FxShape};
use sabitori::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Disc,
    Square,
    Ring,
    Glyph(char),
}

pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub radius: f32,
    pub color: Color,
    pub age: f32,
    pub lifetime: f32,
    pub gravity: f32,
    pub shape: ShapeKind,
    pub twinkle: bool,
}

impl Particle {
    pub fn alpha(&self) -> f32 {
        let base = (1.0 - self.age / self.lifetime).clamp(0.0, 1.0);
        if self.twinkle {
            let t = (self.age * 25.0).sin() * 0.5 + 0.5;
            base * (0.6 + 0.4 * t)
        } else {
            base
        }
    }
    pub fn alive(&self) -> bool {
        self.age < self.lifetime
    }
    pub fn tick(&mut self, dt: f32) {
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        self.vy += self.gravity * dt;
        self.age += dt;
    }
}

#[derive(Default)]
pub struct FxState {
    pub particles: Vec<Particle>,
    seed: u32,
}

impl FxState {
    /// Spawn one burst at `(x, y)` using the parameters in `params`.
    /// `params` carries its own shape + palette overrides — each event
    /// class (mouse vs key) decides independently.
    ///
    /// `typed_label` carries a printable label for the key that
    /// triggered the burst (key events only). For a printable
    /// single-letter key it's just `"a"` etc.; for control / arrow
    /// keys it's a spelled-out name like `"Backspace"` / `"Enter"` /
    /// `"Up"`. When `params.shape == FxShape::Typed`, every particle
    /// in the burst becomes a glyph cycled from that label so a
    /// multi-char key spells itself out across the burst (count = 12
    /// + label = "Enter" → E n t e r E n t e r E n). The label is
    /// only used here and never stored / logged — it lives on the
    /// caller's stack frame and dies with the burst.
    pub fn burst(&mut self, x: f32, y: f32, params: &FxParams, typed_label: Option<&str>) {
        let shape = params.shape;
        // Pre-collect chars so resolve_shape can index in O(1) per
        // particle (otherwise nth() is O(label_len) and the burst
        // loop becomes O(label_len * count)).
        let label_chars: Vec<char> = typed_label
            .map(|s| s.chars().collect())
            .unwrap_or_default();
        let palette = params.palette;
        // Direction = straight up. The spread cone widens around this
        // axis, so spread = 0 → all particles fly straight up, spread
        // = 360 → distributed across the full circle.
        let direction = std::f32::consts::FRAC_PI_2;
        let spread_rad = params.spread.to_radians();
        let count = params.count.max(1) as usize;

        for i in 0..count {
            let t = if count > 1 {
                i as f32 / (count - 1) as f32
            } else {
                0.5
            };
            let jitter = self.rand_unit() - 0.5;
            let angle_offset = (t - 0.5) * spread_rad + jitter * 0.25;
            let angle = direction + angle_offset;
            let speed = params.speed * (0.75 + self.rand_unit() * 0.5);
            let radius = params.radius * (0.6 + self.rand_unit() * 0.4);
            let color = pick_color(palette, i, t, self.next_rand());
            let kind = resolve_shape(
                shape,
                default_shape_for_palette(palette),
                self.next_rand(),
                &label_chars,
                i,
            );
            self.particles.push(Particle {
                x,
                y,
                // Sabitori's Y axis goes down, so an angle pointing
                // "up" in math has a negative Y component.
                vx: angle.cos() * speed,
                vy: -angle.sin() * speed,
                radius,
                color,
                age: 0.0,
                lifetime: params.lifetime,
                gravity: params.gravity,
                shape: kind,
                twinkle: params.twinkle,
            });
        }
        self.enforce_cap();
    }

    pub fn tick(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.tick(dt);
        }
        self.particles.retain(|p| p.alive());
    }

    /// Hard ceiling on total live particles. Held-key auto-repeat at
    /// `count = 12` × ~30 keyDown/sec = 360 particles/sec; with the
    /// 0.8s default lifetime a normal burst session sits under 300.
    /// 512 leaves headroom for users cranking count + lifetime, but
    /// caps runaway accumulation that could starve the text renderer.
    /// Drops the oldest particles first so the new burst is always
    /// fully visible.
    fn enforce_cap(&mut self) {
        const PARTICLE_CAP: usize = 512;
        if self.particles.len() > PARTICLE_CAP {
            let drop_n = self.particles.len() - PARTICLE_CAP;
            self.particles.drain(0..drop_n);
        }
    }

    fn next_rand(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        self.seed
    }
    fn rand_unit(&mut self) -> f32 {
        ((self.next_rand() >> 8) & 0xFFFF) as f32 / 65535.0
    }
}

fn resolve_shape(
    user: FxShape,
    fallback: ShapeKind,
    _seed: u32,
    label_chars: &[char],
    i: usize,
) -> ShapeKind {
    match user {
        FxShape::Default => fallback,
        FxShape::Disc => ShapeKind::Disc,
        FxShape::Square => ShapeKind::Square,
        FxShape::Ring => ShapeKind::Ring,
        FxShape::Triangle => ShapeKind::Glyph('▲'),
        FxShape::Star => ShapeKind::Glyph('★'),
        FxShape::Heart => ShapeKind::Glyph('♥'),
        // Typed shape cycles the label's chars across the burst's
        // particles. Single-char label ("a") → every particle is 'a';
        // multi-char ("Enter") → particles spell "Enter" repeatedly
        // until count is exhausted. Mouse triggers and modifier-only
        // key presses pass an empty label — fall back to disc so the
        // burst still renders something.
        FxShape::Typed => {
            if label_chars.is_empty() {
                ShapeKind::Disc
            } else {
                ShapeKind::Glyph(label_chars[i % label_chars.len()])
            }
        }
    }
}

fn default_shape_for_palette(_p: FxPalette) -> ShapeKind {
    ShapeKind::Disc
}

fn pick_color(palette: FxPalette, i: usize, t: f32, seed: u32) -> Color {
    match palette {
        FxPalette::Default => {
            let idx = (i + (seed >> 24) as usize) % 5;
            match idx {
                0 => Color::new(1.00, 0.45, 0.55, 1.0),
                1 => Color::new(0.40, 0.90, 0.55, 1.0),
                2 => Color::new(0.45, 0.65, 1.00, 1.0),
                3 => Color::new(1.00, 0.75, 0.30, 1.0),
                _ => Color::new(0.85, 0.60, 1.00, 1.0),
            }
        }
        FxPalette::Mono => Color::new(1.0, 1.0, 1.0, 1.0),
        FxPalette::Pastel => match i % 5 {
            0 => Color::new(0.98, 0.78, 0.86, 1.0),
            1 => Color::new(0.82, 0.94, 0.88, 1.0),
            2 => Color::new(0.85, 0.88, 0.97, 1.0),
            3 => Color::new(0.98, 0.92, 0.78, 1.0),
            _ => Color::new(0.90, 0.84, 0.96, 1.0),
        },
        FxPalette::Neon => match i % 5 {
            0 => Color::new(1.0, 0.10, 0.55, 1.0),
            1 => Color::new(0.10, 1.0, 0.60, 1.0),
            2 => Color::new(0.20, 0.60, 1.0, 1.0),
            3 => Color::new(1.0, 0.85, 0.05, 1.0),
            _ => Color::new(0.75, 0.20, 1.0, 1.0),
        },
        FxPalette::Rainbow => hsv_to_rgb(t, 0.85, 1.0, 1.0),
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32, a: f32) -> Color {
    let h6 = (h.fract() * 6.0 + 6.0) % 6.0;
    let c = v * s;
    let x = c * (1.0 - (h6 % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h6 as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Color::new(r + m, g + m, b + m, a)
}
