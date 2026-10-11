//! The architect calculator: expressions with feet and inches, and automatic length, area and
//! volume. `10' 6" x 20'` is an area, `3 x 4 x 5` a volume, and every result is shown in metric and
//! imperial at once. Metres are the working unit; the calculator does not read the drawing's units.
//!
//! This is a port of the Architect Calculator widget (the HC Works Android calculator) so the
//! desktop widget, the `cal` command, scripts and the agent all give the same answers.
//!
//! Input is hostile: expressions are length-capped, nesting is bounded, and every failure is an
//! `Err`/ignored keypress, never a panic.

const METRES_PER_FOOT: f64 = 0.3048;
const METRES_PER_INCH: f64 = 0.0254;
const SQUARE_METRES_PER_SQUARE_FOOT: f64 = 0.092_903_04;
const CUBIC_METRES_PER_CUBIC_FOOT: f64 = 0.028_316_846_592;
/// Longest expression accepted (keypad or command line).
pub const MAX_EXPRESSION: usize = 512;
const MAX_DEPTH: usize = 64;
const OPERATORS: &[char] = &['+', '-', 'x', '*', '/', '^'];

/// What the expression measures: one factor is a length, two an area, three or more a volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Length,
    Area,
    Volume,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Length => "length",
            Kind::Area => "area",
            Kind::Volume => "volume",
        }
    }
}

/// A measured expression, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measure {
    pub metres: f64,
    pub square_metres: f64,
    pub cubic_metres: f64,
    pub kind: Kind,
    /// The expression used feet or inches, so the answer is shown in imperial.
    pub imperial: bool,
}

/// The six figures shown under the display, formatted with thousands separators and 2 decimals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Results {
    pub metres: String,
    pub square_metres: String,
    pub cubic_metres: String,
    pub feet: String,
    pub square_feet: String,
    pub cubic_feet: String,
}

// ---------------- plain arithmetic ----------------

struct Parser<'a> {
    s: &'a [char],
    i: usize,
    depth: usize,
}

type R<T> = std::result::Result<T, String>;

/// Evaluate `+ - * / ^` with parentheses on plain numbers (no units). `x` and `×` multiply,
/// `÷` divides, `−` subtracts.
pub fn eval(expr: &str) -> R<f64> {
    if expr.chars().count() > MAX_EXPRESSION {
        return Err("expression too long".into());
    }
    let normalised: Vec<char> = expr
        .chars()
        .map(|c| match c {
            '×' => '*',
            '÷' => '/',
            '−' => '-',
            c => c,
        })
        .collect();
    let trimmed: Vec<char> = {
        let s: String = normalised.iter().collect();
        s.trim().chars().collect()
    };
    if trimmed.is_empty() {
        return Ok(0.0);
    }
    let mut p = Parser { s: &trimmed, i: 0, depth: 0 };
    let v = p.expression()?;
    p.skip();
    if p.i != p.s.len() {
        return Err("unexpected input".into());
    }
    if v.is_finite() { Ok(v) } else { Err("result is not a finite number".into()) }
}

