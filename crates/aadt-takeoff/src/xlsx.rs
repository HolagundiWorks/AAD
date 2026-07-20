//! Minimal `.xlsx` (OOXML SpreadsheetML) writer — dependency-free.
//!
//! An `.xlsx` file is a ZIP archive of XML parts. The workspace is deliberately
//! std-only (see the root `Cargo.toml`), so rather than pull in a spreadsheet
//! crate this module writes both layers by hand:
//!
//! - **ZIP**, using the *stored* (uncompressed) method. Deflate would need a
//!   compressor; stored entries are valid ZIP and Excel opens them. Takeoff
//!   tables are small enough that the size cost is irrelevant.
//! - **SpreadsheetML**, with inline strings rather than a shared-string table,
//!   which keeps each sheet self-contained.
//!
//! **Output is byte-for-byte deterministic**: entries are written in a fixed
//! order and every timestamp is pinned to the DOS epoch (1980-01-01), so the
//! same takeoff always produces the same bytes. That is what lets an `.xlsx`
//! be golden-tested the way the CSV is, and it matches the reproducibility
//! guarantee in spec §8.

/// A single spreadsheet cell.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Empty,
    Text(String),
    /// Pre-formatted numeric literal. Held as a string so the takeoff's own
    /// rounding is the single source of truth and the `.xlsx` cannot disagree
    /// with the `.csv` in the last decimal place.
    Number(String),
}

impl Cell {
    pub fn text(s: impl Into<String>) -> Self {
        Cell::Text(s.into())
    }
}

/// One worksheet: a name and a grid of rows.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<Cell>>,
}

impl Sheet {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rows: Vec::new(),
        }
    }

    pub fn push_row(&mut self, row: Vec<Cell>) {
        self.rows.push(row);
    }
}

/// Serialize sheets into `.xlsx` bytes.
pub fn workbook(sheets: &[Sheet]) -> Vec<u8> {
    let mut zip = Zip::new();
    zip.add("[Content_Types].xml", content_types(sheets).into_bytes());
    zip.add("_rels/.rels", ROOT_RELS.as_bytes().to_vec());
    zip.add("xl/workbook.xml", workbook_xml(sheets).into_bytes());
    zip.add(
        "xl/_rels/workbook.xml.rels",
        workbook_rels(sheets).into_bytes(),
    );
    zip.add("xl/styles.xml", STYLES.as_bytes().to_vec());
    for (i, s) in sheets.iter().enumerate() {
        zip.add(
            &format!("xl/worksheets/sheet{}.xml", i + 1),
            sheet_xml(s).into_bytes(),
        );
    }
    zip.finish()
}

// --- XML parts ---------------------------------------------------------------

const ROOT_RELS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>"#,
    r#"</Relationships>"#
);

/// The smallest style sheet Excel accepts. Every cell uses format 0; the takeoff
/// carries its own rounding, so no number formats are declared.
const STYLES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    r#"<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>"#,
    r#"<fills count="1"><fill><patternFill patternType="none"/></fill></fills>"#,
    r#"<borders count="1"><border/></borders>"#,
    r#"<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>"#,
    r#"<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>"#,
    r#"</styleSheet>"#
);

fn content_types(sheets: &[Sheet]) -> String {
    let mut s = String::from(concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
        r#"<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#,
    ));
    for i in 1..=sheets.len() {
        s.push_str(&format!(
            r#"<Override PartName="/xl/worksheets/sheet{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#
        ));
    }
    s.push_str("</Types>");
    s
}

fn workbook_xml(sheets: &[Sheet]) -> String {
    let mut s = String::from(concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main""#,
        r#" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
        r#"<sheets>"#,
    ));
    for (i, sheet) in sheets.iter().enumerate() {
        let id = i + 1;
        s.push_str(&format!(
            r#"<sheet name="{}" sheetId="{id}" r:id="rId{id}"/>"#,
            escape(&sheet.name)
        ));
    }
    s.push_str("</sheets></workbook>");
    s
}

fn workbook_rels(sheets: &[Sheet]) -> String {
    let mut s = String::from(concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    ));
    for i in 1..=sheets.len() {
        s.push_str(&format!(
            r#"<Relationship Id="rId{i}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{i}.xml"/>"#
        ));
    }
    // Styles takes the next free id so it never collides with a sheet.
    s.push_str(&format!(
        r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
        sheets.len() + 1
    ));
    s.push_str("</Relationships>");
    s
}

