use log::{Level, Record};
use log4rs::encode::{Encode, Write, pattern::PatternEncoder};

/// A pattern encoder that prints the `::` of Rust targets as `.` and the level `WARN` as `WARNING`,
/// so that the records of Rust and the loggers of Python look the same in the log.
#[derive(Debug)]
pub(crate) struct DottedPatternEncoder {
    inner: PatternEncoder,
}

impl DottedPatternEncoder {
    /// The placeholders `{l}` and `{level}` of the pattern print the level names of Python.
    pub(crate) fn new(pattern: &str) -> Self {
        Self {
            inner: PatternEncoder::new(
                &pattern
                    .replace("{l}", "{K(levelname)}")
                    .replace("{level}", "{K(levelname)}"),
            ),
        }
    }
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::Warn => "WARNING",
        other => other.as_str(),
    }
}

impl Encode for DottedPatternEncoder {
    fn encode(&self, writer: &mut dyn Write, record: &Record<'_>) -> anyhow::Result<()> {
        struct Collector(Vec<(String, String)>);
        impl<'kvs> log::kv::VisitSource<'kvs> for Collector {
            fn visit_pair(
                &mut self,
                key: log::kv::Key<'kvs>,
                value: log::kv::Value<'kvs>,
            ) -> Result<(), log::kv::Error> {
                self.0.push((key.to_string(), value.to_string()));
                Ok(())
            }
        }

        let mut collector = Collector(Vec::new());
        let _ = record.key_values().visit(&mut collector);
        let mut key_values: Vec<(&str, &str)> = collector
            .0
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        key_values.push(("levelname", level_name(record.level())));

        let target = record.target().replace("::", ".");
        self.inner.encode(
            writer,
            &record
                .to_builder()
                .target(&target)
                .key_values(&key_values.as_slice())
                .build(),
        )
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DottedPatternConfig {
    pattern: String,
}

pub(super) struct DottedPatternEncoderDeserializer;

impl log4rs::config::Deserialize for DottedPatternEncoderDeserializer {
    type Config = DottedPatternConfig;
    type Trait = dyn Encode;

    fn deserialize(
        &self,
        config: DottedPatternConfig,
        _deserializers: &log4rs::config::Deserializers,
    ) -> anyhow::Result<Box<dyn Encode>> {
        Ok(Box::new(DottedPatternEncoder::new(&config.pattern)))
    }
}

#[cfg(test)]
mod tests {
    use log4rs::encode::writer::simple::SimpleWriter;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(log::Level::Error, "ERROR")]
    #[case(log::Level::Warn, "WARNING")]
    #[case(log::Level::Info, "INFO")]
    #[case(log::Level::Debug, "DEBUG")]
    fn levels_are_printed_like_python_does(#[case] level: log::Level, #[case] expected: &str) {
        let mut output = Vec::new();
        DottedPatternEncoder::new("{l}|{level}|{K(func)}")
            .encode(
                &mut SimpleWriter(&mut output),
                &Record::builder()
                    .target("shinqlx")
                    .level(level)
                    .args(format_args!("text"))
                    .key_values(&[("func", "dispatch")].as_slice())
                    .build(),
            )
            .unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!("{expected}|{expected}|dispatch")
        );
    }

    #[rstest]
    #[case("shinqlx", "shinqlx")]
    #[case("shinqlx::hooks", "shinqlx.hooks")]
    #[case("shinqlx::plugins::balance", "shinqlx.plugins.balance")]
    fn targets_are_printed_with_dots(#[case] target: &str, #[case] expected: &str) {
        let mut output = Vec::new();
        DottedPatternEncoder::new("{t} {l} {K(func)} {m}")
            .encode(
                &mut SimpleWriter(&mut output),
                &Record::builder()
                    .target(target)
                    .level(log::Level::Info)
                    .args(format_args!("text"))
                    .key_values(&[("func", "dispatch")].as_slice())
                    .build(),
            )
            .unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!("{expected} INFO dispatch text")
        );
    }
}
