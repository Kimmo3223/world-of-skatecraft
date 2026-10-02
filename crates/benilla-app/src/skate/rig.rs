//! The retarget: the skater's solved skeleton onto our character's M2 skeleton, as the IW4L skate
//! mode poses its soldier. Each mapped WoW bone's bind segment (its pivot toward its mapped child)
//! is aligned to the reference skater's, then carried by that skater bone's solved pose. Unmapped
//! bones (fingers, cloak, attachment points) keep their own animation under their parent: the
//! sheath points, for one, are turned by constant animation tracks, not by their bind pose.
//!
//! WoW limbs are found from the KeyBoneIDs vanilla tags (Root, Waist, SpineLow, Head, the
//! shoulders and upper arms) and the hierarchy below them, so every race's skeleton maps.

use std::sync::OnceLock;

use bevy::prelude::*;
use benilla_world::rig_anim::RigPose;

use super::{SkatePose, METERS_PER_YARD};

#[derive(serde::Deserialize)]
pub(super) struct ReferenceBone {
    pub(super) name: String,
    pub(super) bind: [f32; 16],
    pub(super) inverse_bind: [f32; 16],
}

/// The reference skater's bind pose (`rig.json`), engine space.
pub(super) fn reference() -> Option<&'static [ReferenceBone]> {
    static RIG: OnceLock<Option<Vec<ReferenceBone>>> = OnceLock::new();
    RIG.get_or_init(|| {
        let root = super::assets_root()?;
        let data = std::fs::read(root.join("rig.json")).ok()?;
        serde_json::from_slice(&data)
            .map_err(|e| warn!("skate rig.json: {e}"))
            .ok()
    })
    .as_deref()
}

fn reference_bind(name: &str) -> Option<Mat4> {
    reference()?
        .iter()
        .find(|b| b.name == name)
        .map(|b| Mat4::from_cols_array(&b.bind))
}

/// A WoW bone driven by a skater bone, and the child pair that fixes its direction.
struct Mapping {
    bone: usize,
    skate: &'static str,
    child: Option<(usize, &'static str)>,
}

/// KeyBoneIDs (wowdev's `KeyBone` enum).
mod key {
    pub const ARM_L: i16 = 0;
    pub const ARM_R: i16 = 1;
    pub const SHOULDER_L: i16 = 2;
    pub const SHOULDER_R: i16 = 3;
    pub const SPINE_LOW: i16 = 4;
    pub const WAIST: i16 = 5;
    pub const HEAD: i16 = 6;
}

/// The skeleton's mappings, found once per rig layout.
fn map_skeleton(parents: &[i16], pivots: &[Vec3], key_bones: &[i16]) -> Vec<Mapping> {
    let n = parents.len();
    let parent = |i: usize| usize::try_from(parents[i]).ok().filter(|&p| p < n);
    let mut children = vec![Vec::new(); n];
    for i in 0..n {
        if let Some(p) = parent(i) {
            children[p].push(i);
        }
    }
    let key = |k: i16| key_bones.iter().position(|&b| b == k);
    // The limb's next joint: the farthest child that carries the chain on, else the farthest.
    let next = |i: usize| -> Option<usize> {
        let far = |c: &&usize| (pivots[**c] - pivots[i]).length();
        children[i]
            .iter()
            .filter(|c| !children[**c].is_empty())
            .max_by(|a, b| far(a).total_cmp(&far(b)))
            .or_else(|| children[i].iter().max_by(|a, b| far(a).total_cmp(&far(b))))
            .filter(|c| (pivots[**c] - pivots[i]).length() > 0.01)
            .copied()
    };
    let mut out = Vec::new();
    let mut push = |bone: Option<usize>, skate, child: Option<(Option<usize>, &'static str)>| {
        if let Some(bone) = bone {
            let child = child.and_then(|(c, name)| Some((c?, name)));
            out.push(Mapping { bone, skate, child });
        }
    };

    let waist = key(key::WAIST);
    let spine = key(key::SPINE_LOW);
    let head = key(key::HEAD);
    let neck = head.and_then(parent);
    let chest = neck.and_then(parent).filter(|&c| Some(c) != spine);
    push(waist, "HIPS", Some((spine, "SPINE")));
    match chest {
        Some(_) => {
            push(spine, "SPINE", Some((chest, "SPINE2")));
            push(chest, "SPINE2", Some((neck, "NECK")));
        }
        None => push(spine, "SPINE", Some((neck, "NECK"))),
    }
    push(neck, "NECK", Some((head, "HEAD")));
    push(head, "HEAD", None);

    let arm = |shoulder: i16, upper: i16| {
        let shoulder = key(shoulder);
        let upper = key(upper);
        let elbow = upper.and_then(next);
        let wrist = elbow.and_then(next);
        (shoulder, upper, elbow, wrist)
    };
    for (side, (shoulder, upper, elbow, wrist)) in [
        ("LEFT", arm(key::SHOULDER_L, key::ARM_L)),
        ("RIGHT", arm(key::SHOULDER_R, key::ARM_R)),
    ] {
        let name = |part: &str| -> &'static str {
            match (side, part) {
                ("LEFT", "SHOULDER") => "LEFTSHOULDER",
                ("LEFT", "ARM") => "LEFTARM",
                ("LEFT", "FOREARM") => "LEFTFOREARM",
                ("LEFT", _) => "LEFTHAND",
                (_, "SHOULDER") => "RIGHTSHOULDER",
                (_, "ARM") => "RIGHTARM",
                (_, "FOREARM") => "RIGHTFOREARM",
                _ => "RIGHTHAND",
            }
        };
        push(shoulder, name("SHOULDER"), Some((upper, name("ARM"))));
        push(upper, name("ARM"), Some((elbow, name("FOREARM"))));
        push(elbow, name("FOREARM"), Some((wrist, name("HAND"))));
        push(wrist, name("HAND"), None);
    }

