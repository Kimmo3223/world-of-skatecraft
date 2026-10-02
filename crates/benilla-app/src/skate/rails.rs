//! Grind rails found in the map's collision: the lips a skater can grind.
//!
//! Ported from the IW4L skate mode (chasmlol/2010-rust-rewrite-mashup,
//! `crates/render_anim/src/skate/rails.rs`, Apache-2.0; this file stays
//! Apache-2.0). WoW collision is terrain, WMO groups and doodad hulls, so the
//! top of a ledge and its wall rarely share an edge vertex for vertex. A lip
//! is therefore found by probing the geometry around each edge of a
//! walkable face rather than by matching triangles: the ground must fall away
//! just past the edge and nothing may rise there. Surviving edges are merged
//! along their lines across seams and T-junctions, then chained into
//! polylines where they meet at a gentle turn, as the authoring tool does for
//! edges marked sharp.

#![allow(clippy::type_complexity)]

use std::collections::HashMap;

use bevy::math::Vec3;

/// A face this upward is walkable; its edges are rail candidates.
const UPWARD_Z: f32 = 0.65;
/// XY grid cell for probing, in inches (the tuning's unit; callers convert).
const CELL: f32 = 64.0;
/// How far past the lip the probes look.
const PROBE_OUT: f32 = 2.0;
/// Open space the ground must drop below the lip, just past it.
const MIN_DROP: f32 = 3.0;
/// Height above the lip the wall probe runs at.
const WALL_PROBE_UP: f32 = 3.0;
/// Shortest rail kept, after merging and chaining.
const MIN_RAIL: f32 = 24.0;
/// Steepest rail, as rise over length: stair handrails and ramp edges pass.
const MAX_SLOPE: f32 = 0.7;
/// Chains continue through a joint turning less than about 35 degrees.
const MIN_TURN_COS: f32 = 0.82;
/// The skate engine counts rails in a u16.
const MAX_RAILS: usize = u16::MAX as usize;

pub struct RailCensus {
    pub candidates: usize,
    pub lips: usize,
    pub runs: usize,
    pub rails: usize,
}

/// Rails over `tris` (inches, z up), each a polyline of two or more points.
pub fn find(tris: &[[Vec3; 3]]) -> (Vec<Vec<Vec3>>, RailCensus) {
    let grid = Grid::build(tris);
    let mut probe = Probe {
        tris,
        grid: &grid,
        stamp: vec![0; tris.len()],
        round: 0,
        scratch: Vec::new(),
    };

    // Every edge of a walkable face, once.
    let key = |v: Vec3| v.to_array().map(|x| (x * 8.).round() as i32);
    let mut edges: HashMap<([i32; 3], [i32; 3]), (Vec3, Vec3, Vec3, Vec3)> = HashMap::new();
    for tri in tris {
        let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
        if normal.z <= UPWARD_Z {
            continue;
        }
        let centroid = (tri[0] + tri[1] + tri[2]) / 3.;
        for i in 0..3 {
            let (a, b) = (tri[i], tri[(i + 1) % 3]);
            let (ka, kb) = (key(a), key(b));
            let k = if ka < kb { (ka, kb) } else { (kb, ka) };
            edges.entry(k).or_insert((a, b, normal, centroid));
        }
    }
    let candidates = edges.len();

    let mut lips = Vec::new();
    for (a, b, normal, centroid) in edges.into_values() {
        let along = b - a;
        let len = along.length();
        if len < 1. || along.z.abs() > MAX_SLOPE * len {
            continue;
        }
        let mid = (a + b) * 0.5;
        let mut out = along.cross(normal).normalize_or_zero();
        if out.dot(centroid - mid) > 0. {
            out = -out;
        }
        let samples: &[f32] = if len < 12. {
            &[0.5]
        } else {
            &[0.25, 0.5, 0.75]
        };
        let passing = samples
            .iter()
            .filter(|&&s| probe.is_lip(a + along * s, out))
            .count();
        if passing * 3 >= samples.len() * 2 {
            lips.push((a, b));
        }
    }
    let lip_count = lips.len();

    let runs = merge_collinear(lips);
    let run_count = runs.len();
    let mut rails = chain(runs);
    rails.retain(|rail| polyline_len(rail) >= MIN_RAIL);
    if rails.len() > MAX_RAILS {
        rails.sort_by(|a, b| polyline_len(b).total_cmp(&polyline_len(a)));
        rails.truncate(MAX_RAILS);
    }
    let census = RailCensus {
        candidates,
        lips: lip_count,
        runs: run_count,
        rails: rails.len(),
    };
    (rails, census)
}

fn polyline_len(points: &[Vec3]) -> f32 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

struct Grid {
    cells: HashMap<(i32, i32), Vec<u32>>,
}

fn cell_of(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

impl Grid {
    fn build(tris: &[[Vec3; 3]]) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (index, tri) in tris.iter().enumerate() {
            let min = tri[0].min(tri[1]).min(tri[2]);
            let max = tri[0].max(tri[1]).max(tri[2]);
            for x in cell_of(min.x)..=cell_of(max.x) {
                for y in cell_of(min.y)..=cell_of(max.y) {
                    cells.entry((x, y)).or_default().push(index as u32);
                }
            }
        }
        Self { cells }
    }
}

struct Probe<'a> {
    tris: &'a [[Vec3; 3]],
    grid: &'a Grid,
    stamp: Vec<u32>,
    round: u32,
    scratch: Vec<u32>,
}

