//! The formula language of `math.formula` nodes: numbers with units, the node's named inputs,
//! `+ - * /`, `^` with a whole-number exponent, parentheses and `min`, `max`, `abs`, `sqrt`,
//! `round`, `floor`, `ceil`. Nothing else: no assignment, no calls out, no loops.
//!
//! Dimensions are checked before evaluation: `3 m + 2` and `sqrt(3 m)` are errors, `2 m * 3 m`
//! is an area.

use std::collections::BTreeMap;

use crate::units::{self, Dim};
use crate::{MAX_EXPR_DEPTH, MAX_EXPR_LEN};

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64, Dim),
    Var(String),
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, i8),
    Call(Func, Vec<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    Min,
    Max,
    Abs,
    Sqrt,
    Round,
    Floor,
    Ceil,
}

impl Func {
    fn from_name(s: &str) -> Option<Func> {
        Some(match s {
            "min" => Func::Min,
            "max" => Func::Max,
            "abs" => Func::Abs,
            "sqrt" => Func::Sqrt,
            "round" => Func::Round,
            "floor" => Func::Floor,
            "ceil" => Func::Ceil,
            _ => return None,
        })
    }
}

/// Why a formula was refused or failed.
#[derive(Clone, Debug, PartialEq)]
pub enum FormulaError {
    Syntax(String),
    UnitMismatch(String),
    DivisionByZero,
    NotFinite,
}

impl std::fmt::Display for FormulaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormulaError::Syntax(m) => write!(f, "{m}"),
            FormulaError::UnitMismatch(m) => write!(f, "{m}"),
            FormulaError::DivisionByZero => write!(f, "division by zero"),
            FormulaError::NotFinite => write!(f, "the result is not a finite number"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Sym(char),
}

