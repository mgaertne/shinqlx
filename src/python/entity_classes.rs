//! The entity classes that a map can have, with the names of their spawnflags and keys.
//!
//! The names were taken from the `entities.def` of the map editor. Only names are kept, no descriptions. A class
//! in the list is not necessarily spawned by the game: the compiler of the maps handles some of them.

/// Name, names of the spawnflag bits (`-` for a bit that is not used) and keys of an entity class.
pub(crate) type EntityClassInfo = (&'static str, &'static str, &'static [&'static str]);

/// Keys that the game reads for every entity, whatever its class.
pub(crate) const COMMON_KEYS: &[&str] = &[
    "origin",
    "angle",
    "angles",
    "spawnflags",
    "target",
    "targetname",
    "model",
    "model2",
    "team",
    "speed",
    "message",
    "wait",
    "random",
    "count",
    "health",
    "light",
    "dmg",
    "gametype",
    "not_gametype",
    "notfree",
    "notteam",
    "notsingle",
    "notbot",
    "nohumans",
    "nobots",
    "targetShaderName",
    "targetShaderNewName",
];

pub(crate) const ENTITY_CLASSES: &[EntityClassInfo] = &[
    (
        "advertisement",
        "",
        &["cellId", "model2", "origin", "light", "color", "targetname"],
    ),
    (
        "ammo_pack",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "ammo_bfg",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "ammo_belt",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "ammo_hmg",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "ammo_bullets",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "ammo_cells",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "ammo_grenades",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "ammo_lightning",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "ammo_nails",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "ammo_mines",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "ammo_rockets",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "ammo_shells",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "ammo_slugs",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "func_bobbing",
        "X_AXIS Y_AXIS",
        &[
            "speed",
            "height",
            "phase",
            "noise",
            "model2",
            "origin",
            "light",
            "color",
            "targetname",
        ],
    ),
    (
        "func_button",
        "",
        &[
            "angle",
            "target",
            "speed",
            "wait",
            "lip",
            "health",
            "light",
            "color",
            "model2",
            "origin",
            "targetname",
        ],
    ),
    (
        "func_door",
        "START_OPEN - CRUSHER STAY_OPEN SILVER_KEY GOLD_KEY",
        &[
            "angle",
            "speed",
            "wait",
            "lip",
            "targetname",
            "health",
            "dmg",
            "team",
            "light",
            "color",
            "model2",
            "origin",
            "startsound",
            "endsound",
        ],
    ),
    (
        "func_pendulum",
        "",
        &[
            "angle",
            "speed",
            "phase",
            "noise",
            "model2",
            "origin",
            "light",
            "color",
            "targetname",
        ],
    ),
    (
        "func_plat",
        "",
        &[
            "speed",
            "lip",
            "height",
            "dmg",
            "targetname",
            "light",
            "color",
            "model2",
            "origin",
        ],
    ),
    (
        "func_rotating",
        "- - X_AXIS Y_AXIS",
        &[
            "speed",
            "noise",
            "model2",
            "origin",
            "light",
            "color",
            "targetname",
        ],
    ),
    (
        "func_static",
        "",
        &[
            "model2",
            "origin",
            "light",
            "color",
            "targetname",
            "targetname",
        ],
    ),
    (
        "func_timer",
        "START_ON",
        &["wait", "random", "target", "targetname"],
    ),
    (
        "func_train",
        "",
        &[
            "speed",
            "target",
            "dmg",
            "model2",
            "origin",
            "light",
            "color",
            "targetname",
        ],
    ),
    (
        "holdable_invulnerability",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "holdable_kamikaze",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "holdable_medkit",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "holdable_teleporter",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    ("info_camp", "", &["range", "weight"]),
    (
        "info_tour_point",
        "INITIAL LOOK_AT_ME DROP_TO_FLOOR - GOTO_EXACT_ORIGIN SALUTE MUST_HAVE_CTF_FLAG HIDE_ITEM PLAYER_MUST_PICKUP_TARGETED_ITEM",
        &[
            "radius",
            "target",
            "message",
            "cvar",
            "cvarValue",
            "tourPointTarget",
            "tourPointTargetName",
            "printChatTextTime",
            "ignorePlayerLocation",
        ],
    ),
    ("info_notnull", "", &["targetname"]),
    ("info_null", "", &["targetname"]),
    (
        "info_player_deathmatch",
        "INITIAL",
        &["angle", "target", "targetname"],
    ),
    ("info_player_intermission", "", &["angles", "target"]),
    ("info_player_start", "", &["angle", "target"]),
    (
        "item_armor_body",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "item_armor_combat",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "item_armor_jacket",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "item_armor_shard",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    ("item_botroam", "SUSPENDED", &["weight"]),
    (
        "item_enviro",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "item_flight",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "item_haste",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "item_health",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "item_health_large",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "item_health_mega",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "item_health_small",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    ("item_key_silver", "SUSPENDED", &["target", "targetname"]),
    ("item_key_gold", "SUSPENDED", &["target", "targetname"]),
    ("item_key_master", "SUSPENDED", &["target", "targetname"]),
    (
        "item_invis",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "item_quad",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "item_regen",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "misc_portal_camera",
        "SLOWROTATE FASTROTATE NOWOBBLE",
        &["target", "targetname", "roll"],
    ),
    ("misc_portal_surface", "", &["target"]),
    ("misc_teleporter_dest", "", &["angle", "targetname"]),
    (
        "path_corner",
        "",
        &["target", "targetname", "speed", "wait"],
    ),
    (
        "item_scout",
        "SUSPENDED REDTEAM BLUETEAM",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "item_guard",
        "SUSPENDED REDTEAM BLUETEAM",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "item_doubler",
        "SUSPENDED REDTEAM BLUETEAM",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "item_ammoregen",
        "SUSPENDED REDTEAM BLUETEAM",
        &["wait", "random", "team", "target", "targetname"],
    ),
    ("race_point", "SUSPENDED", &[]),
    (
        "shooter_grenade",
        "",
        &["angles", "target", "targetname", "random"],
    ),
    (
        "shooter_plasma",
        "",
        &["angles", "target", "targetname", "random"],
    ),
    (
        "shooter_rocket",
        "",
        &["angles", "target", "targetname", "random"],
    ),
    (
        "target_delay",
        "",
        &["target", "targetname", "wait", "delay", "random"],
    ),
    ("target_give", "", &["target", "targetname"]),
    ("target_kill", "", &["targetname"]),
    ("target_location", "", &["message", "count"]),
    ("target_position", "", &["targetname"]),
    (
        "target_print",
        "REDTEAM BLUETEAM PRIVATE",
        &["message", "targetname"],
    ),
    (
        "target_push",
        "BOUNCEPAD",
        &["angles", "speed", "target", "targetname"],
    ),
    (
        "target_relay",
        "RED_ONLY BLUE_ONLY RANDOM",
        &["target", "targetname"],
    ),
    ("target_remove_powerups", "", &["targetname"]),
    ("target_score", "", &["targetname", "count"]),
    (
        "target_speaker",
        "LOOPED_ON LOOPED_OFF GLOBAL ACTIVATOR",
        &["noise", "wait", "random", "targetname"],
    ),
    ("target_teleporter", "", &["target", "targetname"]),
    ("target_cvar", "", &["targetname", "cvar", "cvarValue"]),
    ("team_dom_point", "", &["identifier", "count", "target"]),
    ("team_blueobelisk", "", &["angle"]),
    ("team_CTF_blueflag", "", &[]),
    ("team_CTF_blueplayer", "", &["target"]),
    ("team_CTF_bluespawn", "", &["target"]),
    ("team_CTF_neutralflag", "SUSPENDED", &["angle"]),
    ("team_CTF_redflag", "", &[]),
    ("team_CTF_redplayer", "", &["target"]),
    ("team_CTF_redspawn", "", &["target"]),
    ("team_neutralobelisk", "", &[]),
    ("team_redobelisk", "", &["angle"]),
    ("trigger_always", "", &["target"]),
    (
        "trigger_hurt",
        "START_OFF - SILENT NO_PROTECTION SLOW",
        &["dmg"],
    ),
    (
        "trigger_multiple",
        "",
        &["target", "targetname", "wait", "random"],
    ),
    ("trigger_push", "", &["target"]),
    ("trigger_teleport", "", &["target"]),
    (
        "weapon_bfg",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "weapon_chaingun",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "weapon_gauntlet",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "weapon_grapplinghook",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "weapon_grenadelauncher",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "weapon_lightning",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "weapon_machinegun",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "weapon_nailgun",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "weapon_plasmagun",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "weapon_prox_launcher",
        "SUSPENDED",
        &["wait", "random", "team", "target", "targetname"],
    ),
    (
        "weapon_railgun",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "weapon_rocketlauncher",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "weapon_shotgun",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
    (
        "weapon_hmg",
        "SUSPENDED",
        &["wait", "random", "count", "team", "target", "targetname"],
    ),
];

/// The class with this name, if the list has it.
pub(crate) fn find(name: &str) -> Option<&'static EntityClassInfo> {
    ENTITY_CLASSES
        .iter()
        .find(|(class_name, _, _)| *class_name == name)
}

/// Whether the game reads the key for entities of the class.
pub(crate) fn knows_key(class: &EntityClassInfo, key: &str) -> bool {
    COMMON_KEYS.contains(&key) || class.2.contains(&key)
}

/// The Python name of the class of the keys of an entity class: `func_door` becomes `FuncDoorKeys`.
#[cfg(test)]
fn keys_class_name(name: &str) -> String {
    let words = name
        .split('_')
        .map(|word| {
            let mut characters = word.chars();
            characters
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + characters.as_str())
                .unwrap_or_default()
        })
        .collect::<String>();
    format!("_{words}Keys")
}

#[cfg(test)]
fn common_key_type(key: &str) -> &'static str {
    match key {
        "spawnflags" => "int",
        _ => "_SpawnValue",
    }
}

/// The part of the type stubs that depends on the entity classes: the types of the keys and the overloads of
/// `spawn_entity`, one for every class.
#[cfg(test)]
pub(crate) fn stub_block() -> String {
    let mut stubs = String::from(
        "_SpawnValue: TypeAlias = str | int | float

class _CommonSpawnKeys(TypedDict, total=False):\n",
    );
    for key in COMMON_KEYS {
        stubs.push_str(&format!("    {key}: {}\n", common_key_type(key)));
    }

    for (name, _, keys) in ENTITY_CLASSES {
        let own_keys = keys
            .iter()
            .filter(|key| !COMMON_KEYS.contains(key))
            .collect::<alloc::collections::BTreeSet<_>>();
        if own_keys.is_empty() {
            continue;
        }
        stubs.push_str(&format!(
            "\nclass {}(_CommonSpawnKeys, total=False):\n",
            keys_class_name(name)
        ));
        for key in own_keys {
            stubs.push_str(&format!("    {key}: _SpawnValue\n"));
        }
    }

    for (index, (name, _, keys)) in ENTITY_CLASSES.iter().enumerate() {
        let separator = if index == 0 { "\n" } else { "" };
        let keys_type = if keys.iter().all(|key| COMMON_KEYS.contains(key)) {
            "_CommonSpawnKeys".to_string()
        } else {
            keys_class_name(name)
        };
        stubs.push_str(&format!(
            "{separator}@overload\ndef spawn_entity(\n    classname: Literal[\"{name}\"],\n    keys: {keys_type} | None = None,\n    *,\n    strict: bool = True,\n) -> EntityInfo | None: ...\n"
        ));
    }
    stubs.push_str(
        "@overload\ndef spawn_entity(\n    classname: str,\n    keys: dict[str, _SpawnValue] | None = None,\n    *,\n    strict: bool = True,\n) -> EntityInfo | None: ...\n\n",
    );
    stubs
}

#[cfg(test)]
mod entity_classes_tests {
    use pretty_assertions::assert_eq;

    use super::{ENTITY_CLASSES, find, keys_class_name, knows_key, stub_block};

    const STUB_FILE: &str = include_str!("../../python/shinqlx/_shinqlx.pyi");
    const BEGIN: &str = "# BEGIN spawn_entity (generated from entity_classes.rs, see its test)\n";
    const END: &str = "# END spawn_entity\n";

    #[test]
    fn the_stubs_have_the_types_of_the_entity_classes() {
        let begin = STUB_FILE.find(BEGIN).expect("begin marker is missing") + BEGIN.len();
        let end = STUB_FILE.find(END).expect("end marker is missing");

        assert_eq!(
            &STUB_FILE[begin..end],
            stub_block(),
            "run `cargo test print_entity_stubs -- --ignored --nocapture` and put the output between the markers"
        );
    }

    #[test]
    #[ignore = "prints the part of the stubs that is generated"]
    fn print_entity_stubs() {
        print!("{BEGIN}{}{END}", stub_block());
    }

    #[test]
    fn the_names_of_the_key_classes_are_unique() {
        let mut names = ENTITY_CLASSES
            .iter()
            .map(|(name, _, _)| keys_class_name(name))
            .collect::<Vec<_>>();
        names.sort();
        let count = names.len();
        names.dedup();

        assert_eq!(names.len(), count);
        assert_eq!(keys_class_name("team_CTF_blueflag"), "_TeamCTFBlueflagKeys");
    }

    #[test]
    fn keys_are_known_for_the_class_or_for_every_class() {
        let speaker = find("target_speaker").expect("target_speaker is a class");

        assert!(knows_key(speaker, "noise"));
        assert!(knows_key(speaker, "origin"));
        assert!(!knows_key(speaker, "lip"));
        assert_eq!(find("no_such_class"), None);
    }
}
