//! The `workshop.txt` of a server: one item id per line, optionally followed by a comment.

/// The lines of a `workshop.txt`; comments, blank lines and the order of the items survive every change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WorkshopList {
    lines: Vec<String>,
}

fn id_of(line: &str) -> Option<u64> {
    line.split('#')
        .next()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

impl WorkshopList {
    pub(crate) fn parse(content: &str) -> Self {
        Self {
            lines: content.lines().map(str::to_owned).collect(),
        }
    }

    /// The ids of the items with the comment of their line, in file order.
    pub(crate) fn entries(&self) -> impl Iterator<Item = (u64, &str)> {
        self.lines.iter().filter_map(|line| {
            let id = id_of(line)?;
            let comment = line
                .split_once('#')
                .map_or("", |(_, comment)| comment.trim());
            Some((id, comment))
        })
    }

    pub(crate) fn contains(&self, id: u64) -> bool {
        self.entries().any(|(known, _)| known == id)
    }

    /// Appends the item, unless it is listed already; returns whether the list changed.
    pub(crate) fn add(&mut self, id: u64, comment: &str) -> bool {
        if self.contains(id) {
            return false;
        }
        self.lines.push(if comment.is_empty() {
            id.to_string()
        } else {
            format!("{id} # {comment}")
        });
        true
    }

    /// Removes every line of the item; returns whether the list changed.
    pub(crate) fn remove(&mut self, id: u64) -> bool {
        let before = self.lines.len();
        self.lines.retain(|line| id_of(line) != Some(id));
        self.lines.len() != before
    }

    pub(crate) fn render(&self) -> String {
        let mut text = self.lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    const SAMPLE: &str = "# specify 1 workshop item id per line\n\n\
    542770475 # maido.pk3 (maido)\n547252823\n  552722973   # a.pk3 (a, b), c.pk3 (c)\n";

    #[test]
    fn reads_ids_and_comments_and_skips_the_rest() {
        let list = WorkshopList::parse(SAMPLE);

        assert_eq!(
            list.entries().collect::<Vec<_>>(),
            vec![
                (542_770_475, "maido.pk3 (maido)"),
                (547_252_823, ""),
                (552_722_973, "a.pk3 (a, b), c.pk3 (c)"),
            ]
        );
    }

    #[test]
    fn an_unchanged_list_renders_as_it_was_read() {
        assert_eq!(WorkshopList::parse(SAMPLE).render(), SAMPLE);
    }

    #[test]
    fn adds_an_item_once_at_the_end_with_its_comment() {
        let mut list = WorkshopList::parse(SAMPLE);

        assert!(list.add(3_301_267_783, "restlessexile.pk3 (restlessexile)"));
        assert!(!list.add(3_301_267_783, "again"));
        assert!(list.add(7, ""));

        assert_eq!(
            list.render(),
            format!("{SAMPLE}3301267783 # restlessexile.pk3 (restlessexile)\n7\n")
        );
    }

    #[test]
    fn removes_only_the_lines_of_the_item() {
        let mut list = WorkshopList::parse(SAMPLE);

        assert!(list.remove(547_252_823));
        assert!(!list.remove(547_252_823));

        assert_eq!(
            list.render(),
            "# specify 1 workshop item id per line\n\n\
    542770475 # maido.pk3 (maido)\n  552722973   # a.pk3 (a, b), c.pk3 (c)\n"
        );
    }

    #[test]
    fn an_empty_file_stays_empty_until_an_item_is_added() {
        let mut list = WorkshopList::parse("");
        assert_eq!(list.render(), "");

        list.add(1, "");
        assert_eq!(list.render(), "1\n");
    }

    #[test]
    fn a_comment_that_starts_with_a_number_is_no_item() {
        assert_eq!(WorkshopList::parse("# 494372396\n").entries().count(), 0);
    }
}
