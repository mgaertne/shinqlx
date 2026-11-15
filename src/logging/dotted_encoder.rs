use log::Record;
use log4rs::encode::{Encode, Write, pattern::PatternEncoder};

/// A pattern encoder that prints the `::` of Rust targets as `.`, so that the records of Rust and
/// the loggers of Python look the same in the log.
#[derive(Debug)]
pub(crate) struct DottedPatternEncoder {
    inner: PatternEncoder,
}

impl DottedPatternEncoder {
    pub(crate) fn new(pattern: &str) -> Self {
        Self {
            inner: PatternEncoder::new(pattern),
        }
    }
}

impl Encode for DottedPatternEncoder {
    fn encode(&self, writer: &mut dyn Write, record: &Record<'_>) -> anyhow::Result<()> {
        let target = record.target().replace("::", ".");
        self.inner
            .encode(writer, &record.to_builder().target(&target).build())
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
