//! A beluga seen at a three-quarter angle. The whale is a real 3D shape, a chain of spheres
//! along a spine, swimming a level ring under a tilted camera. Each pixel keeps its depth,
//! surface normal and position on the body, so light and markings follow the form, and
//! everything is then reduced to a small pixel-art palette.

use std::f32::consts::{PI, TAU};

use crate::activity::{Activity, Language, Mood};

pub const W: usize = 256;
pub const H: usize = 204;
const FRAMES: usize = 108;
const FLOW_BANDS: usize = 12;
const RING: f32 = 70.0;
const LENGTH: f32 = 90.0;
/// Camera height above the horizon: lower shows more flank, higher shows more back.
const ELEVATION: f32 = 0.56;
const FOCAL: f32 = 430.0;
const SCREEN_CENTER: (f32, f32) = (128.0, 106.0);
const ZOOM: f32 = 1.2;
const WATER_Z: f32 = -27.0;
/// Where color starts on the body; the head stays white for the face.
const COLOR_START: f32 = 0.25;
/// How much of the skin behind the head the rings cover; the rest stays white between them.
const RING_COVERAGE: f32 = 0.55;
const MOUTH_RADIUS: f32 = 0.72;
const BLUSH_RADIUS: f32 = 1.7;
const EYE_U: f32 = 0.15;
const EYE_AROUND: f32 = 68.0;
/// Tips each eye a little forward, so both can be seen from the front.
const EYE_AHEAD: f32 = 1.0;

const INK: Rgb = Rgb(0x25, 0x2c, 0x45);
const LINE: Rgb = Rgb(0x3d, 0x4a, 0x6b);

type V3 = [f32; 3];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// One frame's pixels, row by row; `None` is transparent.
pub type Frame = Vec<Option<Rgb>>;

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: V3) -> V3 {
    mul(a, 1.0 / dot(a, a).sqrt().max(1e-5))
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2654435761) ^ 0x9e3779b9;
    x ^= x >> 15;
    x = x.wrapping_mul(2246822519);
    x ^= x >> 13;
    (x % 10000) as f32 / 10000.0
}
fn profile(knots: &[(f32, f32)], u: f32) -> f32 {
    for pair in knots.windows(2) {
        if u <= pair[1].0 {
            let k = ((u - pair[0].0) / (pair[1].0 - pair[0].0)).clamp(0.0, 1.0);
            return lerp(pair[0].1, pair[1].1, k * k * (3.0 - 2.0 * k));
        }
    }
    knots[knots.len() - 1].1
}

fn hue_of(c: Rgb) -> (f32, f32) {
    let (r, g, b) = (c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0);
    }
    let l = (max + min) / 2.0;
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s)
}
fn hsl(h: f32, s: f32, l: f32) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match (h.rem_euclid(360.0) / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let byte = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb(byte(r), byte(g), byte(b))
}

const BAYER: [[f32; 4]; 4] = [
    [0.0, 8.0, 2.0, 10.0],
    [12.0, 4.0, 14.0, 6.0],
    [3.0, 11.0, 1.0, 9.0],
    [15.0, 7.0, 13.0, 5.0],
];

#[derive(Clone, Copy)]
struct Ramp([Rgb; 4]);

impl Ramp {
    fn at(&self, light: f32, x: i32, y: i32) -> Rgb {
        let dither = BAYER[y.rem_euclid(4) as usize][x.rem_euclid(4) as usize] / 16.0 - 0.47;
        let band = (light.clamp(0.0, 1.0) * 3.0 + dither * 0.55)
            .round()
            .clamp(0.0, 3.0);
        self.0[band as usize]
    }
    /// A language's hue at a set brightness, so even a murky color reads on the whale.
    /// `vivid` is the lowest saturation it may have, unless it was grey to begin with.
    fn language(color: Rgb, vivid: f32) -> Ramp {
        let (h, s) = hue_of(color);
        let s = if s < 0.05 { s } else { s.max(vivid) };
        Ramp([
            hsl(h, s, 0.26),
            hsl(h, s, 0.4),
            hsl(h, s, 0.55),
            hsl(h, s * 0.95, 0.72),
        ])
    }
}

const ADULT_SKIN: Ramp = Ramp([
    Rgb(0x6d, 0x85, 0xa9),
    Rgb(0xa8, 0xbd, 0xd5),
    Rgb(0xdc, 0xe7, 0xf2),
    Rgb(0xff, 0xff, 0xff),
]);
/// Beluga calves are born grey and whiten as they grow.
const CALF_SKIN: Ramp = Ramp([
    Rgb(0x52, 0x5e, 0x78),
    Rgb(0x76, 0x84, 0x9e),
    Rgb(0x9a, 0xa7, 0xbd),
    Rgb(0xb9, 0xc4, 0xd6),
]);

// ------------------------------------------------------------------ camera

struct Camera {
    right: V3,
    down: V3,
    toward: V3,
}

impl Camera {
    fn new() -> Camera {
        let (sin, cos) = ELEVATION.sin_cos();
        // World: x right, y toward the viewer along the ground, z up.
        Camera {
            right: [1.0, 0.0, 0.0],
            down: [0.0, sin, -cos],
            toward: [0.0, cos, sin],
        }
    }
    /// Screen position, depth (larger is nearer), and scale from perspective.
    fn project(&self, p: V3) -> (f32, f32, f32, f32) {
        let depth = dot(p, self.toward);
        let scale = ZOOM * FOCAL / (FOCAL - depth);
        (
            SCREEN_CENTER.0 + dot(p, self.right) * scale,
            SCREEN_CENTER.1 + dot(p, self.down) * scale,
            depth,
            scale,
        )
    }
}