    // The thighs: the waist's children that carry a chain and sit off to a side. Our model space
    // faces −Z with its left on −X.
    if let Some(w) = waist {
        let thighs: Vec<usize> = children[w]
            .iter()
            .copied()
            .filter(|&c| !children[c].is_empty() && (pivots[c].x - pivots[w].x).abs() > 0.03)
            .collect();
        let side = |left: bool| {
            thighs
                .iter()
                .copied()
                .filter(|&c| (pivots[c].x < pivots[w].x) == left)
                .max_by(|&a, &b| {
                    (pivots[a].x - pivots[w].x)
                        .abs()
                        .total_cmp(&(pivots[b].x - pivots[w].x).abs())
                })
        };
        for (left, names) in [
            (true, ["LEFTUPLEG", "LEFTLEG", "LEFTFOOT", "LEFTTOEBASE"]),
            (false, ["RIGHTUPLEG", "RIGHTLEG", "RIGHTFOOT", "RIGHTTOEBASE"]),
        ] {
            let thigh = side(left);
            let knee = thigh.and_then(next);
            let ankle = knee.and_then(next);
            let toe = ankle.and_then(next);
            push(thigh, names[0], Some((knee, names[1])));
            push(knee, names[1], Some((ankle, names[2])));
            push(ankle, names[2], Some((toe, names[3])));
            push(toe, names[3], None);
        }
    }
    out
}

