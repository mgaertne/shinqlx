use core::fmt::Write;

use crate::run::{FileReport, Outcome};

/// Lists the migrated files and all warnings, followed by a summary line.
pub(crate) fn render(reports: &[FileReport], dry_run: bool) -> String {
    let migrated_label = if dry_run { "would migrate" } else { "migrated" };
    let mut text = String::new();
    let mut migrated = 0;
    let mut warnings = 0;

    for report in reports {
        let path = report.path.display();
        if let Outcome::Migrated { edits } = report.outcome {
            migrated += 1;
            let _ = writeln!(text, "{migrated_label}: {path} ({edits} edits)");
        }
        for warning in &report.warnings {
            warnings += 1;
            if warning.line == 0 {
                let _ = writeln!(text, "warning: {path}: {}", warning.message);
            } else {
                let _ = writeln!(
                    text,
                    "warning: {path}:{}: {}",
                    warning.line, warning.message
                );
            }
        }
    }

    let _ = writeln!(
        text,
        "{} files: {migrated} {}, {} copied unchanged, {warnings} warnings",
        reports.len(),
        if dry_run {
            "would be migrated"
        } else {
            "migrated"
        },
        reports.len() - migrated,
    );
    text
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::render;
    use crate::{
        rules::Warning,
        run::{FileReport, Outcome},
    };

    fn reports() -> Vec<FileReport> {
        vec![
            FileReport {
                path: PathBuf::from("a.py"),
                outcome: Outcome::Migrated { edits: 2 },
                warnings: vec![Warning::new(7, "check this")],
            },
            FileReport {
                path: PathBuf::from("b.py"),
                outcome: Outcome::Copied,
                warnings: vec![],
            },
        ]
    }

    #[test]
    fn renders_migrated_files_warnings_and_summary() {
        assert_eq!(
            render(&reports(), false),
            "migrated: a.py (2 edits)\nwarning: a.py:7: check this\n2 files: 1 migrated, 1 copied unchanged, 1 warnings\n"
        );
    }

    #[test]
    fn dry_run_uses_the_conditional() {
        assert!(render(&reports(), true).starts_with("would migrate: a.py (2 edits)"));
    }
}
