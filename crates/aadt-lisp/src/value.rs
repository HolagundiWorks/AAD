//! The LISP value type and its errors.

use std::fmt;
use std::rc::Rc;

use crate::eval::Env;

/// A LISP value. AutoLISP distinguishes integers from reals, uses `nil` for both
/// false and the empty list, and treats any non-`nil` value as true; `t` is the
/// canonical truth symbol. Proper lists are Vec-backed for speed; an improper
/// (dotted) list ends in a [`Pair`](Value::Pair).
#[derive(Clone)]
pub enum Value {
    /// `nil` — false and the empty list.
    Nil,
    /// `t` — canonical truth.
    T,
    Int(i64),
    Real(f64),
    Str(String),
    /// A symbol (variable / function name).
    Symbol(String),
    /// A proper list.
    List(Vec<Value>),
    /// A dotted pair `(car . cdr)` — the cdr is not a proper list. Built by
    /// `cons` onto a non-list (e.g. `(cons 0 "LINE")`) and used for association
    /// entries.
    Pair(Rc<(Value, Value)>),
    /// A user function created by `defun` / `lambda`, capturing its defining env.
    Lambda(Rc<Lambda>),
    /// A built-in or host-provided function.
    Native(Rc<Native>),
}

/// `cons` semantics, shared by the reader and the `cons` builtin. Consing onto a
/// list or `nil` yields a proper list; onto anything else, a dotted [`Pair`].
///
/// [`Pair`]: Value::Pair
pub fn cons(head: Value, tail: Value) -> Value {
    match tail {
        Value::List(mut v) => {
            v.insert(0, head);
            Value::List(v)
        }
        Value::Nil => Value::List(vec![head]),
        other => Value::Pair(Rc::new((head, other))),
    }
}

/// A user-defined function. AutoLISP's parameter list allows locals after `/`
/// (bound to `nil`) and this dialect adds `&rest` for a variadic tail.
pub struct Lambda {
    /// Required positional parameters.
    pub params: Vec<String>,
    /// A trailing `&rest` parameter capturing the extra arguments as a list.
    pub rest: Option<String>,
    /// Locals declared after `/`, bound to `nil` in the call scope.
    pub locals: Vec<String>,
    pub body: Vec<Value>,
    pub env: Env,
    pub name: Option<String>,
}

/// A function implemented in Rust. Arguments arrive already evaluated.
pub struct Native {
    pub name: String,
    #[allow(clippy::type_complexity)]
    pub func: Box<dyn Fn(&[Value]) -> Result<Value, LispError>>,
}

impl Native {
    /// Wrap a Rust function as a callable [`Value::Native`]. Returns a `Value`
    /// (not `Self`) by design — a native is only ever handled as a value.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        name: impl Into<String>,
        func: impl Fn(&[Value]) -> Result<Value, LispError> + 'static,
    ) -> Value {
        Value::Native(Rc::new(Native {
            name: name.into(),
            func: Box::new(func),
        }))
    }
}

impl Value {
    /// AutoLISP truthiness: everything except `nil` is true.
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Nil)
    }

    /// A number as `f64`, for arithmetic that mixes ints and reals.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Real(r) => Some(*r),
            _ => None,
        }
    }

    /// The type name, for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::T => "t",
            Value::Int(_) => "int",
            Value::Real(_) => "real",
            Value::Str(_) => "string",
            Value::Symbol(_) => "symbol",
            Value::List(_) => "list",
            Value::Pair(_) => "pair",
            Value::Lambda(_) => "function",
            Value::Native(_) => "subr",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::T => write!(f, "t"),
            Value::Int(i) => write!(f, "{i}"),
            // AutoLISP prints reals with a leading/trailing digit; Rust's default
            // is close enough and round-trips.
            Value::Real(r) => write!(f, "{r}"),
            Value::Str(s) => write!(f, "\"{s}\""),
            Value::Symbol(s) => write!(f, "{s}"),
            Value::List(items) => {
                write!(f, "(")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, ")")
            }
            // Walk the cons chain, merging a `List`/`Nil` tail into a proper
            // `(a b c)` and closing an atom tail as `(a b . c)`.
            Value::Pair(p) => {
                write!(f, "({}", p.0)?;
                let mut tail = &p.1;
                loop {
                    match tail {
                        Value::Nil => break,
                        Value::List(items) => {
                            for v in items {
                                write!(f, " {v}")?;
                            }
                            break;
                        }
                        Value::Pair(next) => {
                            write!(f, " {}", next.0)?;
                            tail = &next.1;
                        }
                        other => {
                            write!(f, " . {other}")?;
                            break;
                        }
                    }
                }
                write!(f, ")")
            }
            Value::Lambda(l) => match &l.name {
                Some(n) => write!(f, "#<function {n}>"),
                None => write!(f, "#<lambda>"),
            },
            Value::Native(n) => write!(f, "#<subr {}>", n.name),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

/// Structural equality for values, matching AutoLISP's `equal`. Functions are
/// only equal to themselves by identity, which for proper-list scripts never
/// matters; numbers compare across int/real by value.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::T, Value::T) => true,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Symbol(a), Value::Symbol(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Pair(a), Value::Pair(b)) => a.0 == b.0 && a.1 == b.1,
            (Value::Lambda(a), Value::Lambda(b)) => Rc::ptr_eq(a, b),
            (Value::Native(a), Value::Native(b)) => Rc::ptr_eq(a, b),
            // Numeric equality bridges int and real.
            _ => match (self.as_f64(), other.as_f64()) {
                (Some(x), Some(y)) => x == y,
                _ => false,
            },
        }
    }
}

/// An error raised while reading or evaluating LISP.
#[derive(Debug, Clone, PartialEq)]
pub enum LispError {
    /// The source could not be parsed.
    Parse(String),
    /// A symbol was used before being bound.
    Unbound(String),
    /// A value was used where a callable was expected.
    NotCallable(String),
    /// Wrong argument count or type.
    BadArgs(String),
    /// Division by zero, and similar runtime faults.
    Runtime(String),
}

impl fmt::Display for LispError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LispError::Parse(m) => write!(f, "parse error: {m}"),
            LispError::Unbound(s) => write!(f, "unbound symbol: {s}"),
            LispError::NotCallable(s) => write!(f, "not callable: {s}"),
            LispError::BadArgs(m) => write!(f, "bad arguments: {m}"),
            LispError::Runtime(m) => write!(f, "runtime error: {m}"),
        }
    }
}

impl std::error::Error for LispError {}
