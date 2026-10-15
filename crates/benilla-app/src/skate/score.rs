//! The skate score on screen, Skate 3 style: the line in progress (latest trick, running score)
//! at the top of the screen, then the landed line's points with what it earned, the experience
//! (`SMSG_LOG_XPGAIN` without a victim) and the Skateboarding skill gained, or BAIL when the line
//! is lost.

use benilla_formats::skatecraft::SKILL_SKATEBOARDING;
use benilla_protocol::SessionEvent;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::net::{NetHandlerApp, ObjectStore, SelfPlayer};

/// How long a landed line stays up, then how long it fades (seconds).
const LANDED_HOLD: f32 = 2.5;
const LANDED_FADE: f32 = 1.0;
/// How long BAIL stays up (seconds).
const BAIL_HOLD: f32 = 1.5;
/// Experience arriving this long after a landing is the landing's (seconds).
const XP_WINDOW: f32 = 4.0;

const TRICK_COLOR: Color = Color::srgb(1.0, 1.0, 1.0);
const POINTS_COLOR: Color = Color::srgb(1.0, 0.82, 0.0);
const LANDED_COLOR: Color = Color::srgb(0.35, 1.0, 0.35);
const BAIL_COLOR: Color = Color::srgb(1.0, 0.25, 0.2);
/// The chat's experience colour (`CHAT_MSG_COMBAT_XP_GAIN`).
const XP_COLOR: Color = Color::srgb(111.0 / 255.0, 111.0 / 255.0, 1.0);
const SKILL_COLOR: Color = Color::srgb(0.4, 0.8, 1.0);

/// The score feed, written by the skate update and the experience handler.
#[derive(Resource, Default)]
pub(super) struct SkateScore {
    /// The line in progress: its highest points so far and latest trick.
    live: (f32, String),
    landed: Option<Landed>,
    /// When the last line was lost, and its points.
    bailed: Option<(f32, f32)>,
    /// The Skateboarding skill last seen, for the gain under a landing.
    skill: Option<u16>,
}

struct Landed {
    at: f32,
    points: f32,
    trick: String,
    xp: u32,
    skill_gain: u16,
}

impl SkateScore {
    /// One engine pose's line state at `now`; `landed` is a line landed since the last pose.
    /// Returns the landed line's points: the most the line showed in progress, since the engine
    /// banks only finished tricks, and its running score sags in the ticks before it banks.
    pub(super) fn observe(
        &mut self,
        now: f32,
        points: f32,
        trick: &str,
        landed: Option<f32>,
    ) -> Option<f32> {
        let (was, _) = self.live;
        let banked = landed.map(|p| p.max(was));
        if let Some(landed_points) = banked {
            self.landed = Some(Landed {
                at: now,
                points: landed_points,
                trick: self.live.1.clone(),
                xp: 0,
                skill_gain: 0,
            });
            self.bailed = None;
        } else if was > 0.0 && points <= 0.0 {
            self.bailed = Some((now, was));
        }
        // The peak carries on only within a line: a landing or a lost line starts over.
        let peak = if banked.is_none() && points > 0.0 {
            points.max(was)
        } else {
            points
        };
        self.live = (peak, trick_label(trick));
        banked
    }

    /// Off the board: nothing in progress.
    pub(super) fn clear_live(&mut self) {
        self.live = (0.0, String::new());
    }
}

pub(super) fn plugin(app: &mut App) {
    use benilla_protocol::SessionEventKind as K;
    app.init_resource::<SkateScore>()
        .net_handler(K::XpGain, on_xp_gain);
}

/// `SMSG_LOG_XPGAIN` without a victim shortly after a landing is the line's experience.
fn on_xp_gain(In(ev): In<SessionEvent>, time: Res<Time>, mut score: ResMut<SkateScore>) {
    let SessionEvent::XpGain(x) = ev else {
        return;
    };
    let now = time.elapsed_secs();
    if let Some(landed) = score.landed.as_mut() {
        if !x.kill && now - landed.at < XP_WINDOW {
            landed.xp += x.total;
        }
    }
}

