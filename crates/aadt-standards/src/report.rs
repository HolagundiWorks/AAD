//! Render the HCW layer standard as shareable documents (CSV / Markdown) for
//! QA, hand-off, and BIM/spec coordination.

use crate::catalog::{Discipline, STANDARD_LAYERS};

/// Escape a field for CSV (RFC 4180): quote when it contains a comma, quote, or
/// newline, doubling any embedded quotes.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The full catalog as CSV — one row per layer, every column of metadata.
pub fn catalog_csv() -> String {
    let mut out = String::from(
        "Layer,Discipline,Element,Phase,ACI,Linetype,Lineweight_mm,Plot,IFC,Uniformat,MasterFormat,AI_Tags\n",
    );
    for l in STANDARD_LAYERS {
        let cols = [
            l.name.to_string(),
            l.discipline.label().to_string(),
            l.element.label().to_string(),
            l.phase.label().to_string(),
            l.aci.to_string(),
            format!("{:?}", l.linetype),
            format!("{:.2}", l.lineweight_mm),
            if l.plottable { "Yes" } else { "No" }.to_string(),
            l.ifc().to_string(),
            l.uniformat().to_string(),
            l.masterformat().to_string(),
            l.ai_tags().join(" "),
        ];
        let row: Vec<String> = cols.iter().map(|c| csv_field(c)).collect();
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

/// The full catalog as Markdown, grouped by discipline with a table per group.
pub fn catalog_markdown() -> String {
    let mut out = String::from("# HCW CAD Layer Standard\n\n");
    out.push_str(&format!(
        "{} layers across {} disciplines. Colours are ACI (screen colours that \
         plot black); lineweights are millimetres.\n",
        STANDARD_LAYERS.len(),
        Discipline::ALL.len()
    ));
    for d in Discipline::ALL {
        let rows: Vec<_> = STANDARD_LAYERS.iter().filter(|l| l.discipline == *d).collect();
        if rows.is_empty() {
            continue;
        }
        out.push_str(&format!("\n## {} ({})\n\n", d.label(), d.prefix()));
        out.push_str("| Layer | Element | ACI | Linetype | LW (mm) | Phase | IFC | Uniformat | MF |\n");
        out.push_str("|-------|---------|-----|----------|---------|-------|-----|-----------|----|\n");
        for l in rows {
            out.push_str(&format!(
                "| {} | {} | {} | {:?} | {:.2} | {} | {} | {} | {} |\n",
                l.name,
                l.element.label(),
                l.aci,
                l.linetype,
                l.lineweight_mm,
                l.phase.label(),
                l.ifc(),
                l.uniformat(),
                l.masterformat(),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_has_header_and_a_row_per_layer() {
        let csv = catalog_csv();
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines[0].starts_with("Layer,Discipline,Element,Phase,ACI"));
        assert_eq!(lines.len(), STANDARD_LAYERS.len() + 1, "one row per layer + header");
    }

    #[test]
    fn csv_contains_core_layers_and_metadata() {
        let csv = catalog_csv();
        assert!(csv.contains("A-WALL,Architecture,Wall,New,1,Continuous,0.50,Yes,IfcWall,B2010,04,"));
        assert!(csv.contains("A-WINDOW,Architecture,Window,New,"));
        assert!(csv.contains("IfcWindow,B2020,08,"));
    }

    #[test]
    fn markdown_has_a_section_per_discipline() {
        let md = catalog_markdown();
        assert!(md.starts_with("# HCW CAD Layer Standard"));
        for d in Discipline::ALL {
            assert!(
                md.contains(&format!("## {} ({})", d.label(), d.prefix())),
                "missing section for {:?}",
                d
            );
        }
    }

    #[test]
    fn csv_fields_with_special_chars_are_quoted() {
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("plain"), "plain");
    }
}
