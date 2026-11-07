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

#[cfg(test)]
mod cmd_line_tests {
    use clap::Parser;

    use super::CommandLineOptions;

    #[test]
    fn defaults_connect_to_the_local_server() {
        let args = CommandLineOptions::try_parse_from(["ql_stats"]).expect("defaults are valid");

        assert_eq!(args.host, "tcp://127.0.0.1:27961");
        assert_eq!(args.password, "");
        assert!(!args.pretty_print);
    }

    #[test]
    fn options_override_the_defaults() {
        let args = CommandLineOptions::try_parse_from([
            "ql_stats",
            "--host",
            "tcp://192.0.2.1:27960",
            "--password",
            "secret",
            "--pretty-print",
        ])
        .expect("the options are valid");

        assert_eq!(args.host, "tcp://192.0.2.1:27960");
        assert_eq!(args.password, "secret");
        assert!(args.pretty_print);
    }
}
