use clap::Parser;

/// QuakeLive server stats
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub(crate) struct CommandLineOptions {
    /// ZMQ URI to connect to.
    #[arg(long, default_value = "tcp://127.0.0.1:27961")]
    pub(crate) host: String,
    /// The ZMQ password.
    #[arg(long, default_value = "")]
    pub(crate) password: String,
    /// Pretty print received json data
    #[arg(long)]
    pub(crate) pretty_print: bool,
}
