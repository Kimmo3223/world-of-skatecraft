//! Skate sounds, from the player's own Skate 3 audio (`tools/skate_audio.py` writes them to
//! `skate-audio/`): rolling (concrete on buildings and props, asphalt on terrain), grinds and
//! wind as loops that follow the board's speed, and one-shots for the pop, flips, landings,
//! powerslide skids and bails, keyed off the engine's physical state. Played 2D: the skater is
//! the listener.

use std::path::PathBuf;
use std::time::Duration;

use avian3d::prelude::CollisionLayers;
use bevy::prelude::*;
use benilla_world::collision::{GroundDecalSurface, WorldCollision};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::{Decibels, Tween};

use super::{SkateDrive, METERS_PER_YARD};

/// The engine's physical state, as the sounds group it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Phase {
    #[default]
    Off,
    Ground,
    Slide,
    Air,
    /// A grind; `true` for the board-on-rail slides, `false` for the truck grinds.
    Grind(bool),
    Wipeout,
    /// Off the board, plants and anything else that rolls no wheels.
    Other,
}

impl Phase {
    pub(super) fn of(state: &str) -> Self {
        match state {
            "PhysicsGround" | "RevertGround" | "GroundAnimation" | "Skitching" | "FollowPath"
            | "LandingOnDeck" => Self::Ground,
            "SlideGround" => Self::Slide,
            "PhysicsAir" | "KnownAir" | "PhysicsAirSecondary" | "Boneless" => Self::Air,
            "GrindBoardslide" | "GrindTipslide" | "GrindDarkslide" => Self::Grind(true),
            "GrindFiftyFifty" | "GrindFiveO" | "GrindBackslash" => Self::Grind(false),
            "WipeoutGround" => Self::Wipeout,
            _ => Self::Other,
        }
    }

    fn rolling(self) -> bool {
        matches!(self, Self::Ground | Self::Slide)
    }
}

/// A one-shot the state machine asks for.
#[derive(Clone, Copy, Debug)]
pub(super) enum Cue {
    Pop,
    Land { hard: bool },
    Bail,
}

/// What the skate update hands the sounds each frame: the phase and this frame's cues.
#[derive(Resource, Default)]
pub(crate) struct SkateAudio {
    pub(super) phase: Phase,
    pub(super) cues: Vec<Cue>,
    /// Deepest fall speed (m/s, negative down) since take-off, for the landing's weight.
    pub(super) fall: f32,
}

/// A landing this fast (m/s downward) or harder plays the heavy set.
const HARD_LANDING: f32 = 5.0;

impl SkateAudio {
    /// Steps the phase machine on a new engine pose.
    pub(super) fn observe(&mut self, state: &str, velocity: Vec3) {
        let next = Phase::of(state);
        let prev = self.phase;
        if next == Phase::Air {
            self.fall = if prev == Phase::Air {
                self.fall.min(velocity.y)
            } else {
                velocity.y
            };
        }
        if next != prev {
            match (prev, next) {
                (Phase::Ground | Phase::Slide | Phase::Grind(_), Phase::Air) => {
                    self.cues.push(Cue::Pop)
                }
                (Phase::Air, Phase::Ground | Phase::Slide | Phase::Grind(_)) => {
                    self.cues.push(Cue::Land {
                        hard: self.fall < -HARD_LANDING,
                    })
                }
                (_, Phase::Wipeout) => self.cues.push(Cue::Bail),
                _ => {}
            }
        }
        self.phase = next;
    }

    pub(super) fn reset(&mut self) {
        self.phase = Phase::Off;
        self.cues.clear();
        self.fall = 0.0;
    }
}

/// `WOW_SKATE_AUDIO`, else a `skate-audio` beside or above the working directory.
fn audio_root() -> Option<PathBuf> {
    std::env::var_os("WOW_SKATE_AUDIO").map(PathBuf::from).or_else(|| {
        ["skate-audio", "../skate-audio"]
            .into_iter()
            .map(PathBuf::from)
            .find(|p| p.join("pop_1.wav").is_file())
    })
}

/// The decoded sounds, loaded on first ride.
struct Bank {
    roll_concrete: Option<StaticSoundData>,
    roll_asphalt: Option<StaticSoundData>,
    grind_trucks: Option<StaticSoundData>,
    grind_board: Option<StaticSoundData>,
    wind: Option<StaticSoundData>,
    pop: Vec<StaticSoundData>,
    land: Vec<StaticSoundData>,
    land_hard: Vec<StaticSoundData>,
    flip: Vec<StaticSoundData>,
    bail: Vec<StaticSoundData>,
    skid: Vec<StaticSoundData>,
}