// ------------------------------------------------------------------ surface buffer

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Body,
    Flipper(usize),
    Fluke(usize),
}

#[derive(Clone, Copy)]
struct Hit {
    depth: f32,
    normal: V3,
    /// Where this pixel sits on the whale in 3D.
    pos: V3,
    part: Part,
    /// How far along the part, 0 at its start and 1 at its end.
    u: f32,
    /// Local frame of this sample, so markings can be read from the normal.
    side: V3,
    up: V3,
}

struct Buffer {
    hits: Vec<Option<Hit>>,
    /// The body's spheres, kept so face features can be placed on its true surface.
    body: Vec<(V3, f32)>,
}

impl Buffer {
    fn new() -> Buffer {
        Buffer {
            hits: vec![None; W * H],
            body: Vec::new(),
        }
    }
    fn get(&self, x: i32, y: i32) -> Option<Hit> {
        if x < 0 || y < 0 || x as usize >= W || y as usize >= H {
            return None;
        }
        self.hits[y as usize * W + x as usize]
    }
    fn sphere(&mut self, cam: &Camera, center: V3, r: f32, part: Part, u: f32, side: V3, up: V3) {
        if part == Part::Body {
            self.body.push((center, r));
        }
        let (sx, sy, depth, scale) = cam.project(center);
        let rs = r * scale;
        let (x0, x1) = (
            ((sx - rs).floor() as i32).max(0),
            ((sx + rs).ceil() as i32).min(W as i32 - 1),
        );
        let (y0, y1) = (
            ((sy - rs).floor() as i32).max(0),
            ((sy + rs).ceil() as i32).min(H as i32 - 1),
        );
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = ((x as f32 + 0.5 - sx) / rs, (y as f32 + 0.5 - sy) / rs);
                let flat = dx * dx + dy * dy;
                if flat > 1.0 {
                    continue;
                }
                let nz = (1.0 - flat).sqrt();
                let front = depth + r * nz;
                let cell = &mut self.hits[y as usize * W + x as usize];
                if cell.is_none_or(|hit| front > hit.depth) {
                    let normal = add(
                        add(mul(cam.right, dx), mul(cam.down, dy)),
                        mul(cam.toward, nz),
                    );
                    *cell = Some(Hit {
                        depth: front,
                        normal,
                        pos: add(center, mul(normal, r)),
                        part,
                        u,
                        side,
                        up,
                    });
                }
            }
        }
    }
}

// ------------------------------------------------------------------ the whale

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pattern {
    /// Slim hoops around the body, sized by each language's share of the colored skin.
    Rings,
    /// Long ribbons of light flowing the length of the body.
    Aurora,
    /// Soft painted patches.
    Patches,
    /// V-shaped waves that travel toward the tail.
    Tide,
    /// One long spiral wrapping the body, turning as it swims.
    Swirl,
    /// Solid color fading from one language into the next.
    Ombre,
}

impl Pattern {
    pub const ALL: [(&'static str, Pattern); 6] = [
        ("rings", Pattern::Rings),
        ("aurora", Pattern::Aurora),
        ("patches", Pattern::Patches),
        ("tide", Pattern::Tide),
        ("swirl", Pattern::Swirl),
        ("ombre", Pattern::Ombre),
    ];

    pub fn parse(name: &str) -> Option<Pattern> {
        Pattern::ALL
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, p)| p)
    }
}

