//! The standard library — arithmetic, comparison, list, and predicate subrs.
//!
//! Every builtin is a [`Native`](crate::value::Native) taking pre-evaluated
//! arguments. They follow AutoLISP conventions: `+`/`*` fold, `-`/`/` subtract or
//! divide from the first argument, integer/real arithmetic promotes to real when
//! any operand is real, and comparisons return `t`/`nil`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::eval::{define, Env};
use crate::value::{cons, LispError, Native, Value};

/// Register every standard builtin into `env`. Output from `print`/`princ` is
/// collected into `out` so a host can capture it (the CLI prints it).
pub fn install(env: &Env, out: &Rc<RefCell<String>>) {
    let def = |name: &str, f: Value| define(env, name, f);

    // --- Arithmetic ---
    def("+", Native::new("+", |a| fold_num(a, 0.0, |x, y| x + y)));
    def("*", Native::new("*", |a| fold_num(a, 1.0, |x, y| x * y)));
    def("-", Native::new("-", sub));
    def("/", Native::new("/", div));
    def(
        "1+",
        Native::new("1+", |a| one_arg_num(a, "1+", |x| x + 1.0, |i| i + 1)),
    );
    def(
        "1-",
        Native::new("1-", |a| one_arg_num(a, "1-", |x| x - 1.0, |i| i - 1)),
    );
    def("min", Native::new("min", |a| fold_num1(a, "min", f64::min)));
    def("max", Native::new("max", |a| fold_num1(a, "max", f64::max)));
    def(
        "abs",
        Native::new("abs", |a| one_arg_num(a, "abs", f64::abs, i64::abs)),
    );
    def(
        "rem",
        Native::new("rem", |a| int_binop(a, "rem", |x, y| x % y)),
    );

    // --- Comparison (chained, AutoLISP style) ---
    def("=", Native::new("=", |a| compare(a, "=", |o| o.is_eq())));
    def("<", Native::new("<", |a| compare(a, "<", |o| o.is_lt())));
    def(">", Native::new(">", |a| compare(a, ">", |o| o.is_gt())));
    def("<=", Native::new("<=", |a| compare(a, "<=", |o| o.is_le())));
    def(">=", Native::new(">=", |a| compare(a, ">=", |o| o.is_ge())));
    def(
        "/=",
        Native::new("/=", |a| {
            // Not-equal: true when the two arguments differ.
            want(a, 2, "/=")?;
            Ok(boolean(a[0] != a[1]))
        }),
    );

    // --- Predicates ---
    def(
        "null",
        Native::new("null", |a| {
            want(a, 1, "null")?;
            Ok(boolean(matches!(a[0], Value::Nil)))
        }),
    );
    def(
        "not",
        Native::new("not", |a| {
            want(a, 1, "not")?;
            Ok(boolean(!a[0].is_truthy()))
        }),
    );
    def(
        "atom",
        Native::new("atom", |a| {
            want(a, 1, "atom")?;
            Ok(boolean(!matches!(a[0], Value::List(_))))
        }),
    );
    def(
        "listp",
        Native::new("listp", |a| {
            want(a, 1, "listp")?;
            Ok(boolean(matches!(a[0], Value::List(_) | Value::Nil)))
        }),
    );
    def(
        "numberp",
        Native::new("numberp", |a| {
            want(a, 1, "numberp")?;
            Ok(boolean(matches!(a[0], Value::Int(_) | Value::Real(_))))
        }),
    );
    def(
        "zerop",
        Native::new("zerop", |a| {
            want(a, 1, "zerop")?;
            Ok(boolean(a[0].as_f64() == Some(0.0)))
        }),
    );
    def(
        "equal",
        Native::new("equal", |a| {
            want(a, 2, "equal")?;
            Ok(boolean(a[0] == a[1]))
        }),
    );

    // --- List operations ---
    def("list", Native::new("list", |a| Ok(Value::List(a.to_vec()))));
    def(
        "car",
        Native::new("car", |a| {
            want(a, 1, "car")?;
            Ok(car(&a[0]))
        }),
    );
    def(
        "cdr",
        Native::new("cdr", |a| {
            want(a, 1, "cdr")?;
            Ok(cdr(&a[0]))
        }),
    );
    def(
        "cadr",
        Native::new("cadr", |a| {
            want(a, 1, "cadr")?;
            Ok(car(&cdr(&a[0])))
        }),
    );
    def(
        "cons",
        Native::new("cons", |a| {
            want(a, 2, "cons")?;
            Ok(cons(a[0].clone(), a[1].clone()))
        }),
    );
    def(
        "append",
        Native::new("append", |a| {
            let mut out = Vec::new();
            for v in a {
                out.extend(as_list(v));
            }
            Ok(if out.is_empty() {
                Value::Nil
            } else {
                Value::List(out)
            })
        }),
    );
    def(
        "length",
        Native::new("length", |a| {
            want(a, 1, "length")?;
            Ok(Value::Int(as_list(&a[0]).len() as i64))
        }),
    );
    def(
        "nth",
        Native::new("nth", |a| {
            want(a, 2, "nth")?;
            let Value::Int(n) = a[0] else {
                return Err(LispError::BadArgs("nth: index must be an integer".into()));
            };
            Ok(as_list(&a[1])
                .get(n.max(0) as usize)
                .cloned()
                .unwrap_or(Value::Nil))
        }),
    );
    def(
        "reverse",
        Native::new("reverse", |a| {
            want(a, 1, "reverse")?;
            let mut items = as_list(&a[0]);
            items.reverse();
            Ok(if items.is_empty() {
                Value::Nil
            } else {
                Value::List(items)
            })
        }),
    );
    def(
        "member",
        Native::new("member", |a| {
            want(a, 2, "member")?;
            let items = as_list(&a[1]);
            Ok(match items.iter().position(|x| x == &a[0]) {
                Some(i) => Value::List(items[i..].to_vec()),
                None => Value::Nil,
            })
        }),
    );
    def(
        "assoc",
        Native::new("assoc", |a| {
            // Association lists as lists of 2-element lists: `((0 "LINE") (8 "A"))`.
            want(a, 2, "assoc")?;
            for entry in as_list(&a[1]) {
                let pair = as_list(&entry);
                if pair.first() == Some(&a[0]) {
                    return Ok(entry);
                }
            }
            Ok(Value::Nil)
        }),
    );

    // --- Strings ---
    def(
        "strcat",
        Native::new("strcat", |a| {
            let mut s = String::new();
            for v in a {
                match v {
                    Value::Str(t) => s.push_str(t),
                    other => {
                        return Err(LispError::BadArgs(format!("strcat: not a string: {other}")))
                    }
                }
            }
            Ok(Value::Str(s))
        }),
    );
    def(
        "strlen",
        Native::new("strlen", |a| {
            want(a, 1, "strlen")?;
            match &a[0] {
                Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
                other => Err(LispError::BadArgs(format!("strlen: not a string: {other}"))),
            }
        }),
    );
    def(
        "itoa",
        Native::new("itoa", |a| {
            want(a, 1, "itoa")?;
            match a[0] {
                Value::Int(i) => Ok(Value::Str(i.to_string())),
                _ => Err(LispError::BadArgs("itoa: not an integer".into())),
            }
        }),
    );

    // --- Output (captured) ---
    let sink = out.clone();
    def(
        "princ",
        Native::new("princ", move |a| {
            let v = a.first().cloned().unwrap_or(Value::Nil);
            sink.borrow_mut().push_str(&princ_string(&v));
            Ok(v)
        }),
    );
    let sink = out.clone();
    def(
        "print",
        Native::new("print", move |a| {
            let v = a.first().cloned().unwrap_or(Value::Nil);
            let mut s = sink.borrow_mut();
            s.push('\n');
            s.push_str(&v.to_string());
            s.push(' ');
            Ok(v)
        }),
    );
    let sink = out.clone();
    def(
        "terpri",
        Native::new("terpri", move |_a| {
            sink.borrow_mut().push('\n');
            Ok(Value::Nil)
        }),
    );
}