impl Bank {
    fn load() -> Option<Self> {
        let root = audio_root()?;
        let one = |name: &str| StaticSoundData::from_file(root.join(format!("{name}.wav"))).ok();
        let set = |prefix: &str| -> Vec<StaticSoundData> {
            (1..)
                .map_while(|i| one(&format!("{prefix}_{i}")))
                .collect()
        };
        let bank = Self {
            roll_concrete: one("roll_concrete"),
            roll_asphalt: one("roll_asphalt"),
            grind_trucks: one("grind_trucks"),
            grind_board: one("grind_board"),
            wind: one("wind"),
            pop: set("pop"),
            land: set("land"),
            land_hard: set("land_hard"),
            flip: set("flip"),
            bail: set("bail"),
            skid: set("skid"),
        };
        info!(
            "skate sound: loaded from {} ({} pops, {} landings, {} skids)",
            root.display(),
            bank.pop.len(),
            bank.land.len() + bank.land_hard.len(),
            bank.skid.len()
        );
        Some(bank)
    }
}

/// The loops while riding, each started silent and faded to its level every frame.
#[derive(Default)]
struct Loops {
    roll_concrete: Option<StaticSoundHandle>,
    roll_asphalt: Option<StaticSoundHandle>,
    grind_trucks: Option<StaticSoundHandle>,
    grind_board: Option<StaticSoundHandle>,
    wind: Option<StaticSoundHandle>,
}

impl Loops {
    fn all(&mut self) -> [&mut Option<StaticSoundHandle>; 5] {
        [
            &mut self.roll_concrete,
            &mut self.roll_asphalt,
            &mut self.grind_trucks,
            &mut self.grind_board,
            &mut self.wind,
        ]
    }
}

#[derive(Default)]
pub(super) struct Player {
    bank: Option<Option<Bank>>,
    loops: Loops,
    riding: bool,
    /// When the next powerslide skid grain is due.
    next_skid: f32,
    rng: u32,
}

impl Player {
    fn pick<'a>(&mut self, set: &'a [StaticSoundData]) -> Option<&'a StaticSoundData> {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        set.get(self.rng as usize % set.len().max(1))
    }
}

fn tween(ms: u64) -> Tween {
    Tween {
        duration: Duration::from_millis(ms),
        ..Default::default()
    }
}

fn db(amp: f32) -> Decibels {
    if amp <= 0.0001 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * amp.log10())
    }
}

