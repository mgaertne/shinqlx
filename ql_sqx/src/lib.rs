//! The format of the match recordings of shinqlx (`.dm_sqx`) and the demo files (`.dm_91`) made from
//! them, without Python or engine types.

extern crate alloc;

mod archive;
mod bits;
mod capture;
mod delta;
mod demo_files;
mod error;
mod export;
mod fields;
#[allow(dead_code, reason = "reading demos is only used by the tests so far")]
mod file;
mod frame_coding;
mod huffman;
#[allow(dead_code, reason = "reading demos is only used by the tests so far")]
mod message;
mod player_coding;
mod players;
mod prediction;
mod recorder;
mod state;
mod summary;
mod writer;

pub use archive::{ArchiveError, ArchiveReader, ArchiveWriter, Record};
pub use capture::{CapturedEntity, Frame, Visibility};
pub use demo_files::{DemoFile, DemoFileError, DemoFiles, demo_file_name, write_demo_files};
pub use error::DemoError;
pub use export::{Demo, ExportError, PrivateMessages, export_dm91};
pub use fields::{ENTITY_FIELD_COUNT, PLAYER_FIELD_COUNT};
pub use players::{Stay, without_colors};
pub use recorder::{PovRecorder, RecordingError, RecordingStats};
pub use state::{ARRAY_LENGTH, EntityState, PlayerState, float_bits};
pub use summary::{StaySummary, Summary, summarize};
pub use writer::{ArchiveStats, DemoWriter, Event};
