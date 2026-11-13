//! Programmatic brand logo: an ice crab on a navy rounded-square badge.
//! Rendered with 4x supersampling for smooth edges, then box-downsampled.
//! Used for the window/taskbar icon (raw RGBA) and served to pages as PNG.

/// Final logo size in pixels (256x256).
pub const OUT: usize = 256;

/// Supersampling factor: the logo is drawn at OUT * SS, then averaged down.
const SS: usize = 4;
const S: usize = OUT * SS;

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Transparent,
    Badge,
    Crab,
    White,
}

/// Squared-ish distance from a point to a line segment.
fn seg_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let vx = bx - ax;
    let vy = by - ay;
    let wx = px - ax;
    let wy = py - ay;
    let c1 = vx * wx + vy * wy;
    let c2 = vx * vx + vy * vy;
    let t = if c2 <= 0.0 {
        0.0
    } else {
        (c1 / c2).clamp(0.0, 1.0)
    };
    let dx = px - (ax + t * vx);
    let dy = py - (ay + t * vy);
    (dx * dx + dy * dy).sqrt()
}

fn in_ellipse(px: f32, py: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> bool {
    let dx = (px - cx) / rx;
    let dy = (py - cy) / ry;
    dx * dx + dy * dy <= 1.0
}

/// Draw a shape only where something already exists (never outside the badge),
/// unless `carve` is set — then it erases Crab back to Badge (pincer notches).
fn draw(grid: &mut [Part], part: Part, carve: bool, test: impl Fn(f32, f32) -> bool) {
    let k = SS as f32;
    for y in 0..S {
        for x in 0..S {
            let idx = y * S + x;
            let px = (x as f32 + 0.5) / k;
            let py = (y as f32 + 0.5) / k;
            if !test(px, py) {
                continue;
            }
            if carve {
                if grid[idx] == Part::Crab {
                    grid[idx] = Part::Badge;
                }
            } else if part == Part::Badge || grid[idx] != Part::Transparent {
                // The badge defines the mask, so it always writes; every
                // later shape may only draw where something already exists.
                grid[idx] = part;
            }
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Color of a part at a given y position (gradients give it depth).
fn color(part: Part, y: f32) -> (f32, f32, f32, f32) {
    match part {
        Part::Transparent => (0.0, 0.0, 0.0, 0.0),
        Part::White => (255.0, 255.0, 255.0, 255.0),
        Part::Badge => {
            // Navy vertical gradient.
            let t = (y / 256.0).clamp(0.0, 1.0);
            (
                lerp(14.0, 27.0, t),
                lerp(42.0, 85.0, t),
                lerp(69.0, 128.0, t),
                255.0,
            )
        }
        Part::Crab => {
            // Ice: near-white top to cyan bottom.
            let t = ((y - 55.0) / 175.0).clamp(0.0, 1.0);
            (
                lerp(235.0, 79.0, t),
                lerp(252.0, 184.0, t),
                lerp(255.0, 239.0, t),
                255.0,
            )
        }
    }
}

/// Render the logo and return raw RGBA bytes (OUT x OUT).
pub fn draw_rgba() -> Vec<u8> {
    let mut grid = vec![Part::Transparent; S * S];
    let k = SS as f32;

    // 1. Badge: rounded rectangle.
    draw(&mut grid, Part::Badge, false, |px, py| {
        let cx = px.clamp(24.0, 256.0 - 24.0);
        let cy = py.clamp(24.0, 256.0 - 24.0);
        let dx = px - cx;
        let dy = py - cy;
        dx * dx + dy * dy <= 24.0 * 24.0
    });

    // 2. Legs (3 per side).
    let legs = [
        (84.0, 160.0, 46.0, 186.0),
        (88.0, 175.0, 54.0, 209.0),
        (99.0, 187.0, 75.0, 222.0),
    ];
    for &(x1, y1, x2, y2) in &legs {
        draw(&mut grid, Part::Crab, false, |px, py| {
            seg_dist(px, py, x1, y1, x2, y2) <= 6.5
        });
        // Mirror for the right side.
        draw(&mut grid, Part::Crab, false, |px, py| {
            seg_dist(px, py, 256.0 - x1, y1, 256.0 - x2, y2) <= 6.5
        });
    }

    // 3. Arms connecting body to claws.
    draw(&mut grid, Part::Crab, false, |px, py| {
        seg_dist(px, py, 83.0, 118.0, 54.0, 89.0) <= 8.5
            || seg_dist(px, py, 173.0, 118.0, 202.0, 89.0) <= 8.5
    });

    // 4. Body.
    draw(&mut grid, Part::Crab, false, |px, py| {
        in_ellipse(px, py, 128.0, 144.0, 59.0, 41.0)
    });

    // 5. Claws.
    draw(&mut grid, Part::Crab, false, |px, py| {
        let d1 = (px - 44.0).powi(2) + (py - 79.0).powi(2);
        let d2 = (px - 212.0).powi(2) + (py - 79.0).powi(2);
        d1 <= 24.0 * 24.0 || d2 <= 24.0 * 24.0
    });

    // 6. Pincer notches (carve a bite out of each claw).
    draw(&mut grid, Part::Badge, true, |px, py| {
        let d1 = (px - 27.0).powi(2) + (py - 62.0).powi(2);
        let d2 = (px - 229.0).powi(2) + (py - 62.0).powi(2);
        d1 <= 14.0 * 14.0 || d2 <= 14.0 * 14.0
    });

    // 7. Eye stalks.
    draw(&mut grid, Part::Crab, false, |px, py| {
        seg_dist(px, py, 109.0, 117.0, 109.0, 93.0) <= 4.0
            || seg_dist(px, py, 147.0, 117.0, 147.0, 93.0) <= 4.0
    });

    // 8. Eyes.
    draw(&mut grid, Part::White, false, |px, py| {
        let d1 = (px - 109.0).powi(2) + (py - 93.0).powi(2);
        let d2 = (px - 147.0).powi(2) + (py - 93.0).powi(2);
        d1 <= 7.5 * 7.5 || d2 <= 7.5 * 7.5
    });

    // 9. Downsample with premultiplied alpha (avoids dark fringes).
    let mut rgba = vec![0u8; OUT * OUT * 4];
    let n = (SS * SS) as f32;
    for oy in 0..OUT {
        for ox in 0..OUT {
            let (mut pr, mut pg, mut pb, mut pa) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let py = ((oy * SS + sy) as f32 + 0.5) / k;
                    let part = grid[(oy * SS + sy) * S + (ox * SS + sx)];
                    let (r, g, b, a) = color(part, py);
                    let af = a / 255.0;
                    pr += r * af;
                    pg += g * af;
                    pb += b * af;
                    pa += af;
                }
            }
            let i = (oy * OUT + ox) * 4;
            if pa > 0.0 {
                rgba[i] = (pr / pa).round().clamp(0.0, 255.0) as u8;
                rgba[i + 1] = (pg / pa).round().clamp(0.0, 255.0) as u8;
                rgba[i + 2] = (pb / pa).round().clamp(0.0, 255.0) as u8;
                rgba[i + 3] = (pa / n * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    rgba
}

/// Encode raw RGBA bytes as a PNG (for serving to web pages).
pub fn png_bytes(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, OUT as u32, OUT as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().expect("png header");
        writer.write_image_data(rgba).expect("png data");
    }
    out
}