#[derive(Component)]
pub(super) struct ScoreRoot;

#[derive(Component, Clone, Copy)]
pub(super) enum ScoreLine {
    Trick,
    Points,
    Xp,
    Skill,
}

pub(super) fn spawn(mut commands: Commands) {
    commands
        .spawn((
            ScoreRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(11.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
            GlobalZIndex(10),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            let text = |line| {
                (
                    line,
                    Text::new(String::new()),
                    TextFont::default(),
                    TextColor(Color::WHITE),
                    TextShadow::default(),
                    Pickable::IGNORE,
                )
            };
            root.spawn(text(ScoreLine::Trick));
            root.spawn(text(ScoreLine::Points));
            // Experience and skill side by side, each in its own colour.
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(24.0),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|row| {
                row.spawn(text(ScoreLine::Xp));
                row.spawn(text(ScoreLine::Skill));
            });
        });
}

/// What the overlay shows this frame: per line its text and colour, and the shared alpha.
struct Shown {
    trick: (String, Color),
    points: (String, Color),
    xp: String,
    skill: String,
    alpha: f32,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    time: Res<Time>,
    assets: Res<AssetServer>,
    windows: Query<&Window, With<PrimaryWindow>>,
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    mut score: ResMut<SkateScore>,
    mut root: Query<&mut Visibility, With<ScoreRoot>>,
    mut lines: Query<(&ScoreLine, &mut Text, &mut TextFont, &mut TextColor)>,
    mut font: Local<Option<Handle<Font>>>,
) {
    let now = time.elapsed_secs();
    // The skill's rise lands on the line it came from.
    let skill = self_q.single().ok().and_then(|store| {
        (0..benilla_protocol::messages::PLAYER_SKILL_SLOTS)
            .filter_map(|slot| store.0.player_skill(slot))
            .find(|s| u32::from(s.skill_id) == SKILL_SKATEBOARDING)
            .map(|s| s.value)
    });
    if let (Some(new), Some(old)) = (skill, score.skill) {
        if new > old {
            if let Some(landed) = score.landed.as_mut() {
                landed.skill_gain += new - old;
            }
        }
    }
    score.skill = skill.or(score.skill);

    let shown = shown(&score, now, skill);
    let Ok(mut visibility) = root.single_mut() else {
        return;
    };
    let Some(shown) = shown else {
        *visibility = Visibility::Hidden;
        return;
    };
    *visibility = Visibility::Visible;

    let height = windows.single().map(|w| w.height()).unwrap_or(768.0);
    let font = font.get_or_insert_with(|| crate::char_select::wow_font(&assets));
    for (line, mut text, mut text_font, mut color) in &mut lines {
        let (value, tint, size) = match line {
            ScoreLine::Trick => (&shown.trick.0, shown.trick.1, 0.032),
            ScoreLine::Points => (&shown.points.0, shown.points.1, 0.055),
            ScoreLine::Xp => (&shown.xp, XP_COLOR, 0.028),
            ScoreLine::Skill => (&shown.skill, SKILL_COLOR, 0.028),
        };
        if text_font.font != *font {
            text_font.font = font.clone();
        }
        text_font.font_size = (height * size).round();
        color.0 = tint.with_alpha(shown.alpha);
        if text.0 != *value {
            text.0.clone_from(value);
        }
    }
}