/// `princ` prints strings without their quotes; everything else like `print`.
fn princ_string(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        other => other.to_string(),
    }
}

fn boolean(b: bool) -> Value {
    if b {
        Value::T
    } else {
        Value::Nil
    }
}

fn want(args: &[Value], n: usize, who: &str) -> Result<(), LispError> {
    if args.len() == n {
        Ok(())
    } else {
        Err(LispError::BadArgs(format!(
            "{who} expects {n} argument(s), got {}",
            args.len()
        )))
    }
}

/// A list value's items; `nil` is the empty list, any atom is a one-element view.
/// A dotted chain is flattened, its final atom becoming the last element, so
/// `(k . v)` reads as `[k, v]` — enough for `assoc`, `length`, and friends.
fn as_list(v: &Value) -> Vec<Value> {
    match v {
        Value::List(items) => items.clone(),
        Value::Nil => Vec::new(),
        Value::Pair(_) => {
            let mut out = Vec::new();
            let mut cur = v.clone();
            loop {
                match cur {
                    Value::Pair(p) => {
                        out.push(p.0.clone());
                        cur = p.1.clone();
                    }
                    Value::List(items) => {
                        out.extend(items);
                        break;
                    }
                    Value::Nil => break,
                    other => {
                        out.push(other);
                        break;
                    }
                }
            }
            out
        }
        other => vec![other.clone()],
    }
}

