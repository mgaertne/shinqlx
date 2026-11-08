use tree_sitter::Node;

use super::{Rule, RuleOutput, Warning};
use crate::{
    edit::Edit,
    source::{Source, line_of},
    tree::{method_call, positional_arguments, walk},
};

/// The redis wrapper of shinqlx takes `value`, `time`, `amount` and `count` by keyword only.
pub(crate) struct RedisKeywords;

fn insert_keywords(output: &mut RuleOutput, arguments: &[Node<'_>], keywords: [&str; 2]) {
    for (argument, keyword) in arguments[1..].iter().zip(keywords) {
        let start = argument.start_byte();
        output
            .edits
            .push(Edit::replace(start..start, format!("{keyword}=")));
    }
}

fn looks_like_amount(argument: Node<'_>) -> bool {
    matches!(argument.kind(), "integer" | "float" | "unary_operator")
}

/// redis-py 2.x takes `(name, value, amount)`, redis-py 3 `(name, amount, value)`.
fn rewrite_zincrby(output: &mut RuleOutput, call_line: usize, arguments: &[Node<'_>]) {
    let keywords = match arguments {
        [_, value, amount] if !looks_like_amount(*value) && looks_like_amount(*amount) => {
            Some(["value", "amount"])
        }
        [_, amount, value] if looks_like_amount(*amount) && !looks_like_amount(*value) => {
            Some(["amount", "value"])
        }
        _ => None,
    };
    match keywords {
        Some(keywords) => insert_keywords(output, arguments, keywords),
        None => output.warnings.push(Warning::new(
            call_line,
            "zincrby takes `value` and `amount` by keyword only, the order of the positional \
             arguments could not be determined",
        )),
    }
}

impl Rule for RedisKeywords {
    fn name(&self) -> &'static str {
        "redis-keywords"
    }

    fn apply(&self, source: &Source<'_>) -> RuleOutput {
        let mut output = RuleOutput::default();
        walk(source.root(), &mut |node| {
            let Some(call) = method_call(node) else {
                return;
            };
            let Some(arguments) = positional_arguments(call.arguments) else {
                return;
            };
            match (source.text_of(call.name), arguments.len()) {
                ("setex", 3) => insert_keywords(&mut output, &arguments, ["time", "value"]),
                ("lrem", 3) => insert_keywords(&mut output, &arguments, ["count", "value"]),
                ("zincrby", 2 | 3) => rewrite_zincrby(&mut output, line_of(node), &arguments),
                _ => {}
            }
        });
        output
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::RedisKeywords;
    use crate::{edit::apply_edits, rules::Rule, source::Source};

    fn run(text: &str) -> (String, usize) {
        let source = Source::parse(text).expect("the test source is valid python");
        let output = RedisKeywords.apply(&source);
        let warnings = output.warnings.len();
        (
            apply_edits(text, output.edits).expect("the edits do not overlap"),
            warnings,
        )
    }

    #[rstest]
    #[case("db.setex(key, 60, 'x')\n", "db.setex(key, time=60, value='x')\n")]
    #[case("db.lrem(key, 0, item)\n", "db.lrem(key, count=0, value=item)\n")]
    #[case(
        "db.zincrby(key, ban_id, -score)\n",
        "db.zincrby(key, value=ban_id, amount=-score)\n"
    )]
    #[case(
        "db.zincrby(key, -score, ban_id)\n",
        "db.zincrby(key, amount=-score, value=ban_id)\n"
    )]
    #[case(
        "self.db.zincrby(KEY.format(map_name) + ':count', k_id, 1)\n",
        "self.db.zincrby(KEY.format(map_name) + ':count', value=k_id, amount=1)\n"
    )]
    fn adds_the_keywords(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(run(input), (expected.to_string(), 0));
    }

    #[rstest]
    #[case("db.zincrby(key, value='x', amount=1)\n")]
    #[case("db.setex(key, time=60, value='x')\n")]
    #[case("db.setex(key, 60, value='x')\n")]
    #[case("db.setex(*args)\n")]
    #[case("db.lrem(key, 0)\n")]
    #[case("setex(key, 60, 'x')\n")]
    fn leaves_other_calls_alone(#[case] input: &str) {
        assert_eq!(run(input), (input.to_string(), 0));
    }

    #[rstest]
    #[case("db.zincrby(key, a, b)\n")]
    #[case("db.zincrby(key, 1, 2)\n")]
    #[case("db.zincrby(key, x)\n")]
    fn warns_when_the_order_is_unclear(#[case] input: &str) {
        assert_eq!(run(input), (input.to_string(), 1));
    }
}