fn parse_hex(text: &str) -> Option<Rgb> {
    let hex = text.strip_prefix('#')?;
    let value = u32::from_str_radix(hex, 16).ok()?;
    (hex.len() == 6).then_some(Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8))
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |x: u8, y: u8| lerp(f32::from(x), f32::from(y), t).round() as u8;
    Rgb(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// One ring: where it starts and ends along the body, and whose language it is.
#[derive(Clone, Copy, Debug)]
struct Ring {
    start: f32,
    end: f32,
    language: usize,
}

/// Lays out one ring per language so each covers its share of the skin. A ring's length is
/// not its share, because the body tapers: skin area goes with the radius, so the same share
/// makes a narrow ring on the wide back and a longer one near the thin tail.
fn lay_out_rings(shares: &[f32]) -> Vec<Ring> {
    const STEPS: usize = 4000;
    let total: f32 = shares.iter().sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let step = (1.0 - COLOR_START) / STEPS as f32;
    let area_of = |i: usize| radius(COLOR_START + (i as f32 + 0.5) * step) * step * LENGTH;
    let body_area: f32 = (0..STEPS).map(area_of).sum();
    let colored = body_area * RING_COVERAGE;
    // Each ring follows an equal band of white, so slim rings spread down the body.
    let gap_area = (body_area - colored) / shares.len() as f32;
    let mut rings = Vec::with_capacity(shares.len());
    let (mut i, mut walked) = (0, 0.0);
    for (language, &share) in shares.iter().enumerate() {
        // White comes first, so the last and smallest ring lands on the thin tail, where the
        // same area makes a longer, easier-to-see band.
        let gap_end = walked + gap_area;
        while i < STEPS && walked < gap_end {
            walked += area_of(i);
            i += 1;
        }
        let start = COLOR_START + i as f32 * step;
        let target = walked + share / total * colored;
        while i < STEPS && walked < target {
            walked += area_of(i);
            i += 1;
        }
        rings.push(Ring {
            start,
            end: COLOR_START + i as f32 * step,
            language,
        });
    }
    rings
}

fn ring_at(rings: &[Ring], u: f32) -> Option<usize> {
    rings
        .iter()
        .find(|r| u >= r.start && u < r.end)
        .map(|r| r.language)
}

/// Everything about the whale that does not change from frame to frame.
pub struct Whale {
    pattern: Pattern,
    /// Overall size: a calf is smaller than an adult.
    size: f32,
    /// How much bigger the head is than an adult's, relative to the body.
    head_boost: f32,
    skin: Ramp,
    ramps: Vec<Ramp>,
    rings: Vec<Ring>,
    slots: Vec<usize>,
    /// Colors for bands that travel: exactly FLOW_BANDS pass per lap, so the loop closes.
    flow: Vec<usize>,
    sorted: Vec<usize>,
    tips: Vec<usize>,
    bubbles: u32,
}

fn allot(shares: &[f32], slots: usize) -> Vec<usize> {
    let total: f32 = shares.iter().sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let mut used = vec![0.0f32; shares.len()];
    (0..slots)
        .map(|_| {
            let mut best = 0;
            let mut need = f32::MIN;
            for (i, &share) in shares.iter().enumerate() {
                let n = share / total * slots as f32 - used[i];
                if n > need {
                    need = n;
                    best = i;
                }
            }
            used[best] += 1.0;
            best
        })
        .collect()
}

impl Whale {
    /// The color each language is painted on the body, in the same order as the languages.
    pub fn language_colors(&self) -> Vec<Rgb> {
        self.ramps.iter().map(|ramp| ramp.0[2]).collect()
    }

    pub fn new(activity: &Activity, languages: &[Language], pattern: Pattern) -> Whale {
        // Growth: fast at first, with diminishing returns up to 20,000 contributions.
        let growth = ((1.0 + activity.total as f32 / 10.0).ln() / 2001f32.ln()).clamp(0.0, 1.0);
        // A calf is grey for its first contributions, then white.
        let whiteness = (activity.total as f32 / 25.0).min(1.0);
        let skin = Ramp(std::array::from_fn(|i| {
            mix(CALF_SKIN.0[i], ADULT_SKIN.0[i], whiteness)
        }));
        // The best streak ever makes colors richer, and never fades.
        let vivid = 0.72 + 0.2 * (activity.longest_streak_weeks as f32 / 26.0).min(1.0);
        let shares: Vec<f32> = languages.iter().map(|l| l.share).collect();
        let slots = allot(&shares, 40);
        let mut sorted = slots.clone();
        sorted.sort_unstable();
        Whale {
            pattern,
            size: 0.84 + 0.3 * growth,
            head_boost: 0.18 * (1.0 - growth),
            skin,
            ramps: languages
                .iter()
                .map(|l| {
                    Ramp::language(parse_hex(&l.color).unwrap_or(Rgb(0x8b, 0x93, 0xa7)), vivid)
                })
                .collect(),
            rings: lay_out_rings(&shares),
            tips: allot(&shares, 4),
            flow: allot(&shares, FLOW_BANDS),
            slots,
            sorted,
            bubbles: activity.bubbles(),
        }
    }
}

struct Sample {
    center: V3,
    r: f32,
    fwd: V3,
    side: V3,
    up: V3,
}

fn radius(u: f32) -> f32 {
    profile(
        &[
            (0.0, 3.4),
            (0.04, 4.4),
            (0.08, 6.0),
            (0.11, 8.0),
            (0.16, 9.8),
            (0.24, 10.4),
            (0.34, 12.2),
            (0.46, 13.4),
            (0.6, 12.0),
            (0.74, 8.4),
            (0.88, 4.6),
            (1.0, 2.2),
        ],
        u,
    )
}

/// The short beak sits low, under the overhang of the melon.
fn beak_drop(u: f32) -> f32 {
    profile(&[(0.0, -3.6), (0.07, -2.6), (0.15, 0.0)], u)
}

/// 1 on the head, fading to 0 by the shoulders.
fn head_part(u: f32) -> f32 {
    1.0 - ((u - 0.18) / 0.12).clamp(0.0, 1.0)
}

/// Swimming beats per lap: integers, so the loop closes.
fn beats(mood: Mood) -> f32 {
    match mood {
        Mood::Rest => 2.0,
        Mood::Drift => 3.0,
        Mood::Cruise => 4.0,
        Mood::Greet => 5.0,
        Mood::Swim => 6.0,
        Mood::Leap => 8.0,
    }
}

/// The spine: a level circle, with the back arched into a hump and a vertical swimming beat
/// that grows toward the tail. A leaping whale arcs out of the water twice a lap, and the
/// body follows the head along the same path.
fn spine(whale: &Whale, mood: Mood, t: f32) -> Vec<Sample> {
    let beat = TAU * t * beats(mood);
    let length = LENGTH * whale.size;
    let strength = if mood == Mood::Rest { 0.35 } else { 1.0 };
    let count = (LENGTH * 2.0) as usize;
    let points: Vec<(V3, f32)> = (0..=count)
        .map(|i| {
            let u = i as f32 / count as f32;
            let theta = TAU * t - u * length / RING;
            let lift = (5.2 * (u.min(1.0) * PI).sin().powi(2) - 2.6 * u) * whale.size;
            let swim = strength * (5.4 * u * u + 0.5) * (beat - u * 4.6).sin();
            let leap = if mood == Mood::Leap {
                20.0 * (theta * 2.0).sin().max(0.0)
            } else {
                0.0
            };
            (
                [RING * theta.cos(), RING * theta.sin(), lift + swim + leap],
                u,
            )
        })
        .collect();
    (0..=count)
        .map(|i| {
            let (a, b) = (points[i.saturating_sub(1)].0, points[(i + 1).min(count)].0);
            let fwd = unit(sub(a, b));
            let outward = unit(cross(fwd, [0.0, 0.0, 1.0]));
            let up0 = cross(outward, fwd);
            // Bank a little into the turn.
            let (sin, cos) = 0.14f32.sin_cos();
            let up = sub(mul(up0, cos), mul(outward, sin));
            let side = add(mul(outward, cos), mul(up0, sin));
            let (center, u) = points[i];
            let r = radius(u) * whale.size * (1.0 + whale.head_boost * head_part(u));
            Sample {
                center,
                r,
                fwd,
                side,
                up,
            }
        })
        .collect()
}

/// A bubble trail left along the ring: longer while a weekly streak is running.
fn draw_bubbles(frame: &mut [Option<Rgb>], cam: &Camera, whale: &Whale, t: f32) {
    for i in 0..whale.bubbles {
        let theta = i as f32 / whale.bubbles as f32 * TAU + hash(i) * 0.25;
        let age = (TAU * t - theta).rem_euclid(TAU) / TAU;
        let (spawn, life) = (0.2 + hash(i + 11) * 0.1, 0.14 + hash(i + 50) * 0.14);
        if age <= spawn || age >= spawn + life {
            continue;
        }
        let k = (age - spawn) / life;
        let rad = RING + (hash(i + 90) - 0.5) * 22.0;
        let (sx, sy, _, _) = cam.project([rad * theta.cos(), rad * theta.sin(), -10.0 + k * 22.0]);
        let (x, y) = (sx as i32, sy as i32);
        let color = Rgb(0x8f, 0xbd, 0xe0);
        let mut put = |px: i32, py: i32| {
            if px >= 0 && py >= 0 && (px as usize) < W && (py as usize) < H {
                frame[py as usize * W + px as usize] = Some(color);
            }
        };
        if k < 0.45 {
            put(x, y);
        } else {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                put(x + dx, y + dy);
            }
        }
    }
}

