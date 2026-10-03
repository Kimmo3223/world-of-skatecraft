//! World of Skatecraft's own client data: the Skateboarding skill line and the skate spells,
//! appended to `SkillLine.dbc`, `SkillRaceClassInfo.dbc` and `Spell.dbc` as [`Chain`] reads them,
//! so every reader (the skills pane, the spellbook, tooltips, the cast bar) sees them as stock
//! rows. The ids match the server's (`Skatecraft.h` in the vmangos patch).
//!
//! [`Chain`]: crate::Chain

/// The Skateboarding secondary skill.
pub const SKILL_SKATEBOARDING: u32 = 760;
/// The ride: an aura on us while we are on the board.
pub const SPELL_SUMMON_SKATEBOARD: u32 = 90001;
/// Higher pop while the aura lasts.
pub const SPELL_OLLIE_BOOST: u32 = 90002;
/// A one-second aura; its arrival is one forward burst.
pub const SPELL_ROCKET_BOOST: u32 = 90003;
/// More push on the ground while the aura lasts.
pub const SPELL_SPEED_DEMON: u32 = 90004;
/// Lighter gravity in the air while the aura lasts.
pub const SPELL_MOON_JUMP: u32 = 90005;

// SpellIcon.dbc
const ICON_MECHASTRIDER: u32 = 1240;
const ICON_BOOTS: u32 = 230;
const ICON_FLARE: u32 = 136;
const ICON_SPRINT: u32 = 516;
const ICON_MOONGLOW: u32 = 46;

/// SkillLineCategory 9, Secondary Skills.
const SKILL_CATEGORY_SECONDARY: u32 = 9;
/// SkillTiers 181: one tier up to 300.
const SKILL_TIER_300: u32 = 181;

const SPELL_EFFECT_APPLY_AURA: u32 = 6;
const SPELL_AURA_DUMMY: u32 = 4;
const TARGET_UNIT_CASTER: u32 = 1;

struct SkateSpell {
    id: u32,
    name: &'static str,
    description: &'static str,
    aura: &'static str,
    icon: u32,
    /// SpellCastTimes.dbc
    cast_time: u32,
    /// SpellDuration.dbc
    duration: u32,
    cooldown_ms: u32,
}

const SPELLS: [SkateSpell; 5] = [
    SkateSpell {
        id: SPELL_SUMMON_SKATEBOARD,
        name: "Summon Skateboard",
        description: "Hop on your skateboard. Land tricks to raise your Skateboarding skill and \
                      earn experience. Cancel the aura to step off.",
        aura: "Riding a skateboard.",
        icon: ICON_MECHASTRIDER,
        cast_time: 16,
        duration: 21,
        cooldown_ms: 0,
    },
    SkateSpell {
        id: SPELL_OLLIE_BOOST,
        name: "Ollie Boost",
        description: "Pop much higher off the ground for 30 sec. Requires your skateboard.",
        aura: "Ollies pop higher.",
        icon: ICON_BOOTS,
        cast_time: 1,
        duration: 2,
        cooldown_ms: 120_000,
    },
    SkateSpell {
        id: SPELL_ROCKET_BOOST,
        name: "Rocket Boost",
        description: "A burst of speed in the direction you are rolling. Requires your skateboard.",
        aura: "Rocketing.",
        icon: ICON_FLARE,
        cast_time: 1,
        duration: 36,
        cooldown_ms: 30_000,
    },
    SkateSpell {
        id: SPELL_SPEED_DEMON,
        name: "Speed Demon",
        description: "Push harder and roll faster for 30 sec. Requires your skateboard.",
        aura: "Rolling faster.",
        icon: ICON_SPRINT,
        cast_time: 1,
        duration: 2,
        cooldown_ms: 180_000,
    },
    SkateSpell {
        id: SPELL_MOON_JUMP,
        name: "Moon Jump",
        description: "Gravity loosens its grip on you for 20 sec. Requires your skateboard.",
        aura: "Floating through the air.",
        icon: ICON_MOONGLOW,
        cast_time: 1,
        duration: 18,
        cooldown_ms: 300_000,
    },
];

