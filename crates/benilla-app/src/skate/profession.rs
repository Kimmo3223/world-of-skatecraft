//! Skateboarding the profession: on a World of Skatecraft server the board is a spell. Casting
//! Summon Skateboard puts its aura on us and the aura is the ride: we are on the board exactly
//! while it lasts, and `J` casts or cancels it. The skate abilities are auras too; while one is on
//! us it is layered over the simulation ([`Boosts`]). Each landed line is reported to the server,
//! which raises the skill and gives experience. On a stock server (no Summon Skateboard in our
//! spellbook) `J` simply hops on and off, as before.

use benilla_formats::skatecraft::{
    SPELL_MOON_JUMP, SPELL_OLLIE_BOOST, SPELL_ROCKET_BOOST, SPELL_SPEED_DEMON,
    SPELL_SUMMON_SKATEBOARD,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use skate_host::bridge::Session;

use crate::net::{ClientCommand, NetCommands, ObjectStore, SelfPlayer};

/// Moon Jump's share of gravity while in the air.
const MOON_GRAVITY: f32 = 0.35;
/// Ollie Boost's extra pop, m/s up, added as the board leaves the ground.
const OLLIE_POP: f32 = 3.5;
/// Speed Demon's push along the direction of travel, m/s², up to its top speed (m/s).
const SPEED_PUSH: f32 = 6.0;
const SPEED_TOP: f32 = 20.0;
/// Speed Demon pushes only once rolling faster than this (m/s).
const SPEED_ROLLING: f32 = 1.0;
/// Rocket Boost's burst, m/s forward and up.
const ROCKET_FORWARD: f32 = 14.0;
const ROCKET_UP: f32 = 1.5;

/// The skate auras on us this frame.
#[derive(Clone, Copy, Default, Debug)]
pub(super) struct SkateAuras {
    pub(super) board: bool,
    pub(super) ollie: bool,
    pub(super) rocket: bool,
    pub(super) speed: bool,
    pub(super) moon: bool,
}

/// What the profession reads and sends from the skate update.
#[derive(SystemParam)]
pub(super) struct Profession<'w, 's> {
    self_q: Query<'w, 's, &'static ObjectStore, With<SelfPlayer>>,
    actions: Option<Res<'w, crate::ui_action::PlayerActions>>,
    commands: Option<Res<'w, NetCommands>>,
}

impl Profession<'_, '_> {
    /// Whether the board is a spell here: we know Summon Skateboard.
    pub(super) fn server_board(&self) -> bool {
        self.actions
            .as_ref()
            .is_some_and(|a| a.spells.contains(&SPELL_SUMMON_SKATEBOARD))
    }

    pub(super) fn auras(&self) -> SkateAuras {
        let mut auras = SkateAuras::default();
        let Ok(store) = self.self_q.single() else {
            return auras;
        };
        for slot in store.0.unit_auras() {
            match slot.spell_id {
                SPELL_SUMMON_SKATEBOARD => auras.board = true,
                SPELL_OLLIE_BOOST => auras.ollie = true,
                SPELL_ROCKET_BOOST => auras.rocket = true,
                SPELL_SPEED_DEMON => auras.speed = true,
                SPELL_MOON_JUMP => auras.moon = true,
                _ => {}
            }
        }
        auras
    }

    fn send(&self, command: ClientCommand) {
        if let Some(commands) = &self.commands {
            let _ = commands.0.send(command);
        }
    }

    pub(super) fn summon(&self) {
        self.send(ClientCommand::CastSpell {
            spell_id: SPELL_SUMMON_SKATEBOARD,
            target: None,
        });
    }

    pub(super) fn dismiss(&self) {
        self.send(ClientCommand::CancelAura {
            spell_id: SPELL_SUMMON_SKATEBOARD,
        });
    }

    /// Reports a landed line of `points` for the skill and experience.
    pub(super) fn landed(&self, points: f64, trick: &str) {
        info!("skate: landed {trick} for {points:.0} points");
        self.send(ClientCommand::AddonMessage {
            distribution: benilla_ui::script::AddonDistribution::Party,
            text: format!("SKATECRAFT\tL\t{points:.0}"),
        });
    }
}

/// The abilities in effect for one step of the simulation.
#[derive(Clone, Copy, Default, Debug)]
pub(super) struct Boosts {
    pub(super) ollie: bool,
    pub(super) speed: bool,
    pub(super) moon: bool,
    /// Fire Rocket Boost's burst once, at this step.
    pub(super) rocket: bool,
}

impl Boosts {
    pub(super) fn from_auras(auras: SkateAuras) -> Self {
        Self {
            ollie: auras.ollie,
            speed: auras.speed,
            moon: auras.moon,
            rocket: false,
        }
    }
}

/// The worker's memory between ticks.
pub(super) struct BoostState {
    airborne: bool,
    /// The gravity scale last set on the session.
    gravity: f32,
}

impl Default for BoostState {
    fn default() -> Self {
        Self {
            airborne: false,
            gravity: 1.0,
        }
    }
}

/// Layers `boosts` over one simulation tick of `period` seconds, after it ran.
pub(super) fn apply(session: &mut Session, boosts: &Boosts, state: &mut BoostState, period: f32) {
    let airborne = session.airborne();
    let took_off = airborne && !state.airborne;
    state.airborne = airborne;
    // Gravity for the next tick; stock again once Moon Jump ends or we land.
    let gravity = if boosts.moon && airborne {
        MOON_GRAVITY
    } else {
        1.0
    };
    if gravity != state.gravity {
        session.set_gravity_scale(gravity);
        state.gravity = gravity;
    }
    if !(boosts.ollie || boosts.speed) {
        return;
    }
    let velocity = Vec3::from_array(session.motion().0);
    if boosts.ollie && took_off && velocity.y > 0.0 {
        add(session, Vec3::Y * OLLIE_POP);
    }
    let flat = Vec3::new(velocity.x, 0.0, velocity.z);
    let speed = flat.length();
    if boosts.speed && !airborne && (SPEED_ROLLING..SPEED_TOP).contains(&speed) {
        add(session, flat / speed * SPEED_PUSH * period);
    }
}

fn add(session: &mut Session, dv: Vec3) {
    session.add_velocity(dv.to_array());
}

/// Rocket Boost's burst: forward along the roll, else the board's heading.
pub(super) fn rocket(session: &mut Session) {
    let (velocity, deck_forward) = session.motion();
    let flat = Vec3::new(velocity[0], 0.0, velocity[2]);
    let heading = if flat.length() > SPEED_ROLLING {
        flat
    } else {
        Vec3::new(deck_forward[0], 0.0, deck_forward[2])
    };
    let Some(forward) = heading.try_normalize() else {
        return;
    };
    add(session, forward * ROCKET_FORWARD + Vec3::Y * ROCKET_UP);
}
