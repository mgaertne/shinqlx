//! The `appworkshop_282440.acf` manifest that Steam keeps: it lists every installed item in more than one section.

/// Removes every block of the item, e.g. `"3301267783" { ... }`, from the manifest text; returns the new text and how
/// many blocks were removed.
pub(crate) fn without_item(manifest: &str, id: u64) -> (String, usize) {
    let header = format!("\"{id}\"");
    let mut kept = Vec::new();
    let mut removed = 0;
    let mut lines = manifest.lines().peekable();

    while let Some(line) = lines.next() {
        let opens_block =
            line.trim() == header && lines.peek().is_some_and(|next| next.trim() == "{");
        if !opens_block {
            kept.push(line);
            continue;
        }

        removed += 1;
        let mut depth = 0_usize;
        for inner in lines.by_ref() {
            match inner.trim() {
                "{" => depth += 1,
                "}" => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    let mut text = kept.join("\n");
    if manifest.ends_with('\n') {
        text.push('\n');
    }
    (text, removed)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    const MANIFEST: &str = "\"AppWorkshop\"\n{\n\t\"WorkshopItemsInstalled\"\n\t{\n\
    \t\t\"11\"\n\t\t{\n\t\t\t\"size\"\t\t\"1\"\n\t\t}\n\t\t\"22\"\n\t\t{\n\t\t\t\"size\"\t\t\"2\"\n\t\t}\n\
    \t}\n\
    \t\"WorkshopItemDetails\"\n\t{\n\t\t\"22\"\n\t\t{\n\t\t\t\"manifest\"\t\t\"9\"\n\t\t}\n\t\t\"11\"\n\t\t{\n\t\t\t\"manifest\"\t\t\"8\"\n\t\t}\n\t}\n}\n";

    #[test]
    fn removes_the_item_from_every_section_and_keeps_the_others() {
        let (text, removed) = without_item(MANIFEST, 22);

        assert_eq!(removed, 2);
        assert_eq!(
            text,
            "\"AppWorkshop\"\n{\n\t\"WorkshopItemsInstalled\"\n\t{\n\
    \t\t\"11\"\n\t\t{\n\t\t\t\"size\"\t\t\"1\"\n\t\t}\n\t}\n\
    \t\"WorkshopItemDetails\"\n\t{\n\t\t\"11\"\n\t\t{\n\t\t\t\"manifest\"\t\t\"8\"\n\t\t}\n\t}\n}\n"
        );
    }

    #[test]
    fn an_item_that_is_not_listed_leaves_the_text_alone() {
        let (text, removed) = without_item(MANIFEST, 33);

        assert_eq!(removed, 0);
        assert_eq!(text, MANIFEST);
    }

    #[test]
    fn a_longer_id_that_contains_the_id_is_not_touched() {
        let (_, removed) = without_item(MANIFEST, 2);

        assert_eq!(removed, 0);
    }

    #[test]
    fn a_block_without_its_closing_brace_ends_with_the_text() {
        let (text, removed) = without_item("\"5\"\n{\n\"a\" \"b\"\n", 5);

        assert_eq!(removed, 1);
        assert_eq!(text, "\n");
    }
}
