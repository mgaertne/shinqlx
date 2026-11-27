/// One networked 32 bit field of an engine state, named like the field of the engine struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Field {
    pub(super) name: &'static str,
    pub(super) kind: FieldKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldKind {
    Float,
    Unsigned(u8),
    Signed(u8),
}

const fn float(name: &'static str) -> Field {
    Field {
        name,
        kind: FieldKind::Float,
    }
}

const fn uint(name: &'static str, bits: u8) -> Field {
    Field {
        name,
        kind: FieldKind::Unsigned(bits),
    }
}

const fn int(name: &'static str, bits: u8) -> Field {
    Field {
        name,
        kind: FieldKind::Signed(bits),
    }
}

/// The position of a field of an entity; a name without a field fails the build in constants.
pub(super) const fn entity_field(name: &str) -> usize {
    position(&ENTITY_FIELDS, name)
}

/// The position of a field of a player state; a name without a field fails the build in constants.
pub(super) const fn player_field(name: &str) -> usize {
    position(&PLAYER_FIELDS, name)
}

const fn position(table: &[Field], name: &str) -> usize {
    let mut index = 0;
    while index < table.len() {
        if same(table[index].name.as_bytes(), name.as_bytes()) {
            return index;
        }
        index += 1;
    }
    panic!("no field has this name");
}

const fn same(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// The number of networked fields of an entity.
pub const ENTITY_FIELD_COUNT: usize = 58;
/// The number of networked fields of a player state, without its arrays.
pub const PLAYER_FIELD_COUNT: usize = 58;

/// The order of the table `entityState_t` of protocol 91 in `qzeroded.x64`.
pub(super) const ENTITY_FIELDS: [Field; ENTITY_FIELD_COUNT] = [
    uint("pos.trTime", 32),
    float("pos.trBase[0]"),
    float("pos.trBase[1]"),
    float("pos.trDelta[0]"),
    float("pos.trDelta[1]"),
    float("pos.trBase[2]"),
    float("apos.trBase[1]"),
    float("pos.trDelta[2]"),
    float("apos.trBase[0]"),
    uint("pos.gravity", 32),
    uint("event", 10),
    float("angles2[1]"),
    uint("eType", 8),
    uint("torsoAnim", 8),
    uint("eventParm", 8),
    uint("legsAnim", 8),
    uint("groundEntityNum", 10),
    uint("pos.trType", 8),
    uint("eFlags", 19),
    uint("otherEntityNum", 10),
    uint("weapon", 8),
    uint("clientNum", 8),
    float("angles[1]"),
    uint("pos.trDuration", 32),
    uint("apos.trType", 8),
    float("origin[0]"),
    float("origin[1]"),
    float("origin[2]"),
    uint("solid", 24),
    uint("powerups", 16),
    uint("modelindex", 8),
    uint("otherEntityNum2", 10),
    uint("loopSound", 8),
    uint("generic1", 8),
    float("origin2[2]"),
    float("origin2[0]"),
    float("origin2[1]"),
    uint("modelindex2", 8),
    float("angles[0]"),
    uint("time", 32),
    uint("apos.trTime", 32),
    uint("apos.trDuration", 32),
    float("apos.trBase[2]"),
    float("apos.trDelta[0]"),
    float("apos.trDelta[1]"),
    float("apos.trDelta[2]"),
    uint("apos.gravity", 32),
    uint("time2", 32),
    float("angles[2]"),
    float("angles2[0]"),
    float("angles2[2]"),
    uint("constantLight", 32),
    uint("frame", 16),
    uint("jumpTime", 32),
    uint("doubleJumped", 1),
    uint("health", 16),
    uint("armor", 16),
    uint("location", 8),
];

/// The order of the table `playerState_t` of protocol 91 in `qzeroded.x64`.
pub(super) const PLAYER_FIELDS: [Field; PLAYER_FIELD_COUNT] = [
    uint("commandTime", 32),
    float("origin[0]"),
    float("origin[1]"),
    uint("bobCycle", 8),
    float("velocity[0]"),
    float("velocity[1]"),
    float("viewangles[1]"),
    float("viewangles[0]"),
    int("weaponTime", 16),
    float("origin[2]"),
    float("velocity[2]"),
    uint("legsTimer", 8),
    int("pm_time", 16),
    uint("eventSequence", 16),
    uint("torsoAnim", 8),
    uint("movementDir", 4),
    uint("events[0]", 8),
    uint("legsAnim", 8),
    uint("events[1]", 8),
    uint("pm_flags", 24),
    uint("groundEntityNum", 10),
    uint("weaponstate", 4),
    uint("eFlags", 16),
    uint("externalEvent", 10),
    uint("gravity", 16),
    uint("speed", 16),
    uint("delta_angles[1]", 16),
    uint("externalEventParm", 8),
    int("viewheight", 8),
    uint("damageEvent", 8),
    uint("damageYaw", 8),
    uint("damagePitch", 8),
    uint("damageCount", 8),
    uint("generic1", 8),
    uint("pm_type", 8),
    uint("delta_angles[0]", 16),
    uint("delta_angles[2]", 16),
    uint("torsoTimer", 12),
    uint("eventParms[0]", 8),
    uint("eventParms[1]", 8),
    uint("clientNum", 8),
    uint("weapon", 5),
    uint("weaponPrimary", 8),
    float("viewangles[2]"),
    float("grapplePoint[0]"),
    float("grapplePoint[1]"),
    float("grapplePoint[2]"),
    uint("jumppad_ent", 10),
    uint("loopSound", 16),
    uint("jumpTime", 32),
    uint("doubleJumped", 1),
    uint("crouchTime", 32),
    uint("crouchSlideTime", 32),
    uint("location", 8),
    uint("fov", 8),
    uint("forwardmove", 8),
    uint("rightmove", 8),
    uint("upmove", 8),
];