/// A leaping whale throws up a ring of droplets wherever it leaves or enters the water.
fn draw_splashes(frame: &mut [Option<Rgb>], cam: &Camera, t: f32) {
    for crossing in 0..4 {
        let theta = crossing as f32 * PI / 2.0;
        let age = (TAU * t - theta).rem_euclid(TAU) / TAU;
        if age >= 0.09 {
            continue;
        }
        let k = age / 0.09;
        for i in 0..14u32 {
            let spin = i as f32 / 14.0 * TAU;
            let reach = 5.0 + 15.0 * k;
            let at = [
                RING * theta.cos() + reach * spin.cos(),
                RING * theta.sin() + reach * spin.sin(),
                -8.0 + 16.0 * (k * PI).sin() * (0.5 + hash(i) * 0.5),
            ];
            let (sx, sy, _, _) = cam.project(at);
            let (x, y) = (sx as i32, sy as i32);
            if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
                frame[y as usize * W + x as usize] = Some(if i % 3 == 0 {
                    Rgb(255, 255, 255)
                } else {
                    Rgb(0x9c, 0xc9, 0xee)
                });
            }
        }
    }
}

fn draw_water(frame: &mut [Option<Rgb>], cam: &Camera, samples: &[Sample]) {
    let shade = Rgb(0x3f, 0x5f, 0x8a);
    for s in samples.iter().step_by(2) {
        let (sx, sy, _, scale) = cam.project([s.center[0], s.center[1], WATER_Z]);
        let (rx, ry) = (s.r * scale * 0.95, s.r * scale * ELEVATION.sin() * 0.95);
        for y in (sy - ry) as i32..=(sy + ry) as i32 + 1 {
            for x in (sx - rx) as i32..=(sx + rx) as i32 + 1 {
                if x < 0 || y < 0 || x as usize >= W || y as usize >= H || (x + y) % 2 != 0 {
                    continue;
                }
                let (dx, dy) = ((x as f32 + 0.5 - sx) / rx, (y as f32 + 0.5 - sy) / ry);
                if dx * dx + dy * dy <= 1.0 {
                    frame[y as usize * W + x as usize] = Some(shade);
                }
            }
        }
    }
}

/// How far along the body a point on the skin really is, from the nearest spine sample. The
/// sphere that drew a pixel can't say: where the body tapers, larger spheres ahead of it win
/// the depth test and would stretch every marking there.
fn along_body(samples: &[Sample], pos: V3) -> f32 {
    let (mut best, mut best_gap) = (0, f32::MAX);
    for (i, s) in samples.iter().enumerate() {
        let d = sub(pos, s.center);
        let gap = dot(d, d);
        if gap < best_gap {
            best_gap = gap;
            best = i;
        }
    }
    best as f32 / (samples.len() - 1) as f32
}

