//! Procedural drawing shared by the viewports: shaded planets, moons and
//! stars, belts, rings, glows and starfield backdrops. Every look is driven
//! by the generated traits (geosphere, surface layout, temperature, life,
//! atmosphere, gas-giant feature, terrain) and seeded by the body's name, so
//! the same world always looks the same.

use crate::rngx::mix64;
use crate::sim::planet::{AtmoKind, BeltFeature, GgFeature, LifeType, Planet, PlanetClass, TempBand};
use crate::sim::star::{Star, StarKind};
use crate::sim::Rgb;
use eframe::egui::{epaint::Mesh, pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};
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

fn hseed(seed: u64, k: u64) -> f32 {
    (mix64(seed ^ k) >> 40) as f32 / (1u64 << 24) as f32
}

// ---------------------------------------------------------------- color

pub type Lin = [f32; 3];

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

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let n = dot(v, v).sqrt().max(1e-6);
    [v[0] / n, v[1] / n, v[2] / n]
}

/// A random direction on the sphere from a seed.
fn spot_dir(seed: u64, k: u64, max_lat: f32) -> [f32; 3] {
    let lon = hseed(seed, k) * TAU;
    let lat = (hseed(seed, k + 1) * 2.0 - 1.0) * max_lat;
    [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()]
}

// ---------------------------------------------------------------- shapes

/// Soft radial glow fading from `r0` (alpha) to nothing at `r1`.
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

/// Surface shader: (albedo, self-emission) for a unit surface point.
pub type Shade = Box<dyn Fn([f32; 3]) -> (Lin, f32)>;