fn shown(score: &SkateScore, now: f32, skill: Option<u16>) -> Option<Shown> {
    let (live_points, live_trick) = &score.live;
    if *live_points > 0.0 {
        return Some(Shown {
            trick: (live_trick.clone(), TRICK_COLOR),
            points: (grouped(*live_points), POINTS_COLOR),
            xp: String::new(),
            skill: String::new(),
            alpha: 1.0,
        });
    }
    if let Some(landed) = &score.landed {
        let age = now - landed.at;
        if age < LANDED_HOLD + LANDED_FADE {
            let xp = match landed.xp {
                0 => String::new(),
                xp => format!("+{xp} XP"),
            };
            let skill = match landed.skill_gain {
                0 => String::new(),
                gain => {
                    let at = skill.map(|v| format!(" ({v})")).unwrap_or_default();
                    format!("+{gain} Skateboarding{at}")
                }
            };
            return Some(Shown {
                trick: (landed.trick.clone(), TRICK_COLOR),
                points: (format!("+{}", grouped(landed.points)), LANDED_COLOR),
                xp,
                skill,
                alpha: fade(age, LANDED_HOLD, LANDED_FADE),
            });
        }
    }
    if let Some((at, lost)) = score.bailed {
        let age = now - at;
        if age < BAIL_HOLD + LANDED_FADE {
            return Some(Shown {
                trick: ("BAIL".into(), BAIL_COLOR),
                points: (grouped(lost), BAIL_COLOR.with_alpha(0.6)),
                xp: String::new(),
                skill: String::new(),
                alpha: fade(age, BAIL_HOLD, LANDED_FADE),
            });
        }
    }
    None
}

/// Full alpha for `hold` seconds, then a linear fade over `fade`.
fn fade(age: f32, hold: f32, fade: f32) -> f32 {
    (1.0 - (age - hold).max(0.0) / fade).clamp(0.0, 1.0)
}

/// An engine trick label for the screen: `ID_TRICK_KICKFLIP` reads "Kickflip".
pub(super) fn trick_label(raw: &str) -> String {
    let words: Vec<&str> = raw.split(['_', ' ']).filter(|w| !w.is_empty()).collect();
    let skip = words
        .iter()
        .take_while(|w| w.eq_ignore_ascii_case("id") || w.eq_ignore_ascii_case("trick"))
        .count();
    words[skip..]
        .iter()
        .map(|w| {
            let lower = w.to_lowercase();
            let mut c = lower.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// `points` rounded, with thousands separators: 12,345.
fn grouped(points: f32) -> String {
    let digits = (points.max(0.0).round() as u64).to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trick_labels_drop_the_id_prefix() {
        assert_eq!(trick_label("ID_TRICK_KICKFLIP"), "Kickflip");
        assert_eq!(trick_label("id_trick_fs_50_50"), "Fs 50 50");
        assert_eq!(trick_label("Kickflip"), "Kickflip");
    }

    #[test]
    fn points_group_by_thousands() {
        assert_eq!(grouped(0.4), "0");
        assert_eq!(grouped(999.0), "999");
        assert_eq!(grouped(12345.0), "12,345");
        assert_eq!(grouped(1234567.0), "1,234,567");
    }

    #[test]
    fn a_landing_shows_then_fades_and_a_lost_line_shows_bail() {
        let mut score = SkateScore::default();
        score.observe(0.0, 250.0, "Kickflip", None);
        let live = shown(&score, 0.0, None).expect("live line");
        assert_eq!(live.points.0, "250");
        score.observe(1.0, 0.0, "", Some(300.0));
        let landed = shown(&score, 1.5, None).expect("landed");
        assert_eq!(landed.points.0, "+300");
        assert_eq!(landed.trick.0, "Kickflip");
        assert!(shown(&score, 1.0 + LANDED_HOLD + LANDED_FADE + 0.1, None).is_none());
        score.observe(10.0, 120.0, "Ollie", None);
        score.observe(10.5, 0.0, "", None);
        assert_eq!(shown(&score, 10.6, None).expect("bail").trick.0, "BAIL");
        assert_eq!(
            score.observe(20.0, 2000.0, "ID_TRICK_DARKSLIDE", None),
            None
        );
        assert_eq!(
            score.observe(20.05, 300.0, "ID_TRICK_DARKSLIDE", None),
            None
        );
        assert_eq!(shown(&score, 20.05, None).expect("live").points.0, "2,000");
        assert_eq!(score.observe(20.1, 0.0, "", Some(120.0)), Some(2000.0));
        assert_eq!(
            shown(&score, 20.2, None).expect("landed").trick.0,
            "Darkslide"
        );
    }
}