#[cfg(test)]
thread_local! {
    /// Body pixels drawn per ring language, so tests can measure the real picture.
    static RING_PIXELS: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn scene(whale: &Whale, mood: Mood, t: f32) -> Frame {
    let cam = Camera::new();
    let samples = spine(whale, mood, t);
    let mut frame: Frame = vec![None; W * H];
    let mut buffer = Buffer::new();
    draw_water(&mut frame, &cam, &samples);
    draw_bubbles(&mut frame, &cam, whale, t);
    let size = whale.size;

    let at_u =
        |u: f32| &samples[((u * (samples.len() - 1) as f32) as usize).min(samples.len() - 1)];

    // The body, with the beak held low and the melon bulging up off the brow.
    for (i, s) in samples.iter().enumerate() {
        let u = i as f32 / (samples.len() - 1) as f32;
        let center = add(s.center, mul(s.up, beak_drop(u)));
        buffer.sphere(&cam, center, s.r, Part::Body, u, s.side, s.up);
    }
    for k in 0..=24 {
        let u = lerp(0.055, 0.2, k as f32 / 24.0);
        let s = at_u(u);
        let bulge = ((u - 0.055) / 0.145 * PI).sin();
        let center = add(s.center, mul(s.up, (1.4 + 3.0 * bulge) * size));
        buffer.sphere(
            &cam,
            center,
            (6.4 + 2.6 * bulge) * size * (1.0 + whale.head_boost),
            Part::Body,
            u,
            s.side,
            s.up,
        );
    }
    // A low dorsal ridge along the top of the hump.
    for k in 0..=40 {
        let u = lerp(0.38, 0.72, k as f32 / 40.0);
        let s = at_u(u);
        let ridge = ((u - 0.38) / 0.34 * PI).sin();
        let center = add(s.center, mul(s.up, s.r * (0.3 + 0.38 * ridge)));
        buffer.sphere(&cam, center, s.r * 0.72, Part::Body, u, s.side, s.up);
    }

    // Flippers: rounded paddles that row slowly from the shoulder.
    let row = (TAU * t * 3.0).sin();
    let shoulder = at_u(0.31);
    for (index, side_sign) in [(0usize, 1.0f32), (1, -1.0)] {
        let root = add(
            shoulder.center,
            add(
                mul(shoulder.side, side_sign * shoulder.r * 0.8),
                mul(shoulder.up, -shoulder.r * 0.42),
            ),
        );
        let reach = add(
            add(
                mul(shoulder.side, side_sign * 0.82),
                mul(shoulder.fwd, -0.5 + 0.12 * row),
            ),
            mul(shoulder.up, -0.32 + 0.14 * row),
        );
        for k in 0..=26 {
            let v = k as f32 / 26.0;
            let spine_point = add(root, mul(reach, 17.0 * size * v));
            let chord = profile(&[(0.0, 2.2), (0.45, 4.4), (0.8, 3.0), (1.0, 0.6)], v) * size;
            for c in -4..=4 {
                let center = add(spine_point, mul(shoulder.fwd, chord * c as f32 / 4.0));
                buffer.sphere(
                    &cam,
                    center,
                    1.5,
                    Part::Flipper(index),
                    v,
                    shoulder.side,
                    shoulder.up,
                );
            }
        }
    }

    // Flukes: two lobes on the tail's own plane, so they pitch with the swimming beat.
    let tail = &samples[samples.len() - 1];
    let flex = 3.6 * (TAU * t * beats(mood) - 4.6).sin();
    for (index, side_sign) in [(0usize, 1.0f32), (1, -1.0)] {
        for k in 0..=44 {
            let v = k as f32 / 44.0;
            let middle = add(
                add(tail.center, mul(tail.fwd, (0.5 - 9.0 * v * v) * size)),
                add(
                    mul(tail.side, side_sign * 24.0 * size * v.powf(0.86)),
                    mul(tail.up, flex * v * v),
                ),
            );
            let chord = profile(&[(0.0, 3.0), (0.3, 6.4), (0.7, 4.2), (1.0, 0.5)], v) * size;
            for c in -5..=5 {
                let center = add(middle, mul(tail.fwd, chord * c as f32 / 5.0));
                buffer.sphere(&cam, center, 1.3, Part::Fluke(index), v, tail.side, tail.up);
            }
        }
    }

    let face = Face::new(&buffer, &samples);
    let closed = mood == Mood::Rest;
    let tip = |i: usize| {
        whale
            .tips
            .get(i)
            .map(|&language| whale.ramps[language])
            .unwrap_or(whale.skin)
    };
    let sun = unit([-0.5, 0.42, 0.76]);

    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let Some(hit) = buffer.get(x, y) else {
                continue;
            };
            // Wrapped light, so the shadow side falls off gently into blue.
            let light = (dot(hit.normal, sun) * 0.74 + 0.27).clamp(0.0, 1.0);
            let color = match hit.part {
                Part::Flipper(i) if hit.u > 0.62 => tip(2 + i).at(light * 0.8 + 0.1, x, y),
                Part::Fluke(i) if hit.u > 0.64 => tip(i).at(light * 0.8 + 0.1, x, y),
                Part::Body => {
                    let around = dot(hit.normal, hit.side)
                        .atan2(dot(hit.normal, hit.up))
                        .to_degrees();
                    let dither = BAYER[y.rem_euclid(4) as usize][x.rem_euclid(4) as usize] / 16.0;
                    if let Some(color) = (hit.u < 0.34)
                        .then(|| face.color_at(hit.pos, closed))
                        .flatten()
                    {
                        color
                    } else if let Some((language, shift)) =
                        marking(whale, along_body(&samples, hit.pos), around, t, dither)
                    {
                        #[cfg(test)]
                        RING_PIXELS.with(|counts| {
                            let mut counts = counts.borrow_mut();
                            if counts.len() <= language {
                                counts.resize(language + 1, 0);
                            }
                            counts[language] += 1;
                        });
                        whale.ramps[language].at(light * 0.85 + 0.12 + shift, x, y)
                    } else {
                        whale.skin.at(light, x, y)
                    }
                }
                _ => whale.skin.at(light, x, y),
            };
            frame[y as usize * W + x as usize] = Some(color);
        }
    }

    // Lines: around the silhouette, and where one part passes in front of another.
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let here = buffer.get(x, y);
            let neighbors = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)];
            match here {
                None => {
                    if neighbors
                        .iter()
                        .any(|&(nx, ny)| buffer.get(nx, ny).is_some())
                    {
                        frame[y as usize * W + x as usize] = Some(INK);
                    }
                }
                Some(hit) => {
                    let behind = neighbors.iter().any(|&(nx, ny)| {
                        buffer.get(nx, ny).is_some_and(|other| {
                            other.depth - hit.depth > 11.0
                                || (other.part != hit.part && other.depth > hit.depth)
                        })
                    });
                    if behind {
                        frame[y as usize * W + x as usize] = Some(LINE);
                    }
                }
            }
        }
    }

    let spouts: &[f32] = match mood {
        Mood::Rest | Mood::Leap => &[],
        Mood::Greet => &[0.2, 0.62],
        _ => &[0.52],
    };
    for &start in spouts {
        draw_breath(&mut frame, &cam, &samples, t, start);
    }
    if mood == Mood::Leap {
        draw_splashes(&mut frame, &cam, t);
    }
    frame
}

