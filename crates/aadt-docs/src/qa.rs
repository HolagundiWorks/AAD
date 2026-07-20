//! Documentation QA — deterministic completeness checks before a sheet is issued.
//!
//! Answers "is this sheet ready to send out?" the same way [`anomaly`] flagging
//! answers "might the quantities be wrong?": with computed, model-free checks, so
//! the warnings never depend on AI availability. A check points at something that
//! would embarrass the firm if issued — a blank title field, a scale that reads
//! `1:100` on a sheet whose title block says otherwise, a sheet with no notes.
//!
//! [`anomaly`]: aadt_detect::anomaly

use crate::titleblock::TitleBlock;

/// How much a QA finding should hold up issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum QaSeverity {
    /// Worth a glance; the sheet can still go out.
    Advisory,
    /// Should be fixed before the sheet is issued.
    Blocking,
}

/// One documentation problem found.
#[derive(Debug, Clone, PartialEq)]
pub struct QaFinding {
    pub severity: QaSeverity,
    pub message: String,
}

impl QaFinding {
    fn blocking(message: impl Into<String>) -> Self {
        Self {
            severity: QaSeverity::Blocking,
            message: message.into(),
        }
    }

    fn advisory(message: impl Into<String>) -> Self {
        Self {
            severity: QaSeverity::Advisory,
            message: message.into(),
        }
    }
}

/// What a sheet is expected to carry, for [`review`] to check against.
///
/// [`review`]: SheetReview::review
#[derive(Debug, Clone)]
pub struct SheetReview {
    /// Require the drawing to carry at least one general note.
    pub require_notes: bool,
    /// Number of general notes actually placed (the caller counts them).
    pub note_count: usize,
    /// Whether a sheet border was placed.
    pub has_border: bool,
}

impl Default for SheetReview {
    fn default() -> Self {
        Self {
            require_notes: true,
            note_count: 0,
            has_border: false,
        }
    }
}

impl SheetReview {
    /// Review a title block and the sheet context, returning findings sorted
    /// most-severe first, then alphabetically — deterministic output.
    pub fn review(&self, tb: &TitleBlock) -> Vec<QaFinding> {
        let mut out = Vec::new();

        // Required title-block fields must be filled.
        for &label in TitleBlock::REQUIRED {
            let filled = tb.field(label).is_some_and(|v| !v.trim().is_empty());
            if !filled {
                out.push(QaFinding::blocking(format!(
                    "title block field '{label}' is blank"
                )));
            }
        }

        // Advisory: unchecked drawings and blank revisions are common oversights.
        if tb.checked_by.trim().is_empty() {
            out.push(QaFinding::advisory("title block 'Checked By' is blank"));
        }
        if tb.revision.trim().is_empty() {
            out.push(QaFinding::advisory("title block 'Revision' is blank"));
        }

        if !self.has_border {
            out.push(QaFinding::advisory("sheet has no border (run PLOTSETUP)"));
        }
        if self.require_notes && self.note_count == 0 {
            out.push(QaFinding::advisory("sheet carries no general notes"));
        }

        out.sort_by(|a, b| {
            // Blocking before advisory (reverse of the enum's natural order),
            // then by message for a stable read.
            b.severity
                .cmp(&a.severity)
                .then_with(|| a.message.cmp(&b.message))
        });
        out
    }

    /// Whether any [`QaSeverity::Blocking`] finding exists — the gate for issue.
    pub fn is_issuable(&self, tb: &TitleBlock) -> bool {
        !self
            .review(tb)
            .iter()
            .any(|f| f.severity == QaSeverity::Blocking)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_tb() -> TitleBlock {
        TitleBlock {
            project: "Riverside Depot".into(),
            sheet_title: "Ground Floor Plan".into(),
            sheet_number: "A-101".into(),
            date: "2026-07-20".into(),
            scale: "1:100".into(),
            drawn_by: "VH".into(),
            checked_by: "JS".into(),
            revision: "A".into(),
            extra: vec![],
        }
    }

    #[test]
    fn a_complete_sheet_is_issuable() {
        let review = SheetReview {
            require_notes: true,
            note_count: 4,
            has_border: true,
        };
        let findings = review.review(&complete_tb());
        assert!(
            findings.is_empty(),
            "expected no findings, got {findings:#?}"
        );
        assert!(review.is_issuable(&complete_tb()));
    }

    #[test]
    fn blank_required_fields_block_issue() {
        let mut tb = complete_tb();
        tb.sheet_number = "  ".into(); // whitespace counts as blank
        tb.scale = "".into();
        let review = SheetReview {
            has_border: true,
            note_count: 1,
            ..Default::default()
        };
        let findings = review.review(&tb);
        let blocking: Vec<_> = findings
            .iter()
            .filter(|f| f.severity == QaSeverity::Blocking)
            .collect();
        assert_eq!(blocking.len(), 2, "sheet number + scale, got {findings:#?}");
        assert!(!review.is_issuable(&tb));
    }

    #[test]
    fn missing_border_and_notes_are_advisory_not_blocking() {
        let review = SheetReview {
            require_notes: true,
            note_count: 0,
            has_border: false,
        };
        let findings = review.review(&complete_tb());
        assert!(findings.iter().all(|f| f.severity == QaSeverity::Advisory));
        assert!(review.is_issuable(&complete_tb()), "advisories don't block");
        assert!(findings.iter().any(|f| f.message.contains("border")));
        assert!(findings
            .iter()
            .any(|f| f.message.contains("no general notes")));
    }

    #[test]
    fn findings_are_ordered_blocking_first_then_stable() {
        let mut tb = complete_tb();
        tb.project = "".into(); // blocking
        tb.checked_by = "".into(); // advisory
        let review = SheetReview {
            has_border: true,
            note_count: 2,
            ..Default::default()
        };
        let findings = review.review(&tb);
        assert_eq!(findings[0].severity, QaSeverity::Blocking);
        // Deterministic: same input, same output.
        assert_eq!(findings, review.review(&tb));
    }
}
