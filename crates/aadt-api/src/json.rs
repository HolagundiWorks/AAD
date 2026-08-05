//! Minimal JSON encode/decode — std only.
//!
//! The workspace hand-rolls its formats rather than taking a serialisation
//! dependency (`prefs.rs`, the `.vdb` codec, the PDF writer), and the API is
//! not the place to break that. The scope here is deliberately the API's own
//! traffic, not JSON in general:
//!
//! - **Encoding** covers what responses contain: strings, finite numbers,
//!   booleans, nulls, objects, arrays.
//! - **Decoding** covers what requests contain: one flat object whose values
//!   are strings, numbers, booleans, or null. Nested values are rejected
//!   rather than half-parsed, so a request that means something this module
//!   cannot represent fails loudly instead of arriving with fields missing.

use std::fmt::Write as _;

/// Escape a string's contents for a JSON string literal (without the quotes).
///
/// Control characters below 0x20 must be escaped; everything else is passed
/// through as UTF-8, which is already valid JSON.
pub fn escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    for c in raw.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// A quoted JSON string.
pub fn string(raw: &str) -> String {
    format!("\"{}\"", escape(raw))
}

/// A JSON number, or `null` when the value is not finite.
///
/// `NaN` and the infinities have no JSON representation. Emitting `null`
/// keeps the document parseable; emitting `NaN` would produce a body that
/// every strict client rejects, turning one bad coordinate into a failed
/// request for the whole drawing.
pub fn number(v: f64) -> String {
    if v.is_finite() {
        let mut s = format!("{v}");
        if s == "-0" {
            s = "0".to_string();
        }
        s
    } else {
        "null".to_string()
    }
}

/// A JSON integer.
pub fn int(v: u64) -> String {
    v.to_string()
}

/// A JSON boolean.
pub fn bool_(v: bool) -> String {
    if v { "true" } else { "false" }.to_string()
}

/// An object from `(key, already-encoded value)` pairs, in the given order.
pub fn object(fields: &[(&str, String)]) -> String {
    let body = fields
        .iter()
        .map(|(k, v)| format!("{}:{}", string(k), v))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

/// An array of already-encoded values.
pub fn array<I: IntoIterator<Item = String>>(items: I) -> String {
    format!("[{}]", items.into_iter().collect::<Vec<_>>().join(","))
}

/// An array of strings.
pub fn string_array<'a, I: IntoIterator<Item = &'a str>>(items: I) -> String {
    array(items.into_iter().map(string))
}

/// A decoded scalar from a request body.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }
}

/// A decoded flat JSON object, preserving key order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Object {
    fields: Vec<(String, Value)>,
}

impl Object {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// The value of a string field, or `None` when absent or another type.
    pub fn str_field(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }
}