impl Parser<'_> {
    fn skip(&mut self) {
        while self.s.get(self.i).is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }
    fn take(&mut self, c: char) -> bool {
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn enter(&mut self) -> R<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { Err("expression too deeply nested".into()) } else { Ok(()) }
    }
    fn expression(&mut self) -> R<f64> {
        let mut v = self.term()?;
        loop {
            self.skip();
            if self.take('+') {
                v += self.term()?;
            } else if self.take('-') {
                v -= self.term()?;
            } else {
                return Ok(v);
            }
        }
    }
    fn term(&mut self) -> R<f64> {
        let mut v = self.power()?;
        loop {
            self.skip();
            if self.take('*') {
                v *= self.power()?;
            } else if self.take('/') {
                let d = self.power()?;
                if d == 0.0 {
                    return Err("division by zero".into());
                }
                v /= d;
            } else {
                return Ok(v);
            }
        }
    }
    fn power(&mut self) -> R<f64> {
        let base = self.unary()?;
        self.skip();
        if self.take('^') {
            self.enter()?;
            let e = self.power();
            self.depth -= 1;
            return Ok(base.powf(e?));
        }
        Ok(base)
    }
    fn unary(&mut self) -> R<f64> {
        self.skip();
        self.enter()?;
        let r = if self.take('+') {
            self.unary()
        } else if self.take('-') {
            self.unary().map(|v| -v)
        } else if self.take('(') {
            let v = self.expression()?;
            self.skip();
            if self.take(')') { Ok(v) } else { Err("missing parenthesis".into()) }
        } else {
            self.number()
        };
        self.depth -= 1;
        r
    }
    fn number(&mut self) -> R<f64> {
        self.skip();
        let start = self.i;
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_digit() || *c == '.') {
            self.i += 1;
        }
        if self.i == start {
            return Err("number expected".into());
        }
        let t: String = self.s.get(start..self.i).unwrap_or(&[]).iter().collect();
        t.parse::<f64>().map_err(|_| format!("bad number `{t}`"))
    }
}

// ---------------- feet and inches ----------------

/// Read `\d+(\.\d+)?` at `i`: the value and the index after it.
fn scan_number(c: &[char], i: usize) -> (f64, usize) {
    let mut j = i;
    while c.get(j).is_some_and(char::is_ascii_digit) {
        j += 1;
    }
    if c.get(j) == Some(&'.') && c.get(j + 1).is_some_and(char::is_ascii_digit) {
        j += 1;
        while c.get(j).is_some_and(char::is_ascii_digit) {
            j += 1;
        }
    }
    let t: String = c.get(i..j).unwrap_or(&[]).iter().collect();
    (t.parse().unwrap_or(0.0), j)
}

fn skip_ws(c: &[char], mut i: usize) -> usize {
    while c.get(i).is_some_and(|x| x.is_whitespace()) {
        i += 1;
    }
    i
}

/// Replace `10' 6"`, `10'` and `6"` with their value in metres; everything else is kept.
fn imperial_to_metres(token: &str) -> String {
    let t = token.replace('×', "*").replace(['x', 'X'], "*");
    let c: Vec<char> = t.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while let Some(&ch) = c.get(i) {
        if !ch.is_ascii_digit() {
            out.push(ch);
            i += 1;
            continue;
        }
        let (n, j) = scan_number(&c, i);
        if c.get(j) == Some(&'\'') {
            let after = skip_ws(&c, j + 1);
            let mut end = after;
            let mut inches = 0.0;
            let mut m = after;
            if c.get(m) == Some(&'-') {
                m = skip_ws(&c, m + 1);
            }
            if c.get(m).is_some_and(char::is_ascii_digit) {
                let (n2, e2) = scan_number(&c, m);
                if c.get(e2) == Some(&'"') {
                    inches = n2;
                    end = e2 + 1;
                }
            }
            out.push_str(&(n * METRES_PER_FOOT + inches * METRES_PER_INCH).to_string());
            i = end;
        } else if c.get(j) == Some(&'"') {
            out.push_str(&(n * METRES_PER_INCH).to_string());
            i = j + 1;
        } else {
            out.extend(c.get(i..j).unwrap_or(&[]));
            i = j;
        }
    }
    if out.trim().is_empty() { "0".into() } else { out }
}

fn parse_length(token: &str) -> R<f64> {
    let m = imperial_to_metres(token);
    match m.trim().parse::<f64>() {
        Ok(v) => Ok(v),
        Err(_) => eval(&m),
    }
}