/// Draw a lit sphere. `shade(p)` gets the unit surface point (x right,
/// y up, z toward the viewer, after `spin`). `light` is the screen
/// direction toward the light source.
pub fn sphere(painter: &Painter, center: Pos2, r: f32, light: Vec2, spin: f32, ambient: f32, shade: &dyn Fn([f32; 3]) -> (Lin, f32)) {
    if r < 0.5 {
        return;
    }
    let rings = ((r / 3.0) as u32).clamp(4, 28);
    let segs = ((r * 1.2) as u32).clamp(12, 72);
    let l = light_vec(light);
    let (ss, cs) = spin.sin_cos();
    let vert = |rho: f32, th: f32| -> (Pos2, Color32) {
        let (x, y) = (rho * th.cos(), rho * th.sin()); // y up
        let z = (1.0 - rho * rho).max(0.0).sqrt();
        let lambert = (x * l[0] + y * l[1] + z * l[2]).max(0.0);
        let p = [x * cs + z * ss, y, -x * ss + z * cs];
        let (base, emit) = shade(p);
        let k = ambient + (1.0 - ambient) * lambert;
        let c = [base[0] * (k + emit), base[1] * (k + emit), base[2] * (k + emit)];
        (center + vec2(x, -y) * r, to32(c))
    };
    let mut mesh = Mesh::default();
    let (p0, c0) = vert(0.0, 0.0);
    mesh.colored_vertex(p0, c0);
    for i in 1..=rings {
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

/// Screen light direction → unit vector in sphere space.
fn light_vec(light: Vec2) -> [f32; 3] {
    norm([light.x, -light.y, 0.65])
}

// ---------------------------------------------------------------- worlds

fn atmo_cloud(a: Option<AtmoKind>) -> (f32, Lin) {
    match a {
        None | Some(AtmoKind::Thin) => (0.0, [1.0, 1.0, 1.0]),
        Some(AtmoKind::Breathable) => (0.3, [1.0, 1.0, 1.0]),
        Some(AtmoKind::Toxic) => (0.4, [0.92, 0.9, 0.62]),
        Some(AtmoKind::Dense) => (0.72, [0.96, 0.92, 0.8]),
        Some(AtmoKind::Corrosive) => (0.85, [0.85, 0.6, 0.35]),
        Some(AtmoKind::Infiltrating) => (0.82, [0.62, 0.78, 0.38]),
    }
}

/// The handful of traits the surface colours depend on.
#[derive(Clone, Copy)]
struct Look {
    geosphere: Option<crate::sim::planet::Geosphere>,
    temp_band: TempBand,
    life: Option<LifeType>,
}

fn land_color(p: &Look, n: f32) -> Lin {
    let rock = mix([0.42, 0.36, 0.3], [0.62, 0.5, 0.36], n);
    let desert = mix([0.72, 0.58, 0.38], [0.86, 0.72, 0.5], n);
    let base = match p.geosphere {
        Some(crate::sim::planet::Geosphere::Desert | crate::sim::planet::Geosphere::Arid) => desert,
        _ => rock,
    };
    let base = match p.temp_band {
        TempBand::Cold => mix(base, [0.62, 0.62, 0.6], 0.5),
        TempBand::Hot => mix(base, [0.7, 0.45, 0.28], 0.4),
        _ => base,
    };
    match p.life {
        Some(LifeType::Forests) => mix([0.12, 0.38, 0.14], [0.35, 0.5, 0.2], n),
        Some(LifeType::Giant) => mix([0.2, 0.42, 0.12], [0.45, 0.48, 0.22], n),
        Some(LifeType::Hostile) => mix([0.22, 0.12, 0.3], [0.18, 0.32, 0.14], n),
        Some(LifeType::ColonyAltered) => {
            if (n * 23.0).fract() < 0.18 { [0.78, 0.7, 0.42] } else { mix([0.16, 0.42, 0.16], [0.4, 0.5, 0.2], n) }
        }
        Some(LifeType::Simple) => mix(base, [0.38, 0.45, 0.28], 0.45),
        Some(LifeType::Microbial) => mix(base, [0.5, 0.42, 0.3], 0.2),
        Some(LifeType::Extinct) => mix(base, [0.45, 0.43, 0.4], 0.6),
        _ => base,
    }
}

/// Surface shader for a planet or moon. `sun` is the substellar direction in
/// sphere space (used for tidally locked worlds, which never turn).
pub fn planet_shader(p: &Planet, sun: [f32; 3]) -> Shade {
    let seed = seed_of(&p.name);
    match p.class {
        PlanetClass::GasGiant | PlanetClass::IceGiant => gas_shader(p, seed),
        PlanetClass::IceWorld => ice_shader(p, seed),
        PlanetClass::AsteroidBelt => Box::new(|_| ([0.45, 0.42, 0.4], 0.0)),
        _ => rock_shader(p, seed, sun),
    }
}

fn gas_shader(p: &Planet, seed: u64) -> Shade {
    let small = p.class == PlanetClass::IceGiant;
    let hot = matches!(p.temp_band, TempBand::Burning | TempBand::Hot);
    let (a, b): (Lin, Lin) = if small {
        (mix([0.5, 0.8, 0.9], [0.4, 0.55, 0.95], hseed(seed, 5)), [0.35, 0.55, 0.8])
    } else if hot {
        ([0.55, 0.32, 0.2], [0.85, 0.55, 0.3])
    } else if hseed(seed, 1) < 0.7 {
        (mix([0.86, 0.74, 0.55], [0.95, 0.6, 0.35], hseed(seed, 2)), mix([0.55, 0.38, 0.25], [0.8, 0.7, 0.6], hseed(seed, 3)))
    } else {
        ([0.75, 0.82, 0.88], [0.45, 0.55, 0.75])
    };
    let feature = p.gg_feature;
    let freq = match feature {
        Some(GgFeature::HighWinds) => 16.0 + hseed(seed, 4) * 8.0,
        _ if small => 4.0,
        _ => 6.0 + hseed(seed, 4) * 6.0,
    };
    let turb_amp = match feature {
        Some(GgFeature::HighWinds) => 0.6,
        Some(GgFeature::Storms) => 2.6,
        _ => 1.8,
    };
    let spots: Vec<([f32; 3], f32, Lin)> = match feature {
        Some(GgFeature::Storms) => (0..5u64)
            .map(|k| (spot_dir(seed, 10 + k * 3, 0.9), 0.97 + hseed(seed, 40 + k) * 0.02, if k % 2 == 0 { [0.95, 0.92, 0.85] } else { [0.4, 0.25, 0.2] }))
            .collect(),
        Some(GgFeature::SingleSuperStorm) => vec![(spot_dir(seed, 60, 0.5), 0.86, [0.8, 0.35, 0.22])],
        _ => vec![],
    };
    let sky = p.life.filter(|l| l.aerial());
    Box::new(move |q| {
        let turb = fbm(seed, q, 2.5) - 0.5;
        let band = ((q[1] * freq + turb * turb_amp).sin() * 0.5 + 0.5).powf(1.4);
        let mut c = mix(a, b, band);
        for (dir, thr, col) in &spots {
            // Oval storms: stretched along longitude.
            let d = norm([q[0], q[1] * 1.8, q[2]]);
            let k = dot(d, norm([dir[0], dir[1] * 1.8, dir[2]]));
            if k > *thr {
                let t = ((k - thr) / (1.0 - thr)).clamp(0.0, 1.0);
                c = mix(c, *col, 0.4 + 0.6 * t);
            }
        }
        if let Some(l) = sky {
            let m = fbm(seed ^ 0x51c7, q, 4.0);
            let tint = match l {
                LifeType::SkyLeviathans => [0.55, 0.35, 0.6],
                LifeType::SkyEcosystem => [0.35, 0.6, 0.45],
                _ => [0.6, 0.7, 0.5],
            };
            if m > 0.6 {
                c = mix(c, tint, (m - 0.6) * 2.0);
            }
        }
        (c, 0.0)
    })
}

fn ice_shader(p: &Planet, seed: u64) -> Shade {
    let has = |s: &str| p.terrain.iter().any(|t| t.contains(s));
    let fissures = has("blue-green") || has("fissures");
    let organic = has("organic");
    let dusty = has("dust-covered") || has("Black");
    let methane = has("methane lakes");
    let cryo = has("cryovolcano") || has("Geysers");
    let smooth_ocean = has("hidden ocean");
    Box::new(move |q| {
        let n = fbm(seed, q, if smooth_ocean { 1.5 } else { 3.5 });
        let mut c = mix([0.66, 0.74, 0.82], [0.93, 0.96, 1.0], n);
        if dusty {
            c = mix(c, [0.25, 0.25, 0.28], (fbm(seed ^ 3, q, 2.0) - 0.35).max(0.0) * 1.8);
        }
        if organic {
            c = mix(c, [0.45, 0.3, 0.2], (fbm(seed ^ 5, q, 2.5) - 0.45).max(0.0) * 2.0);
        }
        if methane && fbm(seed ^ 7, q, 3.0) > 0.66 {
            c = [0.12, 0.14, 0.2];
        }
        if fissures {
            let f = (fbm(seed ^ 9, q, 3.0) - 0.5).abs();
            if f < 0.025 {
                c = [0.25, 0.75, 0.75];
            }
        }
        let mut emit = 0.0;
        if cryo && noise3(seed ^ 11, [q[0] * 6.0, q[1] * 6.0, q[2] * 6.0]) > 0.86 {
            c = [1.0, 1.0, 1.0];
            emit = 0.25;
        }
        // Craters.
        if !smooth_ocean && noise3(seed ^ 13, [q[0] * 9.0, q[1] * 9.0, q[2] * 9.0]) > 0.82 {
            c = scale(c, 0.8);
        }
        (c, emit)
    })
}

fn rock_shader(p: &Planet, seed: u64, sun: [f32; 3]) -> Shade {
    let sea = p.geosphere.map(|g| g.sea_fraction()).unwrap_or(0.0);
    let s = p.surface.to_lowercase();
    let freq = if s.contains("archipelago") || s.contains("islands") || s.contains("island chains") {
        6.0
    } else if s.contains("supercontinent") || s.contains("one island-continent") || s.contains("two great") {
        1.2
    } else {
        2.2
    };
    // fbm sits around 0.5 ± 0.13: map the sea fraction to a threshold.
    let threshold = 0.3 + 0.42 * sea;
    let (cloud_amt, cloud_col) = {
        let (a, c) = atmo_cloud(p.atmosphere);
        let overcast = p.terrain.iter().any(|t| t.contains("overcast") || t.contains("super storm") || t.contains("rainfall"));
        ((a + if overcast { 0.25 } else { 0.0 }).min(0.92), c)
    };
    let cap = match p.temp_band {
        TempBand::Frozen => -1.0,
        TempBand::Cold => 0.5,
        TempBand::Temperate => 0.82,
        _ => 2.0,
    };
    let locked = p.tidally_locked;
    let band = p.temp_band;
    let sea_col: Lin = match (band, p.atmosphere) {
        (_, Some(AtmoKind::Toxic | AtmoKind::Corrosive)) => [0.2, 0.3, 0.18],
        (TempBand::Hot, _) => [0.08, 0.32, 0.42],
        (TempBand::Cold, _) => [0.08, 0.16, 0.32],
        _ => [0.05, 0.16, 0.42],
    };
    let ocean_life = p.life == Some(LifeType::Ocean);
    let look = Look { geosphere: p.geosphere, temp_band: p.temp_band, life: p.life };
    Box::new(move |q| {
        let n = fbm(seed, q, freq);
        let detail = fbm(seed ^ 0xd17, q, 6.0);
        let mut emit = 0.0;
        let mut c = if band == TempBand::Frozen {
            mix([0.7, 0.76, 0.84], [0.95, 0.97, 1.0], detail)
        } else if n < threshold {
            let shallow = ((threshold - n) * 8.0).min(1.0);
            let base = mix([0.12, 0.42, 0.55], sea_col, shallow);
            if ocean_life && detail > 0.62 { mix(base, [0.1, 0.5, 0.35], 0.5) } else { base }
        } else {
            land_color(&look, detail)
        };
        if band == TempBand::Burning {
            c = mix([0.18, 0.12, 0.1], [0.32, 0.22, 0.16], detail);
            let crack = (fbm(seed ^ 0x1a7a, q, 4.0) - 0.5).abs();
            if crack < 0.03 {
                c = [1.0, 0.45, 0.1];
                emit = 1.2;
            }
        }
        if q[1].abs() > cap {
            c = mix(c, [0.95, 0.97, 1.0], ((q[1].abs() - cap) * 10.0).min(1.0));
        }
        if locked {
            // Substellar face scorched, far side iced over.
            let day = dot(q, sun);
            if day > 0.55 {
                c = mix(c, [0.5, 0.36, 0.25], (day - 0.55) * 2.0);
            } else if day < -0.25 {
                c = mix(c, [0.88, 0.92, 0.98], ((-day - 0.25) * 2.5).min(1.0));
            }
        }
        if cloud_amt > 0.0 {
            let cl = fbm(seed ^ 0xc10d, q, 3.0);
            let thr = 1.0 - cloud_amt * 0.8;
            if cl > thr - 0.4 + 0.4 * (1.0 - cloud_amt) {
                c = mix(c, cloud_col, ((cl - thr + 0.25) * 3.0).clamp(0.0, 1.0) * cloud_amt.max(0.3));
            }
        }
        (c, emit)
    })
}

/// A planet or moon with atmosphere rim, rings and aurora. `light` points
/// toward the star.
pub fn planet(painter: &Painter, center: Pos2, r: f32, p: &Planet, light: Vec2, spin: f32) {
    if p.class == PlanetClass::AsteroidBelt {
        rock_cluster(painter, center, r, seed_of(&p.name));
        return;
    }
    if p.ring {
        ring_half(painter, center, r, p, true);
    }
    let sun = light_vec(light);
    let shade = planet_shader(p, sun);
    let spin = if p.tidally_locked { 0.0 } else { spin };
    sphere(painter, center, r, light, spin, 0.08, &*shade);
    let haze = match p.class {
        PlanetClass::GasGiant => Some(Color32::from_rgb(230, 200, 150)),
        PlanetClass::IceGiant => Some(Color32::from_rgb(150, 220, 255)),
        _ => match p.atmosphere {
            Some(AtmoKind::Breathable) => Some(Color32::from_rgb(140, 190, 255)),
            Some(AtmoKind::Toxic) => Some(Color32::from_rgb(210, 220, 120)),
            Some(AtmoKind::Dense) => Some(Color32::from_rgb(240, 225, 190)),
            Some(AtmoKind::Corrosive) => Some(Color32::from_rgb(230, 150, 80)),
            Some(AtmoKind::Infiltrating) => Some(Color32::from_rgb(160, 220, 90)),
            _ => None,
        },
    };
    if let Some(hc) = haze
        && r > 5.0 {
            let thick = matches!(p.atmosphere, Some(AtmoKind::Dense | AtmoKind::Corrosive | AtmoKind::Infiltrating));
            glow(painter, center, r * 0.98, r * if thick { 1.2 } else { 1.12 }, hc, if thick { 0.32 } else { 0.22 });
        }
    if p.gg_feature == Some(GgFeature::IntenseRadiation) && r > 6.0 {
        let aur = Color32::from_rgb(120, 255, 190);
        glow(painter, center + vec2(0.0, -r * 0.93), r * 0.05, r * 0.45, aur, 0.45);
        glow(painter, center + vec2(0.0, r * 0.93), r * 0.05, r * 0.4, aur, 0.35);
    }
    if p.ring {
        ring_half(painter, center, r, p, false);
    }
}

/// Planetary ring, drawn in two halves around the disc.
fn ring_half(painter: &Painter, center: Pos2, r: f32, p: &Planet, back: bool) {
    let seed = seed_of(&p.name);
    let tilt = 0.28 + hseed(seed, 70) * 0.12;
    let (inner, outer) = if p.class == PlanetClass::GasGiant { (1.35, 2.25) } else { (1.4, 1.9) };
    let bands = 7;
    for b in 0..bands {
        let t = b as f32 / (bands - 1) as f32;
        let rr = r * (inner + (outer - inner) * t);
        let dens = 0.35 + 0.5 * hseed(seed, 80 + b as u64);
        let col = with_alpha(Color32::from_rgb(215, 200, 170), dens * 0.6);
        let n = 64;
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = if back { std::f32::consts::PI * i as f32 / n as f32 } else { std::f32::consts::PI * (1.0 + i as f32 / n as f32) };
                center + vec2(a.cos() * rr, -a.sin() * rr * tilt)
            })
            .collect();
        painter.add(Shape::line(pts, Stroke::new((r * 0.07).clamp(0.8, 4.0), col)));
    }
}