fn sheet_xml(sheet: &Sheet) -> String {
    let mut s = String::from(concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
        r#"<sheetData>"#,
    ));
    for (r, row) in sheet.rows.iter().enumerate() {
        let rn = r + 1;
        s.push_str(&format!(r#"<row r="{rn}">"#));
        for (c, cell) in row.iter().enumerate() {
            let re = format!("{}{}", column_name(c), rn);
            match cell {
                // Empty cells are omitted entirely — a sparse sheet is valid and
                // smaller, and Excel renders the gap identically.
                Cell::Empty => {}
                Cell::Text(t) => s.push_str(&format!(
                    r#"<c r="{re}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#,
                    escape(t)
                )),
                Cell::Number(n) => s.push_str(&format!(r#"<c r="{re}"><v>{n}</v></c>"#)),
            }
        }
        s.push_str("</row>");
    }
    s.push_str("</sheetData></worksheet>");
    s
}

/// Spreadsheet column name for a zero-based index: 0 → `A`, 25 → `Z`, 26 → `AA`.
fn column_name(mut index: usize) -> String {
    let mut name = Vec::new();
    loop {
        name.push(b'A' + (index % 26) as u8);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    name.reverse();
    String::from_utf8(name).expect("ASCII column name")
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

// --- ZIP (stored entries) ----------------------------------------------------

/// DOS timestamp for 1980-01-01 00:00. Pinned rather than taken from the clock
/// so archives are reproducible.
const DOS_DATE: u16 = 0x0021;
const DOS_TIME: u16 = 0;

struct Entry {
    name: String,
    data: Vec<u8>,
    crc: u32,
    offset: u32,
}

struct Zip {
    out: Vec<u8>,
    entries: Vec<Entry>,
}

impl Zip {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            entries: Vec::new(),
        }
    }

    fn add(&mut self, name: &str, data: Vec<u8>) {
        let offset = self.out.len() as u32;
        let crc = crc32(&data);
        let n = data.len() as u32;

        self.out.extend_from_slice(&0x0403_4b50u32.to_le_bytes()); // local header
        self.out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        self.out.extend_from_slice(&0u16.to_le_bytes()); // flags
        self.out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        self.out.extend_from_slice(&DOS_TIME.to_le_bytes());
        self.out.extend_from_slice(&DOS_DATE.to_le_bytes());
        self.out.extend_from_slice(&crc.to_le_bytes());
        self.out.extend_from_slice(&n.to_le_bytes()); // compressed size
        self.out.extend_from_slice(&n.to_le_bytes()); // uncompressed size
        self.out
            .extend_from_slice(&(name.len() as u16).to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        self.out.extend_from_slice(name.as_bytes());
        self.out.extend_from_slice(&data);

        self.entries.push(Entry {
            name: name.to_string(),
            data,
            crc,
            offset,
        });
    }

    fn finish(mut self) -> Vec<u8> {
        let cd_offset = self.out.len() as u32;
        for e in &self.entries {
            let n = e.data.len() as u32;
            self.out.extend_from_slice(&0x0201_4b50u32.to_le_bytes()); // central header
            self.out.extend_from_slice(&20u16.to_le_bytes()); // version made by
            self.out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            self.out.extend_from_slice(&0u16.to_le_bytes()); // flags
            self.out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
            self.out.extend_from_slice(&DOS_TIME.to_le_bytes());
            self.out.extend_from_slice(&DOS_DATE.to_le_bytes());
            self.out.extend_from_slice(&e.crc.to_le_bytes());
            self.out.extend_from_slice(&n.to_le_bytes());
            self.out.extend_from_slice(&n.to_le_bytes());
            self.out
                .extend_from_slice(&(e.name.len() as u16).to_le_bytes());
            self.out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            self.out.extend_from_slice(&0u16.to_le_bytes()); // comment len
            self.out.extend_from_slice(&0u16.to_le_bytes()); // disk number
            self.out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            self.out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            self.out.extend_from_slice(&e.offset.to_le_bytes());
            self.out.extend_from_slice(e.name.as_bytes());
        }
        let cd_size = self.out.len() as u32 - cd_offset;
        let count = self.entries.len() as u16;

        self.out.extend_from_slice(&0x0605_4b50u32.to_le_bytes()); // end of central dir
        self.out.extend_from_slice(&0u16.to_le_bytes()); // this disk
        self.out.extend_from_slice(&0u16.to_le_bytes()); // disk with cd
        self.out.extend_from_slice(&count.to_le_bytes());
        self.out.extend_from_slice(&count.to_le_bytes());
        self.out.extend_from_slice(&cd_size.to_le_bytes());
        self.out.extend_from_slice(&cd_offset.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        self.out
    }
}

/// CRC-32 (IEEE, reflected) as required by the ZIP spec.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_names_roll_over() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(27), "AB");
        assert_eq!(column_name(51), "AZ");
        assert_eq!(column_name(52), "BA");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
    }

    #[test]
    fn crc32_matches_known_vector() {
        // The canonical check value for "123456789".
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn escapes_xml_metacharacters() {
        assert_eq!(escape(r#"a<b>&"c'"#), "a&lt;b&gt;&amp;&quot;c&apos;");
        // A layer name like this must not break the sheet.
        assert_eq!(escape("A&B"), "A&amp;B");
    }

    #[test]
    fn archive_has_zip_signature_and_central_directory() {
        let mut s = Sheet::new("Takeoff");
        s.push_row(vec![Cell::text("category"), Cell::text("area")]);
        s.push_row(vec![Cell::text("ROOM"), Cell::Number("42.000".to_string())]);
        let bytes = workbook(&[s]);

        assert_eq!(&bytes[0..4], b"PK\x03\x04", "local file header signature");
        // End-of-central-directory signature lives in the last 22 bytes.
        let eocd = &bytes[bytes.len() - 22..];
        assert_eq!(&eocd[0..4], b"PK\x05\x06");
        // 6 parts: content types, root rels, workbook, workbook rels, styles, 1 sheet.
        assert_eq!(u16::from_le_bytes([eocd[10], eocd[11]]), 6);
    }

    #[test]
    fn archive_is_byte_reproducible() {
        let build = || {
            let mut s = Sheet::new("Takeoff");
            s.push_row(vec![Cell::text("category")]);
            workbook(&[s])
        };
        assert_eq!(build(), build(), "xlsx output must be deterministic");
    }

    #[test]
    fn empty_cells_are_omitted_but_row_survives() {
        let mut s = Sheet::new("S");
        s.push_row(vec![Cell::Empty, Cell::text("b")]);
        let xml = sheet_xml(&s);
        assert!(xml.contains(r#"<row r="1">"#));
        assert!(!xml.contains(r#"r="A1""#), "empty cell should be skipped");
        assert!(xml.contains(r#"r="B1""#));
    }
}