/// `bytes` as read for `name`, with our rows appended when `name` is one of the patched tables.
pub(crate) fn patch(name: &str, bytes: Vec<u8>) -> Vec<u8> {
    let key = name.replace('/', "\\").to_ascii_lowercase();
    let rows = match key.as_str() {
        "dbfilesclient\\skillline.dbc" => skill_line_rows(),
        "dbfilesclient\\skillraceclassinfo.dbc" => skill_race_class_rows(),
        "dbfilesclient\\spell.dbc" => spell_rows(),
        _ => return bytes,
    };
    append(&bytes, rows).unwrap_or(bytes)
}

/// One appended record: its fields, and the strings to place in string columns.
struct Row {
    fields: Vec<u32>,
    strings: Vec<(usize, &'static str)>,
}

fn skill_line_rows() -> Vec<Row> {
    // SkillLine: id, category, cost, name[8], name flags, description[8], description flags, icon.
    let mut fields = vec![0; 22];
    fields[0] = SKILL_SKATEBOARDING;
    fields[1] = SKILL_CATEGORY_SECONDARY;
    fields[21] = ICON_MECHASTRIDER;
    vec![Row {
        fields,
        strings: vec![
            (3, "Skateboarding"),
            (12, "Riding a skateboard and landing tricks."),
        ],
    }]
}

fn skill_race_class_rows() -> Vec<Row> {
    // SkillRaceClassInfo: id (filled in by `append`), skill, race mask, class mask, flags, min
    // level, tier, cost. Flag 0x80 files the skill's spells under General, as First Aid's.
    vec![Row {
        fields: vec![
            0,
            SKILL_SKATEBOARDING,
            511,
            1503,
            0x80,
            0,
            SKILL_TIER_300,
            0,
        ],
        strings: vec![],
    }]
}

fn spell_rows() -> Vec<Row> {
    SPELLS
        .iter()
        .map(|s| {
            // Spell.dbc 5875, 173 fields; columns as `spells::COL_*` and vmangos `SpellEntry`.
            let mut f = vec![0u32; 173];
            f[0] = s.id;
            f[18] = s.cast_time;
            f[19] = s.cooldown_ms;
            f[21] = if s.cast_time > 1 { 0x0F } else { 0 };
            f[28] = 1; // base level
            f[29] = 1; // spell level
            f[30] = s.duration;
            f[36] = 1; // range: self
            f[58] = u32::MAX; // equipped item class: none
            f[61] = SPELL_EFFECT_APPLY_AURA;
            f[64] = 1; // die sides
            f[82] = TARGET_UNIT_CASTER;
            f[91] = SPELL_AURA_DUMMY;
            f[97] = 1.0f32.to_bits(); // multiple value
            f[117] = s.icon;
            f[118] = s.icon;
            f[157] = 133; // the global cooldown category
            f[158] = 1500;
            f[167] = 1.0f32.to_bits(); // damage multiplier
            f[168] = 1.0f32.to_bits();
            f[169] = 1.0f32.to_bits();
            Row {
                fields: f,
                strings: vec![(120, s.name), (138, s.description), (147, s.aura)],
            }
        })
        .collect()
}

/// `bytes` (a WDBC file) with `rows` appended; a row whose id (field 0) is already present is
/// left out, and a zero id takes the next free one. `None` when `bytes` is not a table `rows`
/// fit.
fn append(bytes: &[u8], rows: Vec<Row>) -> Option<Vec<u8>> {
    let u32_at = |o: usize| Some(u32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?));
    if bytes.get(..4)? != b"WDBC" {
        return None;
    }
    let (count, field_count, record_size, string_size) = (
        u32_at(4)? as usize,
        u32_at(8)? as usize,
        u32_at(12)? as usize,
        u32_at(16)? as usize,
    );
    if record_size != field_count * 4 || rows.iter().any(|r| r.fields.len() != field_count) {
        return None;
    }
    let records = bytes.get(20..20 + count * record_size)?;
    let strings_start = 20 + count * record_size;
    let mut strings = bytes
        .get(strings_start..strings_start + string_size)?
        .to_vec();
    let ids: Vec<u32> = records
        .chunks_exact(record_size)
        .map(|r| u32::from_le_bytes(r[..4].try_into().unwrap()))
        .collect();
    let mut next_id = ids.iter().copied().max().unwrap_or(0) + 1;