impl Probe<'_> {
    /// Past `p` along `out` the ground falls away and nothing rises.
    fn is_lip(&mut self, p: Vec3, out: Vec3) -> bool {
        let down_from = p + out * PROBE_OUT + Vec3::Z;
        if self.hit(down_from, -Vec3::Z, 1. + MIN_DROP) {
            return false;
        }
        let across_from = p - out + Vec3::Z * WALL_PROBE_UP;
        !self.hit(across_from, out, 2. + PROBE_OUT)
    }

    /// Whether the segment from `origin` along unit `dir` for `len` crosses
    /// any triangle, either side facing.
    fn hit(&mut self, origin: Vec3, dir: Vec3, len: f32) -> bool {
        let end = origin + dir * len;
        let (min, max) = (origin.min(end), origin.max(end));
        self.round = self.round.wrapping_add(1);
        if self.round == 0 {
            self.stamp.fill(0);
            self.round = 1;
        }
        self.scratch.clear();
        for x in cell_of(min.x)..=cell_of(max.x) {
            for y in cell_of(min.y)..=cell_of(max.y) {
                let Some(cell) = self.grid.cells.get(&(x, y)) else {
                    continue;
                };
                for &index in cell {
                    let seen = &mut self.stamp[index as usize];
                    if *seen != self.round {
                        *seen = self.round;
                        self.scratch.push(index);
                    }
                }
            }
        }
        self.scratch
            .iter()
            .any(|&index| ray_triangle(origin, dir, len, &self.tris[index as usize]))
    }
}

/// Möller–Trumbore, both faces, hits within `(0, len]`.
fn ray_triangle(origin: Vec3, dir: Vec3, len: f32, tri: &[Vec3; 3]) -> bool {
    let e1 = tri[1] - tri[0];
    let e2 = tri[2] - tri[0];
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-8 {
        return false;
    }
    let inv = 1. / det;
    let s = origin - tri[0];
    let u = s.dot(p) * inv;
    if !(0. ..=1.).contains(&u) {
        return false;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0. || u + v > 1. {
        return false;
    }
    let t = e2.dot(q) * inv;
    t > 1e-4 && t <= len
}

/// Lips on one line, merged into maximal runs where they overlap or touch.
fn merge_collinear(lips: Vec<(Vec3, Vec3)>) -> Vec<(Vec3, Vec3)> {
    let mut lines: HashMap<([i32; 3], [i32; 3]), (Vec3, Vec3, Vec<(f32, f32)>)> = HashMap::new();
    for (mut a, mut b) in lips {
        let mut d = (b - a).normalize_or_zero();
        if d == Vec3::ZERO {
            continue;
        }
        let flip =
            d.x < -1e-4 || (d.x.abs() <= 1e-4 && (d.y < -1e-4 || (d.y.abs() <= 1e-4 && d.z < 0.)));
        if flip {
            d = -d;
            std::mem::swap(&mut a, &mut b);
        }
        let o = a - d * a.dot(d);
        let k = (
            (d * 64.).to_array().map(|x| x.round() as i32),
            (o * 2.).to_array().map(|x| x.round() as i32),
        );
        let line = lines.entry(k).or_insert((d, o, Vec::new()));
        line.2.push((a.dot(line.0), b.dot(line.0)));
    }
    let mut runs = Vec::new();
    for (d, o, mut spans) in lines.into_values() {
        spans.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut current = spans[0];
        for &(t0, t1) in &spans[1..] {
            if t0 <= current.1 + 1.5 {
                current.1 = current.1.max(t1);
            } else {
                runs.push((o + d * current.0, o + d * current.1));
                current = (t0, t1);
            }
        }
        runs.push((o + d * current.0, o + d * current.1));
    }
    runs
}

/// Runs joined end to end into polylines through joints where exactly two
/// runs meet at a gentle turn.
fn chain(runs: Vec<(Vec3, Vec3)>) -> Vec<Vec<Vec3>> {
    let node = |v: Vec3| v.to_array().map(|x| x.round() as i32);
    let mut at: HashMap<[i32; 3], Vec<(usize, bool)>> = HashMap::new();
    for (i, (a, b)) in runs.iter().enumerate() {
        at.entry(node(*a)).or_default().push((i, false));
        at.entry(node(*b)).or_default().push((i, true));
    }
    let mut used = vec![false; runs.len()];
    let far = |i: usize, from_end: bool| if from_end { runs[i].0 } else { runs[i].1 };
    let near = |i: usize, from_end: bool| if from_end { runs[i].1 } else { runs[i].0 };
    // The run continuing a polyline at `joint`, arriving along `heading`.
    let next = |joint: Vec3, heading: Vec3, used: &[bool]| -> Option<(usize, bool)> {
        let there = at.get(&node(joint))?;
        if there.len() != 2 {
            return None;
        }
        let &(i, at_end) = there.iter().find(|(i, _)| !used[*i])?;
        let leave = (far(i, at_end) - near(i, at_end)).normalize_or_zero();
        (heading.dot(leave) >= MIN_TURN_COS).then_some((i, at_end))
    };
    let mut rails = Vec::new();
    for start in 0..runs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let (a, b) = runs[start];
        let mut points = std::collections::VecDeque::from([a, b]);
        // Forward from b, then backward from a.
        let mut tip = b;
        let mut heading = (b - a).normalize_or_zero();
        while let Some((i, at_end)) = next(tip, heading, &used) {
            used[i] = true;
            let to = far(i, at_end);
            heading = (to - tip).normalize_or_zero();
            tip = to;
            points.push_back(to);
        }
        let mut tip = a;
        let mut heading = (a - b).normalize_or_zero();
        while let Some((i, at_end)) = next(tip, heading, &used) {
            used[i] = true;
            let to = far(i, at_end);
            heading = (to - tip).normalize_or_zero();
            tip = to;
            points.push_front(to);
        }
        rails.push(points.into_iter().collect());
    }
    rails
}