/// Parse a flat JSON object. An empty input is an empty object, so a body-less
/// POST is a request with no fields rather than a parse error.
pub fn parse_object(src: &str) -> Result<Object, String> {
    let mut p = Parser {
        chars: src.chars().collect(),
        pos: 0,
    };
    p.skip_ws();
    if p.done() {
        return Ok(Object::default());
    }
    let obj = p.object()?;
    p.skip_ws();
    if !p.done() {
        return Err("trailing characters after the JSON object".to_string());
    }
    Ok(obj)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn done(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, want: char) -> Result<(), String> {
        self.skip_ws();
        match self.bump() {
            Some(c) if c == want => Ok(()),
            Some(c) => Err(format!("expected {want:?}, found {c:?}")),
            None => Err(format!("expected {want:?}, found end of input")),
        }
    }

    fn object(&mut self) -> Result<Object, String> {
        self.expect('{')?;
        let mut fields = Vec::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.pos += 1;
            return Ok(Object { fields });
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.expect(':')?;
            let value = self.value()?;
            fields.push((key, value));
            self.skip_ws();
            match self.bump() {
                Some(',') => continue,
                Some('}') => break,
                Some(c) => return Err(format!("expected ',' or '}}', found {c:?}")),
                None => return Err("unterminated object".to_string()),
            }
        }
        Ok(Object { fields })
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err("unterminated string".to_string()),
                Some('"') => return Ok(out),
                Some('\\') => match self.bump() {
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('/') => out.push('/'),
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('b') => out.push('\u{08}'),
                    Some('f') => out.push('\u{0c}'),
                    Some('u') => out.push(self.unicode_escape()?),
                    Some(c) => return Err(format!("unknown escape \\{c}")),
                    None => return Err("unterminated escape".to_string()),
                },
                Some(c) => out.push(c),
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, String> {
        let mut hex = String::new();
        for _ in 0..4 {
            match self.bump() {
                Some(c) if c.is_ascii_hexdigit() => hex.push(c),
                _ => return Err("malformed \\u escape".to_string()),
            }
        }
        let code = u32::from_str_radix(&hex, 16).map_err(|e| e.to_string())?;
        // Surrogate halves cannot stand alone. Rather than reassembling pairs
        // for an API whose inputs are command lines, substitute the
        // replacement character so the request still carries its other fields.
        Ok(char::from_u32(code).unwrap_or('\u{fffd}'))
    }

    fn value(&mut self) -> Result<Value, String> {
        self.skip_ws();
        match self.peek() {
            Some('"') => self.string().map(Value::Str),
            Some('t') => self.literal("true").map(|_| Value::Bool(true)),
            Some('f') => self.literal("false").map(|_| Value::Bool(false)),
            Some('n') => self.literal("null").map(|_| Value::Null),
            Some('{') | Some('[') => {
                Err("nested objects and arrays are not accepted by this API".to_string())
            }
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("unexpected character {c:?} in value")),
            None => Err("unexpected end of input in value".to_string()),
        }
    }

    fn literal(&mut self, word: &str) -> Result<(), String> {
        for want in word.chars() {
            match self.bump() {
                Some(c) if c == want => {}
                _ => return Err(format!("expected literal {word}")),
            }
        }
        Ok(())
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
        {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse::<f64>()
            .map(Value::Num)
            .map_err(|_| format!("malformed number {text:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_quotes_backslashes_and_controls() {
        assert_eq!(escape("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(escape("line\nbreak"), "line\\nbreak");
        assert_eq!(escape("\u{1}"), "\\u0001");
        // Non-ASCII passes through: UTF-8 is already valid JSON.
        assert_eq!(escape("mm²"), "mm²");
    }

    #[test]
    fn non_finite_numbers_become_null_not_nan() {
        assert_eq!(number(1.5), "1.5");
        assert_eq!(number(f64::NAN), "null");
        assert_eq!(number(f64::INFINITY), "null");
        // A bill or a coordinate printing "-0" reads as a defect.
        assert_eq!(number(-0.0), "0");
    }

    #[test]
    fn builds_objects_and_arrays_in_order() {
        let json = object(&[
            ("a", int(1)),
            ("b", string("x")),
            ("c", array(["1".into()])),
        ]);
        assert_eq!(json, r#"{"a":1,"b":"x","c":[1]}"#);
    }

    #[test]
    fn parses_a_flat_object() {
        let o = parse_object(r#"{"line":"LINE 0,0 5,0","echo":true,"n":2.5,"z":null}"#).unwrap();
        assert_eq!(o.str_field("line"), Some("LINE 0,0 5,0"));
        assert_eq!(o.get("echo").and_then(Value::as_bool), Some(true));
        assert_eq!(o.get("n").and_then(Value::as_number), Some(2.5));
        assert_eq!(o.get("z"), Some(&Value::Null));
        assert_eq!(o.len(), 4);
    }

    #[test]
    fn round_trips_escaped_text_through_encode_and_decode() {
        let raw = "TEXT 0,0 2 \"a\\b\"\nnext";
        let body = object(&[("line", string(raw))]);
        let back = parse_object(&body).unwrap();
        assert_eq!(back.str_field("line"), Some(raw));
    }

    #[test]
    fn empty_body_is_an_empty_object_not_an_error() {
        assert!(parse_object("").unwrap().is_empty());
        assert!(parse_object("  \n ").unwrap().is_empty());
        assert!(parse_object("{}").unwrap().is_empty());
    }

    #[test]
    fn malformed_and_nested_input_is_rejected_loudly() {
        assert!(parse_object("{\"a\":").is_err());
        assert!(parse_object("{\"a\":1}{").is_err());
        assert!(parse_object("[1,2]").is_err());
        // Silently dropping a nested field would leave the caller thinking it
        // was honoured.
        assert!(parse_object(r#"{"a":{"b":1}}"#).is_err());
    }

    #[test]
    fn decodes_unicode_escapes() {
        let o = parse_object(r#"{"s":"\u00b2 \u0041"}"#).unwrap();
        assert_eq!(o.str_field("s"), Some("² A"));
    }
}
