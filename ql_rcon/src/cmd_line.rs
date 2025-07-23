use clap::Parser;
use termcolor::ColorChoice;

/// QuakeLive rcon options
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub(crate) struct CommandLineOptions {
    /// ZMQ URI to connect to.
    #[arg(long, default_value = "tcp://127.0.0.1:27961")]
    pub(crate) host: String,
    /// The ZMQ password.
    #[arg(long, default_value = "")]
    pub(crate) password: String,
    /// Specify the socket identity. Random UUID used by default
    #[arg(long, default_value = "")]
    pub(crate) identity: String,
    /// Use color output
    #[arg(long, default_value = "auto")]
    pub(crate) color: ColorChoice,
}
