//! Keeps the team scoreboards under the size the engine sends.
//!
//! `SV_SendServerCommand` throws away a command of 1023 bytes or more without a message, and the game then
//! falls back to a stub that shows no players. The trim cuts a scoreboard back in steps, only as far as needed,
//! so that a match with many players still shows the best players of each team.

use core::sync::atomic::{AtomicU64, Ordering};

/// The cvar that switches the trim on or off.
pub(crate) const TRIM_CVAR: &str = "qlx_scoreboardTrim";
/// The cvar for the size in bytes from which a scoreboard is cut back.
pub(crate) const BUDGET_CVAR: &str = "qlx_scoreboardBudget";
/// The cvar for the ranks per team that keep all their values.
pub(crate) const FULL_CVAR: &str = "qlx_scoreboardFullPerTeam";
/// The cvar for the ranks per team that stay in the scoreboard, with their values set to zero.
pub(crate) const LIGHT_CVAR: &str = "qlx_scoreboardLightPerTeam";
/// The cvar for the ranks per team that stay even if the scoreboard is still too long.
pub(crate) const MIN_CVAR: &str = "qlx_scoreboardMinPerTeam";

/// The scoreboard commands that are known: name, fields before the rows, fields per row.
///
/// The first field of the header is the number of rows. A row starts with the client number and the team
/// (1 red, 2 blue, 3 spectator).
const LAYOUTS: [(&str, usize, usize); 1] = [("scores_ca", 3, 16)];

/// The values of the cvars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrimSettings {
    pub(crate) enabled: bool,
    pub(crate) budget: usize,
    pub(crate) full_per_team: usize,
    pub(crate) light_per_team: usize,
    pub(crate) min_per_team: usize,
}

impl TrimSettings {
    pub(crate) const DEFAULT: Self = Self {
        enabled: true,
        budget: 1000,
        full_per_team: 8,
        light_per_team: 12,
        min_per_team: 4,
    };

    /// Takes the cvar values and keeps them in a sensible range.
    pub(crate) fn from_values(
        enabled: i32,
        budget: i32,
        full_per_team: i32,
        light_per_team: i32,
        min_per_team: i32,
    ) -> Self {
        let clamp = |value: i32, low: i32, high: i32| {
            usize::try_from(value.clamp(low, high)).unwrap_or(low as usize)
        };
        let full_per_team = clamp(full_per_team, 1, 32);
        Self {
            enabled: enabled != 0,
            budget: clamp(budget, 200, 1022),
            full_per_team,
            light_per_team: clamp(light_per_team, full_per_team as i32, 64),
            min_per_team: clamp(min_per_team, 1, 16),
        }
    }
}

/// How far a scoreboard was cut back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tier {
    SpectatorsBlanked,
    TeamsBlanked,
    TeamsCutToLight,
    SpectatorsDropped,
    TeamsCutToMinimum,
}

impl Tier {
    const ALL: [Self; 5] = [
        Self::SpectatorsBlanked,
        Self::TeamsBlanked,
        Self::TeamsCutToLight,
        Self::SpectatorsDropped,
        Self::TeamsCutToMinimum,
    ];

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|tier| *tier == self)
            .unwrap_or_default()
    }

    /// A short description for the console.
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Self::SpectatorsBlanked => "values of spectators set to zero",
            Self::TeamsBlanked => "values of ranks beyond qlx_scoreboardFullPerTeam set to zero",
            Self::TeamsCutToLight => "ranks beyond qlx_scoreboardLightPerTeam dropped",
            Self::SpectatorsDropped => "spectators dropped",
            Self::TeamsCutToMinimum => "ranks dropped down to qlx_scoreboardMinPerTeam",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Team {
    Playing,
    Spectator,
    Other,
}

struct Row<'a> {
    fields: Vec<&'a str>,
    team: Team,
    team_number: &'a str,
    rank: usize,
    own: bool,
}

impl Row<'_> {
    fn blank(&mut self) {
        let last = self.fields.len().saturating_sub(1);
        self.fields
            .iter_mut()
            .take(last)
            .skip(2)
            .for_each(|field| *field = "0");
    }
}

fn render(name: &str, head: &[&str], rows: &[Row<'_>]) -> String {
    let count = rows.len().to_string();
    core::iter::once(name)
        .chain(core::iter::once(count.as_str()))
        .chain(head.iter().skip(1).copied())
        .chain(rows.iter().flat_map(|row| row.fields.iter().copied()))
        .collect::<Vec<&str>>()
        .join(" ")
}

/// The counters of the trim.
#[derive(Debug, Default)]
pub(crate) struct TrimCounters {
    cut: [AtomicU64; 5],
    longest: AtomicU64,
}

impl TrimCounters {
    pub(crate) const fn new() -> Self {
        Self {
            cut: [const { AtomicU64::new(0) }; 5],
            longest: AtomicU64::new(0),
        }
    }

    /// How often each tier was the last one that was needed, and the longest scoreboard seen.
    pub(crate) fn snapshot(&self) -> ([u64; 5], u64) {
        (
            core::array::from_fn(|i| self.cut[i].load(Ordering::Relaxed)),
            self.longest.load(Ordering::Relaxed),
        )
    }

    pub(crate) fn reset(&self) {
        self.cut
            .iter()
            .chain(core::iter::once(&self.longest))
            .for_each(|counter| counter.store(0, Ordering::Relaxed));
    }
}

/// The counters of the running server.
pub(crate) static TRIM_COUNTERS: TrimCounters = TrimCounters::new();

pub(crate) fn tier_names() -> impl Iterator<Item = &'static str> {
    Tier::ALL.into_iter().map(Tier::describe)
}

