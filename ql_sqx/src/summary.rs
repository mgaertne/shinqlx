//! What a recording contains.

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::time::Duration;
use std::io::Read;

use super::{
    archive::{ArchiveError, ArchiveReader, Record},
    players::{Presence, Stay},
    writer::Event,
};

/// A stay and the part of the recording it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaySummary {
    /// Who stayed.
    pub stay: Stay,
    /// The frames the player is part of.
    pub frames: u64,
    /// The server time from the begin of the recording to the first of these frames.
    pub from: Duration,
    /// The server time from the begin of the recording to the last of these frames.
    pub until: Duration,
}

/// What a recording contains.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    /// Descriptions like the map, as key and value.
    pub meta: Vec<(String, String)>,
    /// The number of frames.
    pub frames: u64,
    /// The server time from the first to the last frame.
    pub length: Duration,
    /// Why the recording ended, `None` if it was cut off.
    pub end: Option<String>,
    /// Whether the file ends in the middle of a record.
    pub truncated: bool,
    /// Everyone who played, in the order of arrival.
    pub stays: Vec<StaySummary>,
}

/// Reads a whole recording.
pub fn summarize<R: Read>(mut recording: ArchiveReader<R>) -> Result<Summary, ArchiveError> {
    let mut summary = Summary::default();
    let mut configstrings = BTreeMap::new();
    let mut presence = Presence::default();
    let mut current: BTreeMap<u8, usize> = BTreeMap::new();
    let mut begin = None;
    while let Some(record) = recording.next_record()? {
        match record {
            Record::Start {
                meta,
                configstrings: initial,
            } => {
                summary.meta = meta;
                configstrings = initial.into_iter().collect();
            }
            Record::Event(Event::Configstring { index, value }) => {
                configstrings.insert(index, value);
            }
            Record::Event(Event::Frame(frame)) => {
                let begin = *begin.get_or_insert(frame.server_time);
                let since_begin = u64::try_from(frame.server_time.saturating_sub(begin))
                    .map_or(Duration::ZERO, Duration::from_millis);
                summary.frames += 1;
                summary.length = since_begin;
                for (client, _) in &frame.players {
                    if let Some(stay) = presence.arrive(*client, &configstrings) {
                        current.insert(*client, summary.stays.len());
                        summary.stays.push(StaySummary {
                            stay,
                            frames: 0,
                            from: since_begin,
                            until: since_begin,
                        });
                    }
                    if let Some(entry) = current
                        .get(client)
                        .and_then(|&index| summary.stays.get_mut(index))
                    {
                        entry.frames += 1;
                        entry.until = since_begin;
                    }
                }
            }
            Record::Event(Event::PlayerLeft { client }) => presence.leave(client),
            Record::Event(Event::End { reason }) => summary.end = Some(reason),
            Record::Event(
                Event::Command { .. }
                | Event::Stats { .. }
                | Event::Inputs(_)
                | Event::Shot(_)
                | Event::Damage(_),
            ) => {}
        }
    }
    summary.truncated = recording.truncated();
    Ok(summary)
}

#[cfg(test)]
mod summary_tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{archive::ArchiveWriter, capture::Frame, state::PlayerState};

    #[test]
    fn meta_data_and_the_reason_of_the_end_are_reported() {
        let meta = [("map".to_owned(), "aerowalk".to_owned())];
        let mut writer = ArchiveWriter::start(Vec::new(), &meta, &[]).expect("started");
        writer
            .event(&Event::End {
                reason: "the map ended".into(),
            })
            .expect("written");
        let mut recording = writer.finish().expect("finished").1;
        let summary = |recording: &[u8]| {
            summarize(ArchiveReader::new(recording).expect("a recording")).expect("readable")
        };

        let complete = summary(&recording);
        recording.pop();
        let cut_off = summary(&recording);

        assert_eq!(complete.meta, meta);
        assert_eq!(complete.end.as_deref(), Some("the map ended"));
        assert!(!complete.truncated);
        assert_eq!(cut_off.end, None);
        assert!(cut_off.truncated);
    }

    #[test]
    fn every_stay_of_a_client_is_listed() {
        let frame = |server_time, clients: &[u8]| {
            Event::Frame(Frame {
                server_time,
                entities: vec![],
                players: clients
                    .iter()
                    .map(|&client| (client, PlayerState::default()))
                    .collect(),
            })
        };
        let mut writer = ArchiveWriter::start(Vec::new(), &[], &[]).expect("started");
        for event in [
            frame(1000, &[0]),
            frame(2000, &[]),
            frame(3000, &[0]),
            Event::PlayerLeft { client: 0 },
            Event::Configstring {
                index: 529,
                value: b"n\\Next".to_vec(),
            },
            frame(9000, &[0]),
        ] {
            writer.event(&event).expect("written");
        }
        let recording = writer.finish().expect("finished").1;

        let summary = summarize(ArchiveReader::new(recording.as_slice()).expect("a recording"))
            .expect("readable");

        assert_eq!(summary.frames, 4);
        assert_eq!(summary.length, Duration::from_secs(8));
        assert_eq!(summary.end, None);
        let stays: Vec<_> = summary
            .stays
            .iter()
            .map(|entry| {
                (
                    entry.stay.number,
                    entry.stay.name.as_deref(),
                    entry.frames,
                    entry.from.as_secs(),
                    entry.until.as_secs(),
                )
            })
            .collect();
        assert_eq!(stays, [(1, None, 2, 0, 2), (2, Some("Next"), 1, 8, 8)]);
    }
}
