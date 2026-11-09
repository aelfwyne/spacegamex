//! Procedural drawing shared by the viewports: shaded spheres for planets
//! and stars, radial glows, starfield backdrops. Everything is generated from
//! deterministic seeds, so the same world always looks the same.

use crate::rngx::mix64;
use crate::sim::planet::{Planet, PlanetClass};
use crate::sim::Rgb;
use eframe::egui::{epaint::Mesh, pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Vec2};
use std::f32::consts::TAU;

// ---------------------------------------------------------------- noise

fn hash01(seed: u64, x: i32, y: i32, z: i32) -> f32 {
    let h = mix64(
        seed ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ (y as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
            ^ (z as u64).wrapping_mul(0x1656_67b1_9e37_79f9),
    );
    (h >> 40) as f32 / (1u64 << 24) as f32
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// 3-D value noise in [0,1]. Sampling on the unit sphere avoids seams.
pub fn noise3(seed: u64, p: [f32; 3]) -> f32 {
    let (x, y, z) = (p[0], p[1], p[2]);
    let (xi, yi, zi) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let (xf, yf, zf) = (smooth(x - xi as f32), smooth(y - yi as f32), smooth(z - zi as f32));
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let c = |dx, dy, dz| hash01(seed, xi + dx, yi + dy, zi + dz);
    let x00 = l(c(0, 0, 0), c(1, 0, 0), xf);
    let x10 = l(c(0, 1, 0), c(1, 1, 0), xf);
    let x01 = l(c(0, 0, 1), c(1, 0, 1), xf);
    let x11 = l(c(0, 1, 1), c(1, 1, 1), xf);
    l(l(x00, x10, yf), l(x01, x11, yf), zf)
}

/// Fractal noise (4 octaves) in roughly [0,1].
pub fn fbm(seed: u64, p: [f32; 3], freq: f32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut f = freq;
    for o in 0..4 {
        sum += amp * noise3(seed.wrapping_add(o), [p[0] * f, p[1] * f, p[2] * f]);
        f *= 2.03;
        amp *= 0.5;
    }
    sum / 0.9375
}

pub fn seed_of(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x1000_0000_01b3))
}

// ---------------------------------------------------------------- color

type Lin = [f32; 3];

fn lin(c: Rgb) -> Lin {
    [c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0]
}

