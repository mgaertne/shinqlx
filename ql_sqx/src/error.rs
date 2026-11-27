use derive_more::Display;

/// Why bytes cannot be turned into a demo or back.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
pub enum DemoError {
    /// There is less data than expected.
    #[display("the data ends in the middle of a value")]
    UnexpectedEnd,
    /// The data is no Huffman code.
    #[display("invalid Huffman code")]
    InvalidCode,
    /// A message contains something unknown.
    #[display("unknown server operation {_0}")]
    UnknownOperation(u8),
    /// A state has fewer fields than reported as changed.
    #[display("invalid number of changed fields: {_0}")]
    FieldCount(u8),
    /// An entity number is out of range or out of order.
    #[display("invalid entity number: {_0}")]
    EntityNumber(u16),
    /// A frame has more players than a server holds.
    #[display("too many players in a frame")]
    PlayerCount,
    /// The area mask is longer than a client accepts.
    #[display("the area mask of {_0} bytes is too long")]
    AreaMaskLength(usize),
    /// A string is longer than a client accepts.
    #[display("a string of {_0} bytes is too long")]
    StringTooLong(usize),
    /// A block of a demo or a record has an impossible length.
    #[display("invalid block length: {_0}")]
    BlockLength(i32),
    /// A snapshot refers to one that was not read.
    #[display("the snapshot to delta from, {_0} messages back, is not available")]
    MissingDeltaBase(u8),
}

impl core::error::Error for DemoError {}