/// Per WoW bone, the map from its model space (meters) to the skater's bind space: its bind
/// segment turned onto the skater's and its pivot moved onto the skater joint. `None` for
/// unmapped bones.
fn fits(maps: &[Mapping], pivots: &[Vec3]) -> Vec<Option<(&'static str, Mat4)>> {
    // The skater faces +Z with its left on +X; our model faces −Z with its left on −X.
    let facing = Quat::from_rotation_y(std::f32::consts::PI);
    let mut out = vec![None; pivots.len()];
    let mapped_parent = |bone: usize| maps.iter().find(|m| m.child.is_some_and(|(c, _)| c == bone));
    for m in maps {
        let Some(bind) = reference_bind(m.skate) else {
            continue;
        };
        let from = pivots[m.bone] * METERS_PER_YARD;
        let to = bind.w_axis.truncate();
        let segment = |a: Vec3, b: Vec3, sa: Vec3, sb: Vec3| {
            Some(Quat::from_rotation_arc(
                (facing * (b - a)).try_normalize()?,
                (sb - sa).try_normalize()?,
            ))
        };
        let turn = m
            .child
            .and_then(|(c, skate)| {
                let c_to = reference_bind(skate)?.w_axis.truncate();
                segment(from, pivots[c] * METERS_PER_YARD, to, c_to)
            })
            .or_else(|| {
                // A terminal joint takes its parent segment's alignment rather than a new roll,
                // keeping wrists and the head seated.
                let p = mapped_parent(m.bone)?;
                let p_to = reference_bind(p.skate)?.w_axis.truncate();
                segment(pivots[p.bone] * METERS_PER_YARD, from, p_to, to)
            })
            .unwrap_or(Quat::IDENTITY);
        let rotation = turn * facing;
        out[m.bone] = Some((
            m.skate,
            Mat4::from_rotation_translation(rotation, to - rotation * from),
        ));
    }
    out
}