fn lex(s: &str) -> Result<Vec<Tok>, FormulaError> {
    if s.len() > MAX_EXPR_LEN {
        return Err(FormulaError::Syntax(format!("a formula is at most {MAX_EXPR_LEN} characters")));
    }
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == b'.' {
            let start = i;
            while b.get(i).is_some_and(|c| c.is_ascii_digit() || *c == b'.') {
                i += 1;
            }
            let text = s.get(start..i).unwrap_or("");
            let v: f64 = text.parse().map_err(|_| FormulaError::Syntax(format!("`{text}` is not a number")))?;
            out.push(Tok::Num(v));
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while b.get(i).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
                i += 1;
            }
            out.push(Tok::Ident(s.get(start..i).unwrap_or("").to_string()));
        } else if b"+-*/^(),".contains(&c) {
            out.push(Tok::Sym(c as char));
            i += 1;
        } else {
            return Err(FormulaError::Syntax(format!("`{}` is not allowed in a formula", c as char)));
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::Sym(c)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn enter(&mut self) -> Result<(), FormulaError> {
        self.depth += 1;
        if self.depth > MAX_EXPR_DEPTH { Err(FormulaError::Syntax("the formula is nested too deeply".into())) } else { Ok(()) }
    }
    fn expr(&mut self) -> Result<Expr, FormulaError> {
        self.enter()?;
        let mut e = self.term()?;
        loop {
            let op = if self.eat('+') {
                Op::Add
            } else if self.eat('-') {
                Op::Sub
            } else {
                break;
            };
            e = Expr::Bin(op, Box::new(e), Box::new(self.term()?));
        }
        self.depth -= 1;
        Ok(e)
    }
    fn term(&mut self) -> Result<Expr, FormulaError> {
        let mut e = self.unary()?;
        loop {
            let op = if self.eat('*') {
                Op::Mul
            } else if self.eat('/') {
                Op::Div
            } else {
                break;
            };
            e = Expr::Bin(op, Box::new(e), Box::new(self.unary()?));
        }
        Ok(e)
    }
    fn unary(&mut self) -> Result<Expr, FormulaError> {
        if self.eat('-') {
            self.enter()?;
            let e = Expr::Neg(Box::new(self.unary()?));
            self.depth -= 1;
            return Ok(e);
        }
        let base = self.primary()?;
        if self.eat('^') {
            let neg = self.eat('-');
            let Some(Tok::Num(n)) = self.next() else { return Err(FormulaError::Syntax("`^` needs a whole-number exponent".into())) };
            if n.fract() != 0.0 || n > 12.0 {
                return Err(FormulaError::Syntax("`^` needs a whole-number exponent of at most 12".into()));
            }
            let n = n as i8;
            return Ok(Expr::Pow(Box::new(base), if neg { -n } else { n }));
        }
        Ok(base)
    }
    fn primary(&mut self) -> Result<Expr, FormulaError> {
        match self.next() {
            Some(Tok::Num(v)) => {
                // A unit may follow a number: `3.2 m`, `230 mm`.
                if let Some(Tok::Ident(u)) = self.peek()
                    && let Some((d, f)) = units::unit(u)
                {
                    self.pos += 1;
                    return Ok(Expr::Num(v * f, d));
                }
                Ok(Expr::Num(v, Dim::NONE))
            }
            Some(Tok::Ident(name)) => {
                if self.eat('(') {
                    let f =
                        Func::from_name(&name).ok_or_else(|| FormulaError::Syntax(format!("`{name}` is not a function of the formula language")))?;
                    self.enter()?;
                    let mut args = vec![self.expr()?];
                    while self.eat(',') {
                        if args.len() >= 16 {
                            return Err(FormulaError::Syntax("too many arguments".into()));
                        }
                        args.push(self.expr()?);
                    }
                    if !self.eat(')') {
                        return Err(FormulaError::Syntax("`)` expected".into()));
                    }
                    self.depth -= 1;
                    let want_one = !matches!(f, Func::Min | Func::Max);
                    if want_one && args.len() != 1 {
                        return Err(FormulaError::Syntax(format!("`{name}` takes one argument")));
                    }
                    Ok(Expr::Call(f, args))
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Some(Tok::Sym('(')) => {
                let e = self.expr()?;
                if !self.eat(')') {
                    return Err(FormulaError::Syntax("`)` expected".into()));
                }
                Ok(e)
            }
            Some(t) => Err(FormulaError::Syntax(format!("unexpected `{}`", tok_text(&t)))),
            None => Err(FormulaError::Syntax("the formula ends too early".into())),
        }
    }
}

fn tok_text(t: &Tok) -> String {
    match t {
        Tok::Num(v) => v.to_string(),
        Tok::Ident(s) => s.clone(),
        Tok::Sym(c) => c.to_string(),
    }
}

pub fn parse(s: &str) -> Result<Expr, FormulaError> {
    let mut p = Parser { toks: lex(s)?, pos: 0, depth: 0 };
    if p.toks.is_empty() {
        return Err(FormulaError::Syntax("the formula is empty".into()));
    }
    let e = p.expr()?;
    if let Some(t) = p.peek() {
        return Err(FormulaError::Syntax(format!("unexpected `{}`", tok_text(t))));
    }
    Ok(e)
}

/// The dimension of `e` given the dimensions of its inputs.
pub fn infer(e: &Expr, vars: &BTreeMap<String, Dim>) -> Result<Dim, FormulaError> {
    let overflow = || FormulaError::UnitMismatch("the units of the formula are out of range".into());
    Ok(match e {
        Expr::Num(_, d) => *d,
        Expr::Var(n) => *vars.get(n).ok_or_else(|| FormulaError::Syntax(format!("`{n}` is not an input of this formula")))?,
        Expr::Neg(a) => infer(a, vars)?,
        Expr::Bin(op, a, b) => {
            let (da, db) = (infer(a, vars)?, infer(b, vars)?);
            match op {
                Op::Add | Op::Sub => {
                    if da != db {
                        return Err(FormulaError::UnitMismatch(format!("cannot add or subtract {} and {}", da.name(), db.name())));
                    }
                    da
                }
                Op::Mul => da.times(db).ok_or_else(overflow)?,
                Op::Div => da.per(db).ok_or_else(overflow)?,
            }
        }
        Expr::Pow(a, n) => infer(a, vars)?.pow(*n).ok_or_else(overflow)?,
        Expr::Call(f, args) => {
            let dims: Vec<Dim> = args.iter().map(|a| infer(a, vars)).collect::<Result<_, _>>()?;
            let first = dims.first().copied().unwrap_or(Dim::NONE);
            match f {
                Func::Min | Func::Max => {
                    if dims.iter().any(|d| *d != first) {
                        return Err(FormulaError::UnitMismatch("min and max need arguments of one unit".into()));
                    }
                    first
                }
                Func::Abs => first,
                Func::Sqrt => first.sqrt().ok_or_else(|| FormulaError::UnitMismatch(format!("the square root of {} has no unit", first.name())))?,
                Func::Round | Func::Floor | Func::Ceil => {
                    if first != Dim::NONE {
                        return Err(FormulaError::UnitMismatch("round, floor and ceil take a plain number (divide by a unit first)".into()));
                    }
                    first
                }
            }
        }
    })
}

/// Evaluate `e` with canonical input values.
pub fn eval(e: &Expr, vars: &BTreeMap<String, f64>) -> Result<f64, FormulaError> {
    let v = match e {
        Expr::Num(v, _) => *v,
        Expr::Var(n) => *vars.get(n).ok_or_else(|| FormulaError::Syntax(format!("`{n}` has no value")))?,
        Expr::Neg(a) => -eval(a, vars)?,
        Expr::Bin(op, a, b) => {
            let (x, y) = (eval(a, vars)?, eval(b, vars)?);
            match op {
                Op::Add => x + y,
                Op::Sub => x - y,
                Op::Mul => x * y,
                Op::Div => {
                    if y == 0.0 {
                        return Err(FormulaError::DivisionByZero);
                    }
                    x / y
                }
            }
        }
        Expr::Pow(a, n) => eval(a, vars)?.powi(i32::from(*n)),
        Expr::Call(f, args) => {
            let xs: Vec<f64> = args.iter().map(|a| eval(a, vars)).collect::<Result<_, _>>()?;
            let x = xs.first().copied().unwrap_or(0.0);
            match f {
                Func::Min => xs.iter().copied().fold(f64::INFINITY, f64::min),
                Func::Max => xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                Func::Abs => x.abs(),
                Func::Sqrt => {
                    if x < 0.0 {
                        return Err(FormulaError::NotFinite);
                    }
                    x.sqrt()
                }
                Func::Round => x.round(),
                Func::Floor => x.floor(),
                Func::Ceil => x.ceil(),
            }
        }
    };
    if v.is_finite() { Ok(v) } else { Err(FormulaError::NotFinite) }
}

/// Input names must not be unit or function names, so `2 m` always means two metres.
pub fn valid_input_name(n: &str) -> bool {
    crate::graph::valid_id(n)
        && n.len() <= 32
        && n.bytes().next().is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && n.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        && units::unit(n).is_none()
        && Func::from_name(n).is_none()
}
