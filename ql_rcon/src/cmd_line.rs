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

#[cfg(test)]
mod cmd_line_tests {
    use clap::Parser;
    use termcolor::ColorChoice;

    use super::CommandLineOptions;

    #[test]
    fn defaults_connect_to_the_local_server() {
        let args = CommandLineOptions::try_parse_from(["ql_rcon"]).expect("defaults are valid");

        assert_eq!(args.host, "tcp://127.0.0.1:27961");
        assert_eq!(args.password, "");
        assert_eq!(args.identity, "");
        assert_eq!(args.color, ColorChoice::Auto);
    }

    #[test]
    fn options_override_the_defaults() {
        let args = CommandLineOptions::try_parse_from([
            "ql_rcon",
            "--host",
            "tcp://192.0.2.1:28960",
            "--password",
            "secret",
            "--identity",
            "me",
            "--color",
            "never",
        ])
        .expect("the options are valid");

        assert_eq!(args.host, "tcp://192.0.2.1:28960");
        assert_eq!(args.password, "secret");
        assert_eq!(args.identity, "me");
        assert_eq!(args.color, ColorChoice::Never);
    }

    #[test]
    fn unknown_color_choices_are_rejected() {
        assert!(CommandLineOptions::try_parse_from(["ql_rcon", "--color", "rainbow"]).is_err());
    }
}