/// A rig's retarget plan, cached by its bone count and KeyBoneIDs.
#[derive(Default)]
pub(super) struct Plan {
    layout: Vec<i16>,
    parents: Vec<i16>,
    pivots: Vec<Vec3>,
    fits: Vec<Option<(&'static str, Mat4)>>,
    /// Unmapped bones that ride a mapped bone other than their nearest ancestor: the hip sheath
    /// points hang off SpineLow, but sit on the pelvis, which a crouching skater's spine leaves.
    rides: Vec<Option<usize>>,
}

/// The bone turns held across a ride ([`retarget`]).
#[derive(Default)]
pub(super) struct Held {
    rotations: Vec<Quat>,
}

/// SpineLow's unmapped children off to a side (the hip sheath points, attachments 32 and 33)
/// ride the Waist bone instead.
fn hip_riders(parents: &[i16], pivots: &[Vec3], key_bones: &[i16], fits: &[Option<(&str, Mat4)>]) -> Vec<Option<usize>> {
    let key = |k: i16| key_bones.iter().position(|&b| b == k);
    let mut rides = vec![None; parents.len()];
    let (Some(spine), Some(waist)) = (key(key::SPINE_LOW), key(key::WAIST)) else {
        return rides;
    };
    for (i, ride) in rides.iter_mut().enumerate() {
        let side = (pivots[i].x - pivots[spine].x).abs();
        if usize::try_from(parents[i]).ok() == Some(spine) && fits[i].is_none() && side > 0.1 {
            *ride = Some(waist);
        }
    }
    rides
}

impl Plan {
    fn for_rig(&mut self, rig: &RigPose) -> &Self {
        if self.layout != rig.key_bones || self.parents != rig.parents {
            let n = rig.parents.len();
            let binds = rig.bind_translations();
            let mut pivots = vec![Vec3::ZERO; n];
            for i in 0..n {
                let parent = usize::try_from(rig.parents[i]).ok().filter(|&p| p < i);
                pivots[i] = binds[i] + parent.map_or(Vec3::ZERO, |p| pivots[p]);
            }
            let maps = map_skeleton(&rig.parents, &pivots, &rig.key_bones);
            info!(
                "skate retarget: {} of {} bones mapped ({})",
                maps.len(),
                n,
                maps.iter().map(|m| m.skate).collect::<Vec<_>>().join(" ")
            );
            self.fits = fits(&maps, &pivots);
            self.rides = hip_riders(&rig.parents, &pivots, &rig.key_bones, &self.fits);
            self.layout = rig.key_bones.clone();
            self.parents = rig.parents.clone();
            self.pivots = pivots;
        }
        self
    }
}

/// Writes our rig's locals from the skater's pose, in the pose post-pass, so the compose and
/// palette passes draw it as any animated pose.
#[allow(clippy::type_complexity)]
pub(super) fn retarget(
    pose: Res<SkatePose>,
    mut plan: Local<Plan>,
    mut held: Local<Held>,
    mut units: Query<(&Transform, &GlobalTransform, &mut RigPose), With<crate::net::Embodied>>,
    globals: Query<&GlobalTransform>,
) {
    if !pose.active || pose.bones.is_empty() {
        held.rotations.clear();
        return;
    }
    let count = units.iter().count();
    let Ok((unit_t, unit_g, mut rig)) = units.single_mut() else {
        debug_once(&format!("no single embodied rig ({count})"));
        return;
    };
    if rig.key_bones.is_empty() {
        debug_once("rig has no key bones");
        return;
    }
    // The model frame this frame: the unit's fresh transform carries last frame's offset to the
    // frame node, as the propagation that refreshes `GlobalTransform` runs after this pass.
    let Ok(frame_g) = globals.get(rig.joints_root) else {
        debug_once("joints root has no GlobalTransform");
        return;
    };
    let offset = unit_g.affine().inverse() * frame_g.affine();
    let model_from_world = Mat4::from(unit_t.compute_affine() * offset).inverse();

    let plan = plan.for_rig(&rig);
    let n = plan.pivots.len();
    if rig.locals.len() != n {
        debug_once("locals/pivots length mismatch");
        return;
    }
    // Engine meters around `origin` to our world yards, both ways round a skin.
    let to_world = Mat4::from_translation(pose.origin)
        * Mat4::from_scale(Vec3::splat(1.0 / METERS_PER_YARD));
    let from_model = Mat4::from_scale(Vec3::splat(METERS_PER_YARD));
    // Each bone's skin: model yards to world yards.
    let mut skins: Vec<Option<Mat4>> = vec![None; n];
    for i in 0..n {
        if let Some((name, fit)) = plan.fits[i] {
            let (Some(posed), Some(bind)) = (pose.bone(name), reference_bind(name)) else {
                continue;
            };
            skins[i] = Some(to_world * posed * bind.inverse() * fit * from_model);
        }
    }
    let hips = plan
        .fits
        .iter()
        .zip(&skins)
        .find_map(|(f, s)| f.filter(|(name, _)| *name == "HIPS").and(*s));
    let Some(hips) = hips else {
        debug_once("no HIPS skin");
        return;
    };
    // The riders' animated turns, taken on the first ride frame, before any of ours is written:
    // a rider's local is rewritten against another bone, so it cannot be read back.
    if held.rotations.len() != n {
        held.rotations = rig.locals.iter().map(|l| l.rotation).collect();
    }
    let mut models = vec![Mat4::IDENTITY; n];
    for i in 0..n {
        let parent = usize::try_from(plan.parents[i]).ok().filter(|&p| p < i);
        let rider = plan.rides[i].filter(|&a| a < i);
        models[i] = match (skins[i], rider, parent) {
            (Some(skin), ..) => model_from_world * skin * Mat4::from_translation(plan.pivots[i]),
            (None, Some(a), _) => {
                models[a]
                    * Mat4::from_rotation_translation(
                        held.rotations[i],
                        plan.pivots[i] - plan.pivots[a],
                    )
            }
            // Its animated local, unchanged, under its posed parent.
            (None, None, Some(p)) => models[p] * rig.locals[i].to_matrix(),
            (None, None, None) => {
                model_from_world * hips * Mat4::from_translation(plan.pivots[i])
            }
        };
        if skins[i].is_none() && rider.is_none() && parent.is_some() {
            continue;
        }
        let local = match parent {
            Some(p) => models[p].inverse() * models[i],
            None => models[i],
        };
        rig.locals[i] = Transform::from_matrix(local);
    }
    rig.pose_dirty = true;
}

/// Logs `what` once per distinct message, for chasing a silent early-out.
fn debug_once(what: &str) {
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap();
    if !seen.iter().any(|s| s.split(':').next() == what.split(':').next()) {
        info!("skate retarget: {what}");
        seen.push(what.to_string());
    }
}