/// Cuts a scoreboard command back until it fits the budget, `None` if it fits already or is of an unknown format.
///
/// `client_id` is the client the command is for; its own row always stays.
pub(crate) fn trim(command: &str, client_id: i32, settings: TrimSettings) -> Option<String> {
    if !settings.enabled {
        return None;
    }
    TRIM_COUNTERS
        .longest
        .fetch_max(command.len() as u64, Ordering::Relaxed);
    if command.len() <= settings.budget {
        return None;
    }

    let mut words = command.split(' ');
    let name = words.next()?;
    let (_, header, row_width) = LAYOUTS.iter().find(|(known, ..)| *known == name)?;
    let rest: Vec<&str> = words.collect();
    let count: usize = rest.first()?.parse().ok()?;
    if rest.len() != header + count * row_width {
        return None;
    }

    let (head, body) = rest.split_at(*header);
    let own_number = client_id.to_string();
    let mut ranks = [0_usize; 2];
    let mut spectator_rank = 0;
    let mut rows: Vec<Row<'_>> = body
        .chunks(*row_width)
        .map(|fields| {
            let team_number = fields[1];
            let (team, rank) = match team_number {
                "1" | "2" => {
                    let slot = usize::from(team_number == "2");
                    ranks[slot] += 1;
                    (Team::Playing, ranks[slot] - 1)
                }
                "3" => {
                    spectator_rank += 1;
                    (Team::Spectator, spectator_rank - 1)
                }
                _ => (Team::Other, 0),
            };
            Row {
                fields: fields.to_vec(),
                team,
                team_number,
                rank,
                own: fields[0] == own_number,
            }
        })
        .collect();

    let fits = |rows: &[Row<'_>]| render(name, head, rows).len() <= settings.budget;
    let finish = |rows: &[Row<'_>], tier: Tier| {
        TRIM_COUNTERS.cut[tier.index()].fetch_add(1, Ordering::Relaxed);
        Some(render(name, head, rows))
    };

    rows.iter_mut()
        .filter(|row| row.team == Team::Spectator && !row.own)
        .for_each(Row::blank);
    if fits(&rows) {
        return finish(&rows, Tier::SpectatorsBlanked);
    }

    rows.iter_mut()
        .filter(|row| row.team == Team::Playing && row.rank >= settings.full_per_team && !row.own)
        .for_each(Row::blank);
    if fits(&rows) {
        return finish(&rows, Tier::TeamsBlanked);
    }

    let keep = settings.light_per_team.max(settings.min_per_team);
    rows.retain(|row| row.own || row.team != Team::Playing || row.rank < keep);
    if fits(&rows) {
        return finish(&rows, Tier::TeamsCutToLight);
    }

    rows.retain(|row| row.own || row.team != Team::Spectator);
    if fits(&rows) {
        return finish(&rows, Tier::SpectatorsDropped);
    }

    while !fits(&rows) {
        let droppable = ["1", "2"]
            .into_iter()
            .map(|number| {
                rows.iter()
                    .enumerate()
                    .filter(|(_, row)| row.team == Team::Playing && row.team_number == number)
                    .collect::<Vec<_>>()
            })
            .filter(|team| team.len() > settings.min_per_team)
            .filter_map(|team| {
                team.iter()
                    .rev()
                    .find(|(_, row)| !row.own)
                    .map(|(index, _)| (team.len(), *index))
            })
            .max_by_key(|(len, _)| *len);
        let Some((_, index)) = droppable else {
            break;
        };
        rows.remove(index);
    }
    finish(&rows, Tier::TeamsCutToMinimum)
}

#[cfg(test)]
mod scoreboard_trim_tests {
    use pretty_assertions::assert_eq;

    use super::{TRIM_COUNTERS, Tier, TrimSettings, trim};
    use crate::prelude::serial;

    fn row(client: u32, team: u32) -> String {
        format!("{client} {team} 0 0 0 1 1 87 3 49 471 0 0 0 0 1")
    }

    fn scoreboard(teams: &[u32]) -> String {
        let rows: Vec<String> = teams
            .iter()
            .enumerate()
            .map(|(client, team)| row(client as u32, *team))
            .collect();
        format!("scores_ca {} 4 7 {}", teams.len(), rows.join(" "))
    }

    fn rows_of(command: &str) -> Vec<Vec<String>> {
        command
            .split(' ')
            .skip(4)
            .map(str::to_owned)
            .collect::<Vec<_>>()
            .chunks(16)
            .map(<[String]>::to_vec)
            .collect()
    }

    fn two_teams(per_team: usize) -> Vec<u32> {
        (0..per_team * 2).map(|i| 1 + (i as u32 % 2)).collect()
    }

    const TIGHT: TrimSettings = TrimSettings {
        enabled: true,
        budget: 500,
        full_per_team: 2,
        light_per_team: 3,
        min_per_team: 1,
    };

    #[test]
    fn a_scoreboard_within_the_budget_is_not_touched() {
        assert_eq!(
            trim(&scoreboard(&two_teams(4)), 0, TrimSettings::DEFAULT),
            None
        );
    }

    #[test]
    fn a_disabled_trim_does_nothing() {
        let settings = TrimSettings {
            enabled: false,
            ..TIGHT
        };

        assert_eq!(trim(&scoreboard(&two_teams(10)), 0, settings), None);
    }

    #[test]
    fn unknown_commands_and_broken_scoreboards_are_not_touched() {
        let long = format!("print \"{}\"", "x".repeat(2000));
        let broken = format!("{} 7", scoreboard(&two_teams(10)));

        assert_eq!(trim(&long, 0, TIGHT), None);
        assert_eq!(trim(&broken, 0, TIGHT), None);
        assert_eq!(trim("scores_ca x", 0, TIGHT), None);
    }

    #[test]
    #[serial]
    fn spectators_are_blanked_first() {
        let mut teams = two_teams(3);
        teams.extend([3, 3]);
        let command = scoreboard(&teams);
        let settings = TrimSettings {
            budget: command.len() - 8,
            ..TIGHT
        };

        let trimmed = trim(&command, 0, settings).expect("the scoreboard should be cut");

        let rows = rows_of(&trimmed);
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[6][2..15], ["0"; 13]);
        assert_eq!(rows[0][7], "87");
    }

    #[test]
    #[serial]
    fn team_ranks_beyond_the_limits_are_blanked_then_cut() {
        let command = scoreboard(&two_teams(8));
        let blanked = trim(
            &command,
            0,
            TrimSettings {
                budget: command.len() - 40,
                ..TIGHT
            },
        )
        .expect("the scoreboard should be cut");
        assert_eq!(rows_of(&blanked).len(), 16);
        assert_eq!(rows_of(&blanked)[15][7], "0");
        assert_eq!(rows_of(&blanked)[3][7], "87");

        let cut = trim(
            &command,
            0,
            TrimSettings {
                budget: 400,
                ..TIGHT
            },
        )
        .expect("the scoreboard should be cut");
        let rows = rows_of(&cut);
        assert!(cut.starts_with(&format!("scores_ca {} 4 7 ", rows.len())));
        assert_eq!(rows.iter().filter(|row| row[1] == "1").count(), 3);
    }

    #[test]
    #[serial]
    fn the_own_row_always_stays() {
        let command = scoreboard(&two_teams(10));
        let settings = TrimSettings {
            budget: 300,
            min_per_team: 1,
            ..TIGHT
        };

        let trimmed = trim(&command, 19, settings).expect("the scoreboard should be cut");

        let rows = rows_of(&trimmed);
        assert!(rows.iter().any(|row| row[0] == "19" && row[7] == "87"));
        assert!(trimmed.len() < command.len());
    }

    #[test]
    #[serial]
    fn the_last_tier_cuts_the_larger_team_first() {
        let command = scoreboard(&[1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2]);
        let settings = TrimSettings {
            budget: 170,
            full_per_team: 1,
            light_per_team: 3,
            min_per_team: 2,
            ..TIGHT
        };
        TRIM_COUNTERS.reset();

        let trimmed = trim(&command, 0, settings).expect("the scoreboard should be cut");

        let rows = rows_of(&trimmed);
        assert_eq!(rows.iter().filter(|row| row[1] == "1").count(), 2);
        assert_eq!(rows.iter().filter(|row| row[1] == "2").count(), 2);
        let (cut, longest) = TRIM_COUNTERS.snapshot();
        assert_eq!(cut[Tier::TeamsCutToMinimum as usize], 1);
        assert_eq!(longest, command.len() as u64);
    }

    #[test]
    fn settings_are_clamped() {
        let settings = TrimSettings::from_values(1, 99_999, 0, 0, 99);

        assert_eq!(settings.budget, 1022);
        assert_eq!(settings.full_per_team, 1);
        assert_eq!(settings.light_per_team, 1);
        assert_eq!(settings.min_per_team, 16);
        assert!(TrimSettings::from_values(0, 0, 8, 4, 4).budget == 200);
        assert!(!TrimSettings::from_values(0, 1000, 8, 12, 4).enabled);
    }
}