/// The face, anchored to fixed 3D points on the skin. A pixel belongs to a feature by its
/// real distance from that point, so both eyes always exist: as the head turns, each one
/// narrows at the edge of the head and slides out of view on its own.
struct Face {
    eyes: Vec<(V3, V3, V3)>,
    blushes: Vec<V3>,
    mouth: Vec<V3>,
}

impl Face {
    fn new(buffer: &Buffer, samples: &[Sample]) -> Face {
        let last = (samples.len() - 1) as f32;
        // Walks in from outside the whale until it touches the skin.
        let on_skin = |from: V3, direction: V3| {
            let mut reach = 26.0;
            while reach > 0.0 {
                let point = add(from, mul(direction, reach));
                if buffer
                    .body
                    .iter()
                    .any(|&(c, r)| dot(sub(point, c), sub(point, c)) <= r * r)
                {
                    return point;
                }
                reach -= 0.1;
            }
            from
        };
        let aim = |u: f32, around: f32, sign: f32, ahead: f32| {
            let s = &samples[(u * last) as usize];
            let (sin, cos) = around.to_radians().sin_cos();
            let direction = unit(add(
                add(mul(s.side, sign * sin), mul(s.up, cos)),
                mul(s.fwd, ahead),
            ));
            on_skin(add(s.center, mul(s.up, beak_drop(u))), direction)
        };
        let mut face = Face {
            eyes: Vec::new(),
            blushes: Vec::new(),
            mouth: Vec::new(),
        };
        for sign in [1.0f32, -1.0] {
            let s = &samples[(EYE_U * last) as usize];
            let eye = aim(EYE_U, EYE_AROUND, sign, EYE_AHEAD);
            face.eyes
                .push((eye, add(eye, add(mul(s.up, 0.85), mul(s.fwd, 0.85))), s.up));
            for k in 0..=44 {
                let u = lerp(0.06, 0.142, k as f32 / 44.0);
                let around = profile(
                    &[(0.06, 106.0), (0.11, 106.0), (0.13, 99.0), (0.142, 88.0)],
                    u,
                );
                face.mouth.push(aim(u, around, sign, 0.0));
            }
            face.blushes.push(aim(0.15, 100.0, sign, 0.45));
        }
        face
    }

    fn color_at(&self, pos: V3, closed: bool) -> Option<Rgb> {
        let near = |a: V3, r: f32| dot(sub(pos, a), sub(pos, a)) < r * r;
        for &(eye, glint, up) in &self.eyes {
            if closed {
                // A resting whale's eye is a soft, shut line.
                if near(eye, 2.6) && dot(sub(pos, eye), up).abs() < 0.6 {
                    return Some(Rgb(0x4d, 0x5d, 0x80));
                }
                continue;
            }
            if near(glint, 0.75) {
                return Some(Rgb(255, 255, 255));
            }
            if near(eye, 2.1) {
                return Some(Rgb(0x10, 0x13, 0x24));
            }
            if near(eye, 3.1) {
                return Some(Rgb(0x8c, 0xa0, 0xbe));
            }
        }
        if self.blushes.iter().any(|&b| near(b, BLUSH_RADIUS)) {
            return Some(Rgb(0xf7, 0xbd, 0xc8));
        }
        if self.mouth.iter().any(|&m| near(m, MOUTH_RADIUS)) {
            return Some(Rgb(0x4d, 0x5d, 0x80));
        }
        None
    }
}