/// A belt portrait: a tumbling field of rocks.
fn rock_cluster(painter: &Painter, center: Pos2, r: f32, seed: u64) {
    for i in 0..40u64 {
        let a = hseed(seed, i * 3) * TAU;
        let d = hseed(seed, i * 3 + 1).sqrt() * r * 1.3;
        let sz = (1.0 + hseed(seed, i * 3 + 2) * 4.0) * r / 40.0;
        let at = center + vec2(a.cos(), a.sin()) * d;
        let shade: Box<dyn Fn([f32; 3]) -> (Lin, f32)> = Box::new(move |q| {
            let n = fbm(seed ^ i, q, 4.0);
            (mix([0.3, 0.28, 0.26], [0.6, 0.55, 0.5], n), 0.0)
        });
        sphere(painter, at, sz.max(1.0), vec2(-0.8, -0.4), 0.0, 0.1, &*shade);
    }
}

/// An asteroid belt as a particle ring in the system view.
pub fn belt(painter: &Painter, center: Pos2, radius: f32, p: &Planet, t: f32) {
    let seed = seed_of(&p.name);
    let (width, count, alpha, size) = match p.belt_feature {
        Some(BeltFeature::Wide) => (radius * 0.18, 520, 0.55, 1.2),
        Some(BeltFeature::Dust) => (radius * 0.07, 700, 0.25, 0.7),
        Some(BeltFeature::Bright) => (radius * 0.06, 360, 0.95, 1.4),
        Some(BeltFeature::HighInclination) => (radius * 0.14, 360, 0.6, 1.2),
        _ => (radius * 0.06, 320, 0.6, 1.2),
    };
    for i in 0..count as u64 {
        let a = hseed(seed, i * 2) * TAU + t * 0.002;
        let off = (hseed(seed, i * 2 + 1) - 0.5) * 2.0 * width;
        let incl = if p.belt_feature == Some(BeltFeature::HighInclination) { (a * 2.0).sin() * width * 0.6 } else { 0.0 };
        let rr = (radius + off + incl).max(1.0);
        let pos = center + vec2(a.cos() * rr, a.sin() * rr);
        let glint = p.belt_feature == Some(BeltFeature::MineralRich) && i % 9 == 0;
        let col = if glint { Color32::from_rgb(240, 210, 130) } else { Color32::from_rgb(170, 160, 150) };
        painter.circle_filled(pos, size, with_alpha(col, alpha));
    }
    if p.belt_feature == Some(BeltFeature::DwarfPlanets) {
        for k in 0..4u64 {
            let a = hseed(seed, 900 + k) * TAU;
            painter.circle_filled(center + vec2(a.cos(), a.sin()) * radius, 2.6, Color32::from_rgb(200, 190, 175));
        }
    }
}

