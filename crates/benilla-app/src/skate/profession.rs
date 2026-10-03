//! Skateboarding the profession: on a World of Skatecraft server the board is a spell. Casting
//! Summon Skateboard puts its aura on us and the aura is the ride: we are on the board exactly
//! while it lasts, and `J` casts or cancels it. The skate abilities are auras too, cast from the
//! D-pad or the action bar; while one is on us it tunes the simulation ([`Boosts`]). Each landed
//! line is reported to the server, which raises the skill and gives experience. On a stock server
//! (no Summon Skateboard in our spellbook) `J` simply hops on and off, as before.

use benilla_formats::skatecraft::{
    SPELL_MOON_JUMP, SPELL_OLLIE_BOOST, SPELL_ROCKET_BOOST, SPELL_SPEED_DEMON,
    SPELL_SUMMON_SKATEBOARD,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use skate_host::bridge::Session;

use crate::net::{ClientCommand, NetCommands, ObjectStore, SelfPlayer};

/// Moon Jump's share of gravity while the buff lasts.
const MOON_GRAVITY: f32 = 0.35;
/// Ollie Boost's multiplier on the engine's pop (ollies and grind pops).
const OLLIE_POP: f32 = 1.8;
/// Speed Demon's multiplier on the engine's push speed and power.
const SPEED_PUSH: f32 = 1.6;
/// Below this (m/s) Rocket Boost fires along the board rather than the roll.
const ROCKET_ROLLING: f32 = 1.0;
/// Rocket Boost's burst, m/s forward and up.
const ROCKET_FORWARD: f32 = 14.0;
const ROCKET_UP: f32 = 1.5;

/// XInput D-pad bits and the abilities they cast. With LB held the D-pad is the engine's
/// session markers instead.
const DPAD: [(u16, u32); 4] = [
    (0x0001, SPELL_OLLIE_BOOST),
    (0x0008, SPELL_ROCKET_BOOST),
    (0x0004, SPELL_SPEED_DEMON),
    (0x0002, SPELL_MOON_JUMP),
];
/// XInput's left bumper.
pub(super) const PAD_LB: u16 = 0x0100;

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

    /// Casts the skate ability bound to a D-pad direction in `pressed` (XInput bits), if known.
    pub(super) fn dpad(&self, pressed: u16) {
        for (bit, spell_id) in DPAD {
            let known = self
                .actions
                .as_ref()
                .is_some_and(|a| a.spells.contains(&spell_id));
            if pressed & bit != 0 && known {
                self.send(ClientCommand::CastSpell {
                    spell_id,
                    target: None,
                });
            }
        }
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

/// The worker's memory between steps: the pop, push and gravity scales last set.
pub(super) struct BoostState {
    tuning: (f32, f32, f32),
}

impl Default for BoostState {
    fn default() -> Self {
        Self {
            tuning: (1.0, 1.0, 1.0),
        }
    }
}

pub(super) fn apply(session: &mut Session, boosts: &Boosts, state: &mut BoostState) {
    // The engine plans each jump at takeoff, so the boosts tune it rather than push it.
    let tuning = (
        if boosts.ollie { OLLIE_POP } else { 1.0 },
        if boosts.speed { SPEED_PUSH } else { 1.0 },
        if boosts.moon { MOON_GRAVITY } else { 1.0 },
    );
    if tuning != state.tuning {
        session.set_boost_tuning(tuning.0, tuning.1);
        session.set_gravity_scale(tuning.2);
        state.tuning = tuning;
    }
}

fn add(session: &mut Session, dv: Vec3) {
    session.add_velocity(dv.to_array());
}

/// Rocket Boost's burst: forward along the roll, else the board's heading.
pub(super) fn rocket(session: &mut Session) {
    let (velocity, deck_forward) = session.motion();
    let flat = Vec3::new(velocity[0], 0.0, velocity[2]);
    let heading = if flat.length() > ROCKET_ROLLING {
        flat
    } else {
        Vec3::new(deck_forward[0], 0.0, deck_forward[2])
    };
    let Some(forward) = heading.try_normalize() else {
        return;
    };
    add(session, forward * ROCKET_FORWARD + Vec3::Y * ROCKET_UP);
}
