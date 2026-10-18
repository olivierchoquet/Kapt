//! kapt tray icons, drawn in code (no image file to install).
//!
//! - recording: red rounded square with a white "K" (the kapt logo)
//! - paused: orange rounded square with white pause bars
//!
//! Shapes are described in a unit square (0..1) as signed distance functions,
//! then rasterized with 4×4 supersampling for smooth edges.

use ksni::Icon;

/// Icon sizes provided to the panel, which picks the best match.
const SIZES: &[i32] = &[16, 22, 24, 32, 48, 64];

/// Samples per pixel along each axis (antialiasing).
const SUPERSAMPLING: usize = 4;

const RED: [u8; 3] = [0xe0, 0x1b, 0x24];
const ORANGE: [u8; 3] = [0xf6, 0x74, 0x00];

/// A thick line segment with rounded ends, from `a` to `b`.
struct Stroke {
    a: (f32, f32),
    b: (f32, f32),
    radius: f32,
}

/// "K" letter: a vertical stem and two diagonals.
const LETTER_K: &[Stroke] = &[
    Stroke {
        a: (0.34, 0.25),
        b: (0.34, 0.75),
        radius: 0.075,
    },
    Stroke {
        a: (0.40, 0.53),
        b: (0.68, 0.25),
        radius: 0.075,
    },
    Stroke {
        a: (0.48, 0.47),
        b: (0.70, 0.75),
        radius: 0.075,
    },
];

/// Pause symbol: two vertical bars.
const PAUSE_BARS: &[Stroke] = &[
    Stroke {
        a: (0.38, 0.29),
        b: (0.38, 0.71),
        radius: 0.085,
    },
    Stroke {
        a: (0.62, 0.29),
        b: (0.62, 0.71),
        radius: 0.085,
    },
];

/// Icon shown while recording.
pub fn recording() -> Vec<Icon> {
    render_all(RED, LETTER_K)
}

/// Icon shown while paused.
pub fn paused() -> Vec<Icon> {
    render_all(ORANGE, PAUSE_BARS)
}

fn render_all(background: [u8; 3], glyph: &[Stroke]) -> Vec<Icon> {
    SIZES
        .iter()
        .map(|&size| render(size, background, glyph))
        .collect()
}

/// Rasterizes one icon: colored rounded square, white glyph on top.
fn render(size: i32, background: [u8; 3], glyph: &[Stroke]) -> Icon {
    let n = size as usize;
    let mut data = Vec::with_capacity(n * n * 4);
    let samples = (SUPERSAMPLING * SUPERSAMPLING) as f32;

    for py in 0..n {
        for px in 0..n {
            // Coverage of the square and of the glyph within this pixel
            let (mut square, mut white) = (0.0, 0.0);
            for sy in 0..SUPERSAMPLING {
                for sx in 0..SUPERSAMPLING {
                    let x = (px as f32 + (sx as f32 + 0.5) / SUPERSAMPLING as f32) / n as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SUPERSAMPLING as f32) / n as f32;
                    if rounded_square(x, y) <= 0.0 {
                        square += 1.0;
                        if glyph.iter().any(|s| s.distance(x, y) <= 0.0) {
                            white += 1.0;
                        }
                    }
                }
            }

            let alpha = square / samples;
            // Fraction of the visible area covered by the glyph
            let mix = if square > 0.0 { white / square } else { 0.0 };
            let channel = |c: u8| (c as f32 + (255.0 - c as f32) * mix).round() as u8;
            // ARGB32, network byte order
            data.extend_from_slice(&[
                (alpha * 255.0).round() as u8,
                channel(background[0]),
                channel(background[1]),
                channel(background[2]),
            ]);
        }
    }

    Icon {
        width: size,
        height: size,
        data,
    }
}

/// Signed distance to the background: a square with rounded corners,
/// filling the icon with a small margin.
fn rounded_square(x: f32, y: f32) -> f32 {
    const HALF: f32 = 0.46; // half side, leaves a 0.04 margin
    const CORNER: f32 = 0.2;
    let dx = (x - 0.5).abs() - (HALF - CORNER);
    let dy = (y - 0.5).abs() - (HALF - CORNER);
    let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
    outside + dx.max(dy).min(0.0) - CORNER
}

impl Stroke {
    /// Signed distance from (x, y) to the stroke outline.
    fn distance(&self, x: f32, y: f32) -> f32 {
        let (ax, ay) = self.a;
        let (bx, by) = self.b;
        let (abx, aby) = (bx - ax, by - ay);
        let (apx, apy) = (x - ax, y - ay);
        // Projection of the point on the segment, clamped to its ends
        let t = ((apx * abx + apy * aby) / (abx * abx + aby * aby)).clamp(0.0, 1.0);
        let (dx, dy) = (apx - abx * t, apy - aby * t);
        (dx * dx + dy * dy).sqrt() - self.radius
    }
}