// ---------------------------------------------------------------- stars

/// A star drawn by kind: corona glow plus a limb-darkened (or banded) disc.
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
        (scale(core, 0.7 + 0.3 * limb + 0.12 * (gran - 0.5)), 0.0)
    };
    sphere(painter, center, r, Vec2::ZERO, 0.0, 1.0, &shade);
}

/// A star rendered for its kind (white dwarf, brown dwarf, giant…).
pub fn star_of(painter: &Painter, center: Pos2, r: f32, s: &Star, t: f32) {
    match s.kind {
        StarKind::WhiteDwarf => {
            // A tiny, fierce point with an outsized halo.
            glow(painter, center, r * 0.3, r * 6.0, Color32::from_rgb(200, 220, 255), 0.45);
            star(painter, center, r * 0.45, s.class.color(), t);
        }
        StarKind::BrownDwarf => {
            let col = lin(s.class.color());
            glow(painter, center, r * 0.9, r * 2.0, to32(col), 0.25);
            let seed = seed_of(&s.name);
            let shade = move |q: [f32; 3]| {
                let turb = fbm(seed, q, 2.5) - 0.5;
                let band = ((q[1] * 7.0 + turb * 1.5).sin() * 0.5 + 0.5).powf(1.3);
                (mix(scale(col, 0.45), col, band), 0.35)
            };
            sphere(painter, center, r, Vec2::ZERO, t * 0.05, 0.5, &shade);
        }
        _ => star(painter, center, r, s.class.color(), t),
    }
}

/// On-screen radius for a star of this kind (system view scale).
pub fn star_px(s: &Star, base: f32) -> f32 {
    match s.kind {
        StarKind::WhiteDwarf => base * 0.45,
        StarKind::BrownDwarf => base * 0.55,
        StarKind::Giant => (base * (1.0 + (s.radius as f32).ln() * 0.35)).min(base * 2.6),
        StarKind::Subgiant => base * 1.3,
        StarKind::MainSequence => base * (0.6 + 0.4 * (s.radius as f32).clamp(0.1, 3.0).sqrt()),
    }
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
    painter.add(Shape::convex_polygon(vec![pts[0], pts[1], pts[2]], color, Stroke::NONE));
    painter.add(Shape::convex_polygon(vec![pts[0], pts[2], pts[3]], color, Stroke::NONE));
    painter.add(Shape::closed_line(pts, Stroke::new(1.0, Color32::WHITE)));
}