    let mut out_records = records.to_vec();
    let mut added = 0;
    for mut row in rows {
        if row.fields[0] == 0 {
            row.fields[0] = next_id;
            next_id += 1;
        } else if ids.contains(&row.fields[0]) {
            continue;
        }
        for (col, text) in row.strings {
            row.fields[col] = strings.len() as u32;
            strings.extend_from_slice(text.as_bytes());
            strings.push(0);
        }
        out_records.extend(row.fields.iter().flat_map(|v| v.to_le_bytes()));
        added += 1;
    }

    let mut out = Vec::with_capacity(20 + out_records.len() + strings.len());
    out.extend_from_slice(b"WDBC");
    for v in [count + added, field_count, record_size, strings.len()] {
        out.extend_from_slice(&(v as u32).to_le_bytes());
    }
    out.extend_from_slice(&out_records);
    out.extend_from_slice(&strings);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(fields: usize, ids: &[u32]) -> Vec<u8> {
        let mut out = b"WDBC".to_vec();
        for v in [ids.len(), fields, fields * 4, 1] {
            out.extend_from_slice(&(v as u32).to_le_bytes());
        }
        for &id in ids {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend(std::iter::repeat_n(0u8, (fields - 1) * 4));
        }
        out.push(0);
        out
    }

    #[test]
    fn appends_rows_and_their_strings() {
        let out = patch("DBFilesClient/SkillLine.dbc", table(22, &[129, 762]));
        assert_eq!(u32::from_le_bytes(out[4..8].try_into().unwrap()), 3);
        let row = &out[20 + 2 * 88..20 + 3 * 88];
        assert_eq!(
            u32::from_le_bytes(row[..4].try_into().unwrap()),
            SKILL_SKATEBOARDING
        );
        let name_at = u32::from_le_bytes(row[12..16].try_into().unwrap()) as usize;
        let strings = &out[20 + 3 * 88..];
        assert!(strings[name_at..].starts_with(b"Skateboarding\0"));
    }

    #[test]
    fn a_present_id_is_not_added_twice_and_zero_ids_take_the_next_free() {
        let out = patch(
            "DBFilesClient\\SkillLine.dbc",
            table(22, &[SKILL_SKATEBOARDING]),
        );
        assert_eq!(u32::from_le_bytes(out[4..8].try_into().unwrap()), 1);
        let out = patch("DBFilesClient\\SkillRaceClassInfo.dbc", table(8, &[5, 9]));
        let row = &out[20 + 2 * 32..20 + 3 * 32];
        assert_eq!(u32::from_le_bytes(row[..4].try_into().unwrap()), 10);
    }

    #[test]
    fn the_client_data_reads_our_skill_and_spells() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let lines = crate::load_skill_line_catalog(&mut chain).expect("skill lines");
        let line = lines.line(SKILL_SKATEBOARDING).expect("Skateboarding");
        assert_eq!(line.name, "Skateboarding");
        assert_eq!(line.category_id, SKILL_CATEGORY_SECONDARY);
        assert!(lines.race_class(SKILL_SKATEBOARDING, 1, 1).is_some());
        let spells = crate::load_spell_catalog(&mut chain).expect("spells");
        for s in &SPELLS {
            let spell = spells.get(s.id).expect("skate spell");
            assert_eq!(spell.name, s.name);
            assert!(spell.icon.is_some(), "{} has an icon", s.name);
        }
    }

    #[test]
    fn other_files_and_odd_layouts_pass_through() {
        let other = table(22, &[1]);
        assert_eq!(patch("DBFilesClient\\Map.dbc", other.clone()), other);
        let narrow = table(4, &[1]);
        assert_eq!(patch("DBFilesClient\\Spell.dbc", narrow.clone()), narrow);
    }
}