/// `car`: the head of a list or pair; `nil` for anything else.
fn car(v: &Value) -> Value {
    match v {
        Value::List(items) => items.first().cloned().unwrap_or(Value::Nil),
        Value::Pair(p) => p.0.clone(),
        _ => Value::Nil,
    }
}

/// `cdr`: the tail of a list (as a list) or pair (its raw cdr); `nil` otherwise.
fn cdr(v: &Value) -> Value {
    match v {
        Value::List(items) if items.len() > 1 => Value::List(items[1..].to_vec()),
        Value::List(_) => Value::Nil,
        Value::Pair(p) => p.1.clone(),
        _ => Value::Nil,
    }
}

/// True when every argument is an integer (so the result stays an integer).
fn all_int(args: &[Value]) -> bool {
    args.iter().all(|a| matches!(a, Value::Int(_)))
}

fn num(v: &Value, who: &str) -> Result<f64, LispError> {
    v.as_f64()
        .ok_or_else(|| LispError::BadArgs(format!("{who}: not a number: {v}")))
}

/// Fold `+`/`*` from an identity; integer result unless a real appears.
fn fold_num(args: &[Value], id: f64, op: fn(f64, f64) -> f64) -> Result<Value, LispError> {
    let mut acc = id;
    for a in args {
        acc = op(acc, num(a, "arith")?);
    }
    Ok(number(acc, all_int(args)))
}

/// `min`/`max` need at least one argument and no identity.
fn fold_num1(args: &[Value], who: &str, op: fn(f64, f64) -> f64) -> Result<Value, LispError> {
    let mut it = args.iter();
    let first = it
        .next()
        .ok_or_else(|| LispError::BadArgs(format!("{who}: needs an argument")))?;
    let mut acc = num(first, who)?;
    for a in it {
        acc = op(acc, num(a, who)?);
    }
    Ok(number(acc, all_int(args)))
}

fn sub(args: &[Value]) -> Result<Value, LispError> {
    match args {
        [] => Ok(Value::Int(0)),
        [one] => Ok(number(-num(one, "-")?, all_int(args))),
        [first, rest @ ..] => {
            let mut acc = num(first, "-")?;
            for a in rest {
                acc -= num(a, "-")?;
            }
            Ok(number(acc, all_int(args)))
        }
    }
}

fn div(args: &[Value]) -> Result<Value, LispError> {
    let [first, rest @ ..] = args else {
        return Err(LispError::BadArgs("/: needs at least one argument".into()));
    };
    let mut acc = num(first, "/")?;
    for a in rest {
        let d = num(a, "/")?;
        if d == 0.0 {
            return Err(LispError::Runtime("division by zero".into()));
        }
        acc /= d;
    }
    // Integer division truncates toward zero when all operands are integers.
    Ok(number(acc, all_int(args)))
}

fn one_arg_num(
    args: &[Value],
    who: &str,
    real: fn(f64) -> f64,
    int: fn(i64) -> i64,
) -> Result<Value, LispError> {
    want(args, 1, who)?;
    match args[0] {
        Value::Int(i) => Ok(Value::Int(int(i))),
        Value::Real(r) => Ok(Value::Real(real(r))),
        _ => Err(LispError::BadArgs(format!("{who}: not a number"))),
    }
}

fn int_binop(args: &[Value], who: &str, op: fn(i64, i64) -> i64) -> Result<Value, LispError> {
    want(args, 2, who)?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => {
            if *b == 0 {
                Err(LispError::Runtime(format!("{who}: division by zero")))
            } else {
                Ok(Value::Int(op(*a, *b)))
            }
        }
        _ => Err(LispError::BadArgs(format!("{who}: needs two integers"))),
    }
}

/// Build a number: integer if `keep_int`, else real.
fn number(value: f64, keep_int: bool) -> Value {
    if keep_int {
        Value::Int(value as i64)
    } else {
        Value::Real(value)
    }
}

/// Chained numeric comparison: `(< 1 2 3)` is true when each is less than the
/// next. Returns `t`/`nil`.
fn compare(
    args: &[Value],
    who: &str,
    ok: fn(std::cmp::Ordering) -> bool,
) -> Result<Value, LispError> {
    if args.len() < 2 {
        return Err(LispError::BadArgs(format!("{who}: needs two arguments")));
    }
    for pair in args.windows(2) {
        // `=` also compares strings and symbols; ordering ops are numeric.
        let ord = match (&pair[0], &pair[1]) {
            (Value::Str(a), Value::Str(b)) => a.cmp(b),
            _ => {
                let x = num(&pair[0], who)?;
                let y = num(&pair[1], who)?;
                x.partial_cmp(&y)
                    .ok_or_else(|| LispError::Runtime(format!("{who}: uncomparable (NaN)")))?
            }
        };
        if !ok(ord) {
            return Ok(Value::Nil);
        }
    }
    Ok(Value::T)
}