/// Split at top-level `x`, `X`, `*` and `×`.
fn split_factors(expr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for ch in expr.chars() {
        match ch {
            '(' => {
                depth += 1;
                cur.push(ch);
            }
            ')' => {
                depth -= 1;
                cur.push(ch);
            }
            'x' | 'X' | '*' | '×' if depth == 0 => {
                let t = cur.trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    let t = cur.trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
    if out.is_empty() {
        let t = expr.trim();
        out.push(if t.is_empty() { "0".into() } else { t.to_string() });
    }
    out
}

/// Measure an expression: its length, area or volume in metres.
pub fn measure(expr: &str) -> R<Measure> {
    if expr.chars().count() > MAX_EXPRESSION {
        return Err("expression too long".into());
    }
    let imperial = expr.contains('\'') || expr.contains('"');
    let factors = split_factors(expr);
    let mut metres = Vec::with_capacity(factors.len());
    for f in &factors {
        metres.push(parse_length(f)?);
    }
    let at = |i: usize| metres.get(i).copied().unwrap_or(0.0);
    let m = if metres.len() >= 3 {
        let sq = at(0) * at(1);
        Measure { metres: at(0), square_metres: sq, cubic_metres: sq * at(2), kind: Kind::Volume, imperial }
    } else if metres.len() == 2 {
        Measure { metres: at(0), square_metres: at(0) * at(1), cubic_metres: 0.0, kind: Kind::Area, imperial }
    } else {
        Measure { metres: at(0), square_metres: 0.0, cubic_metres: 0.0, kind: Kind::Length, imperial }
    };
    if [m.metres, m.square_metres, m.cubic_metres].iter().all(|v| v.is_finite()) { Ok(m) } else { Err("result is not a finite number".into()) }
}

// ---------------- formatting ----------------

/// `1,234.50`; `Error` when the value is not a number.
pub fn number(v: f64) -> String {
    if !v.is_finite() {
        return "Error".into();
    }
    let s = format!("{:.2}", v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, "00"));
    let mut grouped = String::new();
    for (n, ch) in int.chars().enumerate() {
        if n > 0 && (int.len() - n).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let neg = v < 0.0 && s != "0.00";
    format!("{}{grouped}.{frac}", if neg { "-" } else { "" })
}

fn plain(v: f64) -> String {
    if v.is_finite() { format!("{v:.2}") } else { "Error".into() }
}

fn result_label(kind: Kind, value: f64, imperial: bool) -> String {
    let unit = match (kind, imperial) {
        (Kind::Length, false) => "m",
        (Kind::Length, true) => "ft",
        (Kind::Area, false) => "m²",
        (Kind::Area, true) => "ft²",
        (Kind::Volume, false) => "m³",
        (Kind::Volume, true) => "ft³",
    };
    format!("{} {unit}", number(value))
}

fn value_in_unit(m: &Measure) -> f64 {
    match (m.kind, m.imperial) {
        (Kind::Length, false) => m.metres,
        (Kind::Length, true) => m.metres / METRES_PER_FOOT,
        (Kind::Area, false) => m.square_metres,
        (Kind::Area, true) => m.square_metres / SQUARE_METRES_PER_SQUARE_FOOT,
        (Kind::Volume, false) => m.cubic_metres,
        (Kind::Volume, true) => m.cubic_metres / CUBIC_METRES_PER_CUBIC_FOOT,
    }
}

/// The six figures for a measure.
pub fn results_of(m: &Measure) -> Results {
    Results {
        metres: number(m.metres),
        square_metres: number(m.square_metres),
        cubic_metres: number(m.cubic_metres),
        feet: number(m.metres / METRES_PER_FOOT),
        square_feet: number(m.square_metres / SQUARE_METRES_PER_SQUARE_FOOT),
        cubic_feet: number(m.cubic_metres / CUBIC_METRES_PER_CUBIC_FOOT),
    }
}

/// `expression → result`, for the command line and the agent.
pub fn summary_of(expr: &str, m: &Measure) -> String {
    format!("{}  →  {}", format_expression(expr, m), result_label(m.kind, value_in_unit(m), m.imperial))
}

fn pretty(v: &str) -> String {
    let mut s = String::new();
    let mut prev_digit = false;
    for ch in v.chars() {
        match ch {
            'x' | 'X' | '*' => s.push_str(" × "),
            '/' => s.push_str(" ÷ "),
            '+' => s.push_str(" + "),
            '-' if prev_digit => s.push_str(" − "),
            c => s.push(c),
        }
        prev_digit = ch.is_ascii_digit();
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn format_expression(expr: &str, m: &Measure) -> String {
    if m.imperial {
        return pretty(expr);
    }
    let f = split_factors(expr);
    if f.len() > 1 { f.iter().map(|x| format!("{} m", pretty(x))).collect::<Vec<_>>().join(" × ") } else { format!("{} m", pretty(expr)) }
}

// ---------------- the keypad state machine ----------------

/// What the keypad has typed and what the last `=` produced.
#[derive(Clone, Debug, Default)]
pub struct Calculator {
    raw: String,
    last_expression: String,
    evaluated: Option<Kind>,
    evaluated_imperial: bool,
}

impl Calculator {
    pub fn new() -> Self {
        Self::default()
    }

    /// The text typed so far (after `=`, the result number).
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The expression behind the last `=`.
    pub fn last_expression(&self) -> &str {
        &self.last_expression
    }

    /// The big display line: the expression while typing, the result with its unit after `=`.
    pub fn display(&self) -> String {
        if self.raw.is_empty() {
            return "0".into();
        }
        if let (Some(kind), Ok(v)) = (self.evaluated, self.raw.parse::<f64>()) {
            return result_label(kind, v, self.evaluated_imperial);
        }
        pretty(&self.raw)
    }

    /// What the expression measures (length until there are two factors).
    pub fn kind(&self) -> Kind {
        self.evaluated.unwrap_or_else(|| measure(if self.raw.trim().is_empty() { "0" } else { &self.raw }).map_or(Kind::Length, |m| m.kind))
    }

    /// The small line above the display: `10' 6" × 20'  →  210.00 ft²`.
    pub fn expression_summary(&self) -> String {
        let src = if self.last_expression.trim().is_empty() { &self.raw } else { &self.last_expression };
        if src.trim().is_empty() {
            return String::new();
        }
        measure(src).map(|m| summary_of(src, &m)).unwrap_or_default()
    }

    fn reset_evaluation(&mut self) {
        if self.evaluated.is_some() {
            self.raw.clear();
            self.last_expression.clear();
        }
        self.evaluated = None;
        self.evaluated_imperial = false;
    }

    fn room(&self) -> bool {
        self.raw.chars().count() + 2 <= MAX_EXPRESSION
    }

    /// One digit, `0` to `9`. Anything else is ignored.
    pub fn append_digit(&mut self, d: &str) {
        if d.chars().count() != 1 || !d.chars().all(|c| c.is_ascii_digit()) {
            return;
        }
        self.reset_evaluation();
        if !self.room() {
            return;
        }
        if self.raw == "0" {
            self.raw = d.to_string();
        } else {
            self.raw.push_str(d);
        }
    }

    fn last_number_chunk(&self) -> &str {
        let end = self.raw.len();
        let start = self.raw.char_indices().rev().take_while(|(_, c)| c.is_ascii_digit() || *c == '.').last().map_or(end, |(i, _)| i);
        self.raw.get(start..).unwrap_or("")
    }

    pub fn append_dot(&mut self) {
        self.reset_evaluation();
        if self.last_number_chunk().contains('.') || !self.room() {
            return;
        }
        if self.raw.chars().last().is_some_and(|c| c.is_ascii_digit()) {
            self.raw.push('.');
        } else {
            self.raw.push_str("0.");
        }
    }

    /// `+ - x * × ÷ / ^`. A second operator replaces the first.
    pub fn append_operator(&mut self, v: &str) {
        self.evaluated = None;
        let op = match v {
            "×" | "*" | "x" | "X" => 'x',
            "÷" | "/" => '/',
            "+" => '+',
            "-" | "−" => '-',
            "^" => '^',
            _ => return,
        };
        let Some(last) = self.raw.chars().last() else {
            if op == '-' {
                self.raw.push('-');
            }
            return;
        };
        if OPERATORS.contains(&last) {
            self.raw.pop();
            self.raw.push(op);
        } else if self.room() {
            self.raw.push(op);
        }
    }

    pub fn append_parenthesis(&mut self, v: &str) {
        if v != "(" && v != ")" {
            return;
        }
        self.reset_evaluation();
        if self.room() {
            self.raw.push_str(v);
        }
    }

    pub fn append_feet(&mut self) {
        self.reset_evaluation();
        if self.raw.chars().last().is_some_and(|c| c.is_ascii_digit()) && self.room() {
            self.raw.push('\'');
        }
    }

    pub fn append_inches(&mut self) {
        self.reset_evaluation();
        if self.raw.chars().last().is_some_and(|c| c.is_ascii_digit()) && self.room() {
            self.raw.push('"');
        }
    }

    pub fn backspace(&mut self) {
        self.reset_evaluation();
        self.raw.pop();
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// `=`: measure the expression and replace it with the result. An expression that does not
    /// parse is left as typed.
    pub fn evaluate(&mut self) {
        let expr = self.raw.trim_end_matches(['x', '*', '+', '-', '/', ' ', '^']).to_string();
        if expr.trim().is_empty() {
            return;
        }
        let Ok(m) = measure(&expr) else { return };
        self.last_expression = expr;
        self.evaluated = Some(m.kind);
        self.evaluated_imperial = m.imperial;
        self.raw = plain(value_in_unit(&m));
    }

    /// The six figures for the last result (or the expression being typed).
    pub fn results(&self) -> Results {
        let src = if !self.last_expression.trim().is_empty() {
            self.last_expression.as_str()
        } else if !self.raw.trim().is_empty() {
            self.raw.as_str()
        } else {
            "0"
        };
        let m = measure(src).unwrap_or(Measure { metres: 0.0, square_metres: 0.0, cubic_metres: 0.0, kind: Kind::Length, imperial: false });
        results_of(&m)
    }

    /// Put back a saved state.
    pub fn restore(&mut self, raw: &str, last_expression: &str) {
        self.raw = raw.chars().take(MAX_EXPRESSION).collect();
        self.last_expression = last_expression.chars().take(MAX_EXPRESSION).collect();
        self.evaluated = if self.last_expression.trim().is_empty() { None } else { measure(&self.last_expression).ok().map(|m| m.kind) };
        self.evaluated_imperial = self.last_expression.contains('\'') || self.last_expression.contains('"');
    }

    /// Type a whole line as the keypad would (used by the keyboard and the tests): digits,
    /// operators, brackets, `'` and `"`; `=` evaluates; anything else is ignored.
    pub fn type_char(&mut self, c: char) {
        match c {
            '0'..='9' => self.append_digit(&c.to_string()),
            '.' | ',' => self.append_dot(),
            '+' | '-' | '*' | '/' | '^' | 'x' | 'X' | '×' | '÷' | '−' => self.append_operator(&c.to_string()),
            '(' | ')' => self.append_parenthesis(&c.to_string()),
            '\'' | '′' => self.append_feet(),
            '"' | '″' => self.append_inches(),
            '=' => self.evaluate(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(expr: &str) -> Calculator {
        let mut c = Calculator::new();
        for ch in expr.chars() {
            c.type_char(ch);
        }
        c.evaluate();
        c
    }

    #[test]
    fn the_documented_examples() {
        let c = run("10' 6\" x 20'");
        assert_eq!(c.display(), "210.00 ft²");
        let r = c.results();
        assert_eq!((r.square_metres.as_str(), r.square_feet.as_str()), ("19.51", "210.00"));
        let c = run("3x4x5");
        assert_eq!(c.display(), "60.00 m³");
        assert_eq!(c.results().cubic_feet, "2,118.88");
        assert_eq!(run("(2+3)x4").display(), "20.00 m²");
        assert_eq!(run("5+2").kind(), Kind::Length);
    }

    #[test]
    fn plain_arithmetic_has_precedence_and_powers() {
        assert_eq!(eval("2+3*4"), Ok(14.0));
        assert_eq!(eval("(2+3)*4"), Ok(20.0));
        assert_eq!(eval("2^3^2"), Ok(512.0));
        assert_eq!(eval("-2^2"), Ok(4.0));
        assert_eq!(eval("8 ÷ 2 × 3 − 1"), Ok(11.0));
        assert_eq!(eval(""), Ok(0.0));
        assert!(eval("1/0").is_err());
        assert!(eval("1+").is_err());
        assert!(eval("((1").is_err());
        assert!(eval("1.2.3").is_err());
    }

    #[test]
    fn feet_and_inches_forms() {
        let m = measure("10'").unwrap();
        assert!((m.metres - 3.048).abs() < 1e-12 && m.imperial);
        let m = measure("6\"").unwrap();
        assert!((m.metres - 0.1524).abs() < 1e-12);
        let m = measure("5' - 3\"").unwrap();
        assert!((m.metres - (5.0 * 0.3048 + 3.0 * 0.0254)).abs() < 1e-12);
        let m = measure("2' 6\" x 4'").unwrap();
        assert_eq!(m.kind, Kind::Area);
        assert!(!measure("12.5").unwrap().imperial);
    }

    #[test]
    fn keypad_rules() {
        let mut c = Calculator::new();
        c.append_operator("-");
        assert_eq!(c.raw(), "-");
        c.clear();
        c.append_operator("+");
        assert_eq!(c.raw(), "", "an operator cannot start an expression, except a minus");
        c.append_digit("5");
        c.append_operator("+");
        c.append_operator("×");
        assert_eq!(c.raw(), "5x", "a second operator replaces the first");
        c.append_dot();
        assert_eq!(c.raw(), "5x0.");
        c.append_dot();
        assert_eq!(c.raw(), "5x0.", "one dot per number");
        c.backspace();
        c.backspace();
        c.append_feet();
        assert_eq!(c.raw(), "5x");
        c.append_digit("0");
        c.append_digit("0");
        assert_eq!(c.raw(), "5x00", "only a lone 0 is replaced by the next digit");
    }

    #[test]
    fn typing_after_equals_starts_a_new_expression() {
        let mut c = run("3x4");
        assert_eq!(c.display(), "12.00 m²");
        c.append_digit("7");
        assert_eq!(c.raw(), "7");
        let mut c = run("3x4");
        c.append_operator("+");
        assert!(c.raw().ends_with('+'), "an operator continues from the result");
        assert_eq!(c.last_expression(), "3x4");
        assert!(c.expression_summary().contains("→"));
    }

    #[test]
    fn restore_keeps_the_result_kind() {
        let mut c = Calculator::new();
        c.restore("12.00", "3x4");
        assert_eq!(c.display(), "12.00 m²");
        assert_eq!(c.kind(), Kind::Area);
    }

    #[test]
    fn hostile_input_never_panics() {
        let long = "9".repeat(5000);
        assert!(measure(&long).is_err());
        assert!(eval(&"(".repeat(5000)).is_err());
        assert!(eval(&"-".repeat(1000)).is_err());
        assert!(eval(&format!("{}1{}", "(".repeat(100), ")".repeat(100))).is_err(), "nesting is bounded");
        let _ = measure(
            "99999999999999999999999999999999999999999999999x99999999999999999999999999999999999999999999999999x9999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999",
        );
        let mut c = Calculator::new();
        for ch in "ünïcode ✓ \u{0} ''\"\" ((( ))) x x x ^^^ ...".chars().cycle().take(5000) {
            c.type_char(ch);
        }
        c.evaluate();
        let _ = (c.display(), c.results(), c.expression_summary(), c.kind());
        assert!(c.raw().chars().count() <= MAX_EXPRESSION);
        assert_eq!(number(f64::NAN), "Error");
        assert_eq!(number(-0.001), "0.00");
        assert_eq!(number(-1234567.891), "-1,234,567.89");
    }
}