/// Which language colors this point on the body, if any, plus a light adjustment. `x` runs
/// along the body and `y` around it, both in pixels. The head stays white for the face.
fn marking(whale: &Whale, u: f32, around: f32, t: f32, dither: f32) -> Option<(usize, f32)> {
    if whale.ramps.is_empty() {
        return None;
    }
    let (slots, sorted) = (&whale.slots, &whale.sorted);
    let edge = ((u - (COLOR_START - 0.015)) / 0.015).clamp(0.0, 1.0);
    if edge <= 0.0 {
        return None;
    }
    let x = u * LENGTH;
    let y = around.to_radians() * radius(u).max(4.0);
    let wrap = |i: i32| slots[i.rem_euclid(slots.len() as i32) as usize];
    let flowing = |band: f32| whale.flow[(band as i32).rem_euclid(FLOW_BANDS as i32) as usize];
    let hit = match whale.pattern {
        Pattern::Aurora => (0..9).find_map(|k| {
            let center =
                -160.0 + k as f32 * 40.0 + 9.0 * (u * 22.0 - TAU * t * 3.0 + k as f32 * 2.1).sin();
            let gap = (around - center + 180.0).rem_euclid(360.0) - 180.0;
            let width = 12.0 * edge * ((1.0 - u) / 0.1).clamp(0.0, 1.0);
            (gap.abs() < width)
                .then_some((wrap(k), 0.12 * (u * 40.0 - TAU * t * 5.0 + k as f32).sin()))
        }),
        Pattern::Patches => (0..30).find_map(|i| {
            let (bu, deg) = (
                0.25 + 0.72 * i as f32 / 30.0,
                (i as f32 * 137.5).rem_euclid(360.0) - 180.0,
            );
            let (dx, gap) = (
                x - bu * LENGTH,
                (around - deg + 180.0).rem_euclid(360.0) - 180.0,
            );
            let dy = gap.to_radians() * radius(bu).max(4.0);
            let wobble = 1.0 + 0.26 * (dx * 0.9 + dy * 1.3).sin();
            let size = 2.8 + hash(i as u32 * 5 + 2) * 2.6;
            (dx.hypot(dy) < size * wobble).then_some((wrap(i * 11), 0.0))
        }),
        Pattern::Rings => {
            // A gentle wave in each edge; it moves the boundary, not the ring's share.
            let wavy = u + 1.6 * (around.to_radians() * 2.0).sin() / LENGTH;
            ring_at(&whale.rings, wavy).map(|language| (language, 0.0))
        }
        Pattern::Tide => {
            let v = (x + y.abs() * 0.85) / 9.0 - t * FLOW_BANDS as f32;
            let band = v.floor();
            ((v - band) < 0.62).then_some((flowing(band), 0.0))
        }
        Pattern::Swirl => {
            let v = (x + around / 360.0 * 16.0 * 3.0) / 16.0 * 3.0 - t * FLOW_BANDS as f32;
            let band = v.floor();
            ((v - band) < 0.6).then_some((flowing(band), 0.0))
        }
        Pattern::Ombre => {
            let along = ((u - 0.25) / 0.72).clamp(0.0, 0.999) * sorted.len() as f32;
            let blur = (dither - 0.5) * 7.0;
            let index = ((along + blur) as i32).clamp(0, sorted.len() as i32 - 1) as usize;
            (around.abs() < 138.0 + dither * 12.0).then_some((sorted[index], 0.0))
        }
    };
    // The head's edge dissolves, so the color never ends on a hard line.
    hit.filter(|_| edge >= 1.0 || dither < edge)
}

/// Once a lap the whale breathes: a little spout that rises from the blowhole and falls.
fn draw_breath(frame: &mut [Option<Rgb>], cam: &Camera, samples: &[Sample], t: f32, start: f32) {
    let span = 0.17;
    if !(start..start + span).contains(&t) {
        return;
    }
    let k = (t - start) / span;
    let hole = samples[(0.2 * samples.len() as f32) as usize].center;
    let top = samples[(0.2 * samples.len() as f32) as usize].up;
    for i in 0..14u32 {
        let spread = (hash(i) - 0.5) * 2.0;
        let lift = 10.0 + 22.0 * (k * PI).sin() * (0.55 + hash(i + 7) * 0.45) + 4.0 * hash(i + 9);
        let sideways = add(
            mul(cam.right, spread * 9.0 * k),
            mul(cam.toward, (hash(i + 3) - 0.5) * 6.0 * k),
        );
        let p = add(add(hole, mul(top, lift)), sideways);
        let (sx, sy, _, _) = cam.project(p);
        let (x, y) = (sx as i32, sy as i32);
        if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
            frame[y as usize * W + x as usize] = Some(if i % 3 == 0 {
                Rgb(0xff, 0xff, 0xff)
            } else {
                Rgb(0x9c, 0xc9, 0xee)
            });
        }
    }
}

// ------------------------------------------------------------------ animation

/// Every frame of one lap, and how long each shows in hundredths of a second. A quieter
/// whale takes longer over the lap.
pub fn animate(whale: &Whale, mood: Mood) -> (Vec<Frame>, u16) {
    let delay = match mood {
        Mood::Rest => 12,
        Mood::Drift => 10,
        Mood::Cruise => 8,
        Mood::Greet | Mood::Swim => 7,
        Mood::Leap => 6,
    };
    let frames = (0..FRAMES)
        .map(|i| scene(whale, mood, i as f32 / FRAMES as f32))
        .collect();
    (frames, delay)
}