/// Whether the board rolls on terrain (asphalt) rather than a building or prop (concrete): the
/// terrain tiles are the ground-decal surfaces on the default layer.
fn on_terrain(
    collide: &WorldCollision,
    at: Vec3,
    surfaces: &Query<(Has<GroundDecalSurface>, Has<CollisionLayers>)>,
) -> bool {
    collide
        .ray_body(at + Vec3::Y * 0.5, Dir3::NEG_Y, 2.0)
        .and_then(|hit| surfaces.get(hit.entity).ok())
        .is_some_and(|(decal, layered)| decal && !layered)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn update(
    time: Res<Time>,
    drive: Res<SkateDrive>,
    mut audio: ResMut<SkateAudio>,
    config: Option<Res<crate::sound::SoundConfig>>,
    collide: WorldCollision,
    surfaces: Query<(Has<GroundDecalSurface>, Has<CollisionLayers>)>,
    output: Option<NonSendMut<crate::sound::SoundOutput>>,
    mut player: Local<Player>,
) {
    let cues = std::mem::take(&mut audio.cues);
    for cue in &cues {
        debug!("skate sound: {cue:?} in {:?}", audio.phase);
    }
    let Some(mut output) = output else {
        return;
    };
    let Some(mixer) = output.mixer.as_mut() else {
        return;
    };
    if !drive.active {
        if player.riding {
            player.riding = false;
            for handle in player.loops.all() {
                if let Some(mut h) = handle.take() {
                    h.stop(tween(250));
                }
            }
        }
        return;
    }
    if player.bank.is_none() {
        player.bank = Some(Bank::load());
        player.rng = 0x9e37_79b9;
    }
    let Some(Some(bank)) = player.bank.take() else {
        // No `skate-audio`: ride silent, without looking again.
        player.bank = Some(None);
        return;
    };
    let sfx = config.map_or(1.0, |c| if c.enabled { c.sfx } else { 0.0 });

    if !player.riding {
        player.riding = true;
        let start = |data: &Option<StaticSoundData>, mixer: &mut crate::sound::Mixer| {
            let data = data.clone()?.loop_region(..).volume(Decibels::SILENCE);
            mixer.play_2d(data).ok()
        };
        player.loops.roll_concrete = start(&bank.roll_concrete, mixer);
        player.loops.roll_asphalt = start(&bank.roll_asphalt, mixer);
        player.loops.grind_trucks = start(&bank.grind_trucks, mixer);
        player.loops.grind_board = start(&bank.grind_board, mixer);
        player.loops.wind = start(&bank.wind, mixer);
    }

    let phase = audio.phase;
    let speed = drive.speed * METERS_PER_YARD;
    let roll = if phase.rolling() {
        (speed / 6.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let terrain = phase.rolling() && on_terrain(&collide, drive.pos, &surfaces);
    let rate = 0.7 + (speed / 12.0).min(0.8) as f64;
    let grind = |board: bool| match phase {
        Phase::Grind(b) if b == board => (0.4 + speed / 8.0).min(1.0),
        _ => 0.0,
    };
    let wind = ((speed - 4.0) / 10.0).clamp(0.0, 0.6);
    let levels = [
        (roll * if terrain { 0.0 } else { 0.9 }, rate),
        (roll * if terrain { 0.9 } else { 0.0 }, rate),
        (grind(false), 0.85 + (speed / 20.0).min(0.4) as f64),
        (grind(true), 0.85 + (speed / 20.0).min(0.4) as f64),
        (wind, 1.0),
    ];
    for (handle, (level, rate)) in player.loops.all().into_iter().zip(levels) {
        if let Some(h) = handle.as_mut() {
            h.set_volume(db(level * sfx), tween(80));
            h.set_playback_rate(rate, tween(80));
        }
    }

    let now = time.elapsed_secs();
    let mut shots: Vec<(StaticSoundData, f32)> = Vec::new();
    for cue in cues {
        let (set, gain) = match cue {
            Cue::Pop => (&bank.pop, 0.9),
            Cue::Land { hard: true } => (&bank.land_hard, 1.0),
            Cue::Land { hard: false } => (&bank.land, 0.8),
            Cue::Bail => (&bank.bail, 1.0),
        };
        if let Some(s) = player.pick(set) {
            shots.push((s.clone(), gain));
        }
        if let Cue::Pop = cue {
            if let Some(s) = player.pick(&bank.flip) {
                shots.push((s.clone(), 0.35));
            }
        }
    }
    // A powerslide is a run of short skid grains.
    if phase == Phase::Slide && speed > 1.0 && now >= player.next_skid {
        if let Some(s) = player.pick(&bank.skid) {
            shots.push((s.clone(), (speed / 8.0).clamp(0.3, 0.8)));
        }
        player.next_skid = now + 0.12;
    }
    for (data, gain) in shots {
        let _ = mixer.play_2d(data.volume(db(gain * sfx)));
    }
    player.bank = Some(Some(bank));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_off_pops_and_a_fast_drop_lands_hard() {
        let mut a = SkateAudio::default();
        a.observe("PhysicsGround", Vec3::ZERO);
        a.observe("KnownAir", Vec3::new(3.0, 2.0, 0.0));
        a.observe("KnownAir", Vec3::new(3.0, -7.0, 0.0));
        a.observe("PhysicsGround", Vec3::ZERO);
        a.observe("WipeoutGround", Vec3::ZERO);
        let cues: Vec<String> = a.cues.iter().map(|c| format!("{c:?}")).collect();
        assert_eq!(cues, ["Pop", "Land { hard: true }", "Bail"]);
    }

    #[test]
    fn a_grind_is_left_with_a_pop_and_entered_with_a_landing() {
        let mut a = SkateAudio::default();
        a.observe("KnownAir", Vec3::new(0.0, -1.0, 0.0));
        a.observe("GrindFiftyFifty", Vec3::ZERO);
        assert_eq!(a.phase, Phase::Grind(false));
        a.observe("PhysicsAir", Vec3::ZERO);
        let cues: Vec<String> = a.cues.iter().map(|c| format!("{c:?}")).collect();
        assert_eq!(cues, ["Land { hard: false }", "Pop"]);
    }

    /// Decodes the converted sounds when `skate-audio/` is present (`tools/skate_audio.py`).
    #[test]
    fn the_converted_sounds_decode() {
        let Some(bank) = std::env::var_os("WOW_SKATE_AUDIO").and(Bank::load()) else {
            return;
        };
        assert!(bank.roll_concrete.is_some() && bank.wind.is_some());
        assert!(!bank.pop.is_empty() && !bank.land.is_empty() && !bank.skid.is_empty());
    }
}