fn mix(a: Lin, b: Lin, t: f32) -> Lin {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn scale(a: Lin, k: f32) -> Lin {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn to32(a: Lin) -> Color32 {
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
    Color32::from_rgb(f(a[0]), f(a[1]), f(a[2]))
}

pub fn with_alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

// ---------------------------------------------------------------- shapes

/// Soft radial glow: opaque-ish at `r0`, fading to nothing at `r1`.
pub fn glow(painter: &Painter, center: Pos2, r0: f32, r1: f32, color: Color32, alpha: f32) {
    if r1 <= r0 || alpha <= 0.0 {
        return;
    }
    let segs = 48u32;
    let rings = 6u32;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, with_alpha(color, alpha));
    for i in 1..=rings {
        let t = i as f32 / rings as f32;
        let r = r0 + (r1 - r0) * t;
        let a = alpha * (1.0 - t).powf(2.2);
        for j in 0..segs {
            let ang = TAU * j as f32 / segs as f32;
            mesh.colored_vertex(center + vec2(ang.cos(), ang.sin()) * r, with_alpha(color, a));
        }
    }
    // Inner disc up to r0 at full alpha (ring 1 starts there).
    let ring = |i: u32, j: u32| 1 + (i - 1) * segs + (j % segs);
    for j in 0..segs {
        mesh.add_triangle(0, ring(1, j), ring(1, j + 1));
    }
    for i in 1..rings {
        for j in 0..segs {
            let (a, b, c, d) = (ring(i, j), ring(i, j + 1), ring(i + 1, j), ring(i + 1, j + 1));
            mesh.add_triangle(a, b, c);
            mesh.add_triangle(b, d, c);
        }
    }
    // Fix: the "inner disc" vertices of ring 1 sit at r0 with ring-1 alpha;
    // that is the soft start we want.
    painter.add(Shape::mesh(mesh));
}

/// Draw a lit sphere. `shade(p)` returns the surface albedo for the unit
/// surface point `p` (x right, y up, z toward the viewer, after `spin`).
/// `light` is the screen direction toward the light source.
pub fn sphere(
    painter: &Painter,
    center: Pos2,
    r: f32,
    light: Vec2,
    spin: f32,
    ambient: f32,
    shade: &dyn Fn([f32; 3]) -> Lin,
) {
    if r < 0.5 {
        return;
    }
    let rings = ((r / 3.0) as u32).clamp(4, 28);
    let segs = ((r * 1.2) as u32).clamp(12, 72);
    let l = {
        let v = [light.x, -light.y, 0.65];
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        [v[0] / n, v[1] / n, v[2] / n]
    };
    let (ss, cs) = spin.sin_cos();
    let vert = |rho: f32, th: f32| -> (Pos2, Color32) {
        let (x, y) = (rho * th.cos(), rho * th.sin()); // y up
        let z = (1.0 - rho * rho).max(0.0).sqrt();
        let lambert = (x * l[0] + y * l[1] + z * l[2]).max(0.0);
        // Rotate around the vertical axis for the texture lookup.
        let p = [x * cs + z * ss, y, -x * ss + z * cs];
        let base = shade(p);
        let k = ambient + (1.0 - ambient) * lambert;
        (center + vec2(x, -y) * r, to32(scale(base, k)))
    };
    let mut mesh = Mesh::default();
    let (p0, c0) = vert(0.0, 0.0);
    mesh.colored_vertex(p0, c0);
    for i in 1..=rings {
        // Denser rings toward the limb, where shading changes fastest.
        let t = i as f32 / rings as f32;
        let rho = (t * std::f32::consts::FRAC_PI_2).sin();
        for j in 0..segs {
            let (p, c) = vert(rho, TAU * j as f32 / segs as f32);
            mesh.colored_vertex(p, c);
        }
    }
    let ring = |i: u32, j: u32| 1 + (i - 1) * segs + (j % segs);
    for j in 0..segs {
        mesh.add_triangle(0, ring(1, j), ring(1, j + 1));
    }
    for i in 1..rings {
        for j in 0..segs {
            let (a, b, c, d) = (ring(i, j), ring(i, j + 1), ring(i + 1, j), ring(i + 1, j + 1));
            mesh.add_triangle(a, b, c);
            mesh.add_triangle(b, d, c);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Albedo function for a planet, from its class, temperature and atmosphere.
pub fn planet_shader(p: &Planet) -> Box<dyn Fn([f32; 3]) -> Lin> {
    let seed = seed_of(&p.name);
    let h = |k: u64| (mix64(seed ^ k) >> 40) as f32 / (1u64 << 24) as f32;
    match p.class {
        PlanetClass::GasGiant => {
            let warm = p.temp > 90.0 || h(1) < 0.6;
            let (a, b) = if warm {
                (mix([0.86, 0.74, 0.55], [0.95, 0.6, 0.35], h(2)), mix([0.55, 0.38, 0.25], [0.8, 0.7, 0.6], h(3)))
            } else {
                ([0.75, 0.82, 0.88], [0.45, 0.55, 0.75])
            };
            let freq = 6.0 + h(4) * 8.0;
            Box::new(move |q| {
                let turb = fbm(seed, q, 2.5) - 0.5;
                let band = ((q[1] * freq + turb * 2.2).sin() * 0.5 + 0.5).powf(1.4);
                mix(a, b, band)
            })
        }
        PlanetClass::IceGiant => {
            let a = mix([0.55, 0.82, 0.92], [0.45, 0.6, 0.95], h(5));
            let b = scale(a, 0.8);
            Box::new(move |q| {
                let band = ((q[1] * 4.0 + (fbm(seed, q, 1.5) - 0.5) * 1.2).sin() * 0.5 + 0.5) * 0.6;
                mix(a, b, band)
            })
        }
        _ => {
            let atmo = p.atmo.as_ref().map(|a| a.pressure_bar).unwrap_or(0.0);
            let temp = p.temp;
            if temp > 420.0 {
                // Scorched / volcanic.
                Box::new(move |q| {
                    let n = fbm(seed, q, 3.0);
                    let rock = mix([0.25, 0.14, 0.1], [0.55, 0.35, 0.22], n);
                    if n > 0.68 { mix(rock, [1.0, 0.45, 0.1], (n - 0.68) * 4.0) } else { rock }
                })
            } else if atmo > 0.3 && (240.0..340.0).contains(&temp) {
                // Temperate: oceans, land, clouds, caps.
                let sea = h(6) * 0.15 + 0.47;
                Box::new(move |q| {
                    let n = fbm(seed, q, 2.2);
                    let mut c = if n < sea {
                        mix([0.05, 0.15, 0.4], [0.1, 0.3, 0.6], n / sea)
                    } else {
                        mix([0.2, 0.45, 0.2], [0.55, 0.45, 0.3], (n - sea) * 3.0)
                    };
                    if q[1].abs() > 0.82 {
                        c = mix(c, [0.95, 0.97, 1.0], (q[1].abs() - 0.82) * 8.0);
                    }
                    let cloud = fbm(seed ^ 0xc10d, q, 3.0);
                    if cloud > 0.58 { mix(c, [1.0, 1.0, 1.0], (cloud - 0.58) * 3.0) } else { c }
                })
            } else if temp < 240.0 {
                // Frozen.
                Box::new(move |q| {
                    let n = fbm(seed, q, 3.5);
                    mix([0.62, 0.7, 0.78], [0.92, 0.95, 1.0], n)
                })
            } else {
                // Barren rock, maybe a thin haze.
                let tint = mix([0.45, 0.42, 0.4], [0.6, 0.48, 0.36], h(7));
                Box::new(move |q| {
                    let n = fbm(seed, q, 3.2);
                    let crater = noise3(seed ^ 0xc4a7, [q[0] * 9.0, q[1] * 9.0, q[2] * 9.0]);
                    let c = scale(tint, 0.6 + 0.6 * n);
                    if crater > 0.8 { scale(c, 0.7) } else { c }
                })
            }
        }
    }
}

/// Planet disc with atmosphere rim. `light` points toward the star.
pub fn planet(painter: &Painter, center: Pos2, r: f32, p: &Planet, light: Vec2, spin: f32) {
    let shade = planet_shader(p);
    sphere(painter, center, r, light, spin, 0.08, &*shade);
    let haze = match p.class {
        PlanetClass::GasGiant => Some(Color32::from_rgb(230, 200, 150)),
        PlanetClass::IceGiant => Some(Color32::from_rgb(150, 220, 255)),
        _ => p.atmo.as_ref().filter(|a| a.pressure_bar > 0.2).map(|a| {
            if a.co2 > 0.5 { Color32::from_rgb(240, 200, 150) } else { Color32::from_rgb(140, 190, 255) }
        }),
    };
    if let Some(hc) = haze
        && r > 6.0 {
            glow(painter, center, r * 0.98, r * 1.12, hc, 0.22);
        }
}

/// A star: corona glow plus a limb-darkened disc.
pub fn star(painter: &Painter, center: Pos2, r: f32, rgb: Rgb, t: f32) {
    let col = lin(rgb);
    let c32 = to32(col);
    glow(painter, center, r * 0.9, r * 4.5, c32, 0.35);
    glow(painter, center, r * 0.9, r * 1.8, c32, 0.5);
    let seed = 0x5_7a_12;
    let shade = move |q: [f32; 3]| {
        let gran = fbm(seed, [q[0] + t * 0.02, q[1], q[2]], 6.0);
        let limb = q[2].clamp(0.0, 1.0).powf(0.45);
        let core = mix(col, [1.0, 1.0, 0.97], limb * 0.75);
        scale(core, 0.7 + 0.3 * limb + 0.12 * (gran - 0.5))
    };
    // Self-lit: light straight at the viewer, high ambient.
    sphere(painter, center, r, Vec2::ZERO, 0.0, 1.0, &shade);
}

/// Deterministic starfield backdrop with a gentle twinkle.
pub fn starfield(painter: &Painter, rect: Rect, seed: u64, density: f32, t: f32) {
    painter.rect_filled(rect, 0.0, Color32::from_rgb(3, 5, 10));
    let n = ((rect.area() / 1000.0) * density) as u32;
    for i in 0..n.min(2500) {
        let a = mix64(seed ^ (i as u64).wrapping_mul(0x9e37_79b9));
        let x = (a & 0xffff) as f32 / 65535.0;
        let y = ((a >> 16) & 0xffff) as f32 / 65535.0;
        let m = ((a >> 32) & 0xff) as f32 / 255.0;
        let tw = 0.75 + 0.25 * ((t * (0.5 + m * 2.0)) + m * 40.0).sin();
        let b = (0.25 + 0.75 * m * m) * tw;
        let tint = (a >> 40) & 3;
        let c = match tint {
            0 => [0.75, 0.82, 1.0],
            1 => [1.0, 0.92, 0.8],
            _ => [0.95, 0.95, 1.0],
        };
        let p = pos2(rect.left() + x * rect.width(), rect.top() + y * rect.height());
        let rad = if m > 0.93 { 1.4 } else { 0.8 };
        painter.circle_filled(p, rad, to32(scale(c, b)));
    }
}

/// Ship marker: a small chevron pointing along `dir`.
pub fn ship(painter: &Painter, at: Pos2, dir: Vec2, size: f32, color: Color32) {
    let d = if dir.length_sq() > 1e-6 { dir.normalized() } else { vec2(1.0, 0.0) };
    let n = vec2(-d.y, d.x);
    let pts = vec![
        at + d * size,
        at - d * size * 0.7 + n * size * 0.65,
        at - d * size * 0.3,
        at - d * size * 0.7 - n * size * 0.65,
    ];
    painter.add(Shape::convex_polygon(
        vec![pts[0], pts[1], pts[2]],
        color,
        eframe::egui::Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![pts[0], pts[2], pts[3]],
        color,
        eframe::egui::Stroke::NONE,
    ));
    painter.add(Shape::closed_line(pts, eframe::egui::Stroke::new(1.0, Color32::WHITE)));
}