/// A single frame, for still images and the contact sheet.
pub fn still(whale: &Whale, mood: Mood, t: f32) -> Frame {
    scene(whale, mood, t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::Activity;

    fn activity(total: u64) -> Activity {
        Activity {
            total,
            ..Activity::quiet()
        }
    }

    fn languages(shares: &[f32]) -> Vec<Language> {
        const COLORS: [&str; 7] = [
            "#DA5B0B", "#3572A5", "#e34c26", "#b07219", "#dea584", "#3A4E3A", "#f34b7d",
        ];
        shares
            .iter()
            .enumerate()
            .map(|(i, &share)| Language {
                name: format!("L{i}"),
                color: COLORS[i].into(),
                share,
            })
            .collect()
    }

    /// Measures how much colored skin each language really got.
    fn measured_shares(rings: &[Ring], count: usize) -> Vec<f32> {
        const STEPS: usize = 20_000;
        let mut area = vec![0.0f32; count];
        for i in 0..STEPS {
            let u = COLOR_START + (1.0 - COLOR_START) * (i as f32 + 0.5) / STEPS as f32;
            if let Some(language) = ring_at(rings, u) {
                area[language] += radius(u);
            }
        }
        let total: f32 = area.iter().sum();
        area.iter().map(|a| a / total).collect()
    }

    #[test]
    fn rings_cover_exactly_each_languages_share() {
        for shares in [
            vec![0.83, 0.17],
            vec![0.283, 0.217, 0.2, 0.1, 0.1, 0.082, 0.018],
            vec![1.0],
        ] {
            let measured = measured_shares(&lay_out_rings(&shares), shares.len());
            for (want, got) in shares.iter().zip(&measured) {
                assert!((want - got).abs() < 0.005, "wanted {want}, measured {got}");
            }
        }
    }

    #[test]
    fn drawn_rings_match_the_shares_over_a_whole_lap() {
        let shares = [0.257, 0.197, 0.182, 0.182, 0.091, 0.074, 0.017];
        let a = activity(40);
        let whale = Whale::new(&a, &languages(&shares), Pattern::Rings);
        RING_PIXELS.with(|counts| counts.borrow_mut().clear());
        for i in 0..FRAMES {
            scene(&whale, Mood::Swim, i as f32 / FRAMES as f32);
        }
        let counts = RING_PIXELS.with(|counts| counts.borrow().clone());
        let total: u32 = counts.iter().sum();
        for (i, (&want, &got)) in shares.iter().zip(&counts).enumerate() {
            let measured = got as f32 / total as f32;
            println!(
                "language {i}: wanted {:.1}%, drew {:.1}%",
                want * 100.0,
                measured * 100.0
            );
            assert!(
                (measured - want).abs() <= 0.03 + want * 0.25,
                "language {i}: wanted {want}, drew {measured}"
            );
        }
    }

    #[test]
    fn a_tiny_language_still_gets_a_visible_ring() {
        let rings = lay_out_rings(&[0.283, 0.217, 0.2, 0.1, 0.1, 0.082, 0.018]);
        let smallest = rings[6];
        assert!(
            (smallest.end - smallest.start) * LENGTH >= 2.0,
            "{smallest:?}"
        );
    }

    #[test]
    fn the_same_inputs_draw_the_same_pixels() {
        let (a, b) = (activity(500), activity(500));
        let langs = languages(&[0.6, 0.4]);
        let (x, y) = (
            Whale::new(&a, &langs, Pattern::Rings),
            Whale::new(&b, &langs, Pattern::Rings),
        );
        assert_eq!(scene(&x, Mood::Swim, 0.3), scene(&y, Mood::Swim, 0.3));
    }

    #[test]
    fn every_mood_and_pattern_loops_back_to_its_first_frame() {
        let langs = languages(&[0.5, 0.3, 0.2]);
        let a = activity(4000);
        for (name, pattern) in Pattern::ALL {
            for mood in Mood::ALL {
                let whale = Whale::new(&a, &langs, pattern);
                let (first, last) = (scene(&whale, mood, 0.0), scene(&whale, mood, 0.999_999));
                let different = first.iter().zip(&last).filter(|(x, y)| x != y).count();
                assert!(
                    different < 120,
                    "{name}/{mood:?} jumps {different} pixels at the loop point"
                );
            }
        }
    }

    #[test]
    fn a_whale_with_no_languages_still_draws() {
        let whale = Whale::new(&activity(0), &[], Pattern::Rings);
        assert!(scene(&whale, Mood::Drift, 0.2).iter().flatten().count() > 200);
    }

    #[test]
    fn nothing_leaves_the_canvas_at_full_size() {
        // The largest whale, at its leap, must not be cut off by the top or side edges.
        let a = activity(1_000_000);
        let whale = Whale::new(&a, &languages(&[1.0]), Pattern::Rings);
        for i in 0..FRAMES {
            let frame = scene(&whale, Mood::Leap, i as f32 / FRAMES as f32);
            let edge = (0..W).any(|x| frame[x].is_some() || frame[(H - 1) * W + x].is_some())
                || (0..H).any(|y| frame[y * W].is_some() || frame[y * W + W - 1].is_some());
            assert!(!edge, "frame {i} touches the edge");
        }
    }
}
