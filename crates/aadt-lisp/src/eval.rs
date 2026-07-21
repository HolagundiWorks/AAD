//! The evaluator — environments, special forms, and function application.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::value::{Lambda, LispError, Value};

/// A lexical scope: its own bindings plus an optional parent.
pub struct Scope {
    vars: HashMap<String, Value>,
    parent: Option<Env>,
}

/// A shared, mutable environment handle.
pub type Env = Rc<RefCell<Scope>>;

/// A fresh root environment.
pub fn new_env() -> Env {
    Rc::new(RefCell::new(Scope {
        vars: HashMap::new(),
        parent: None,
    }))
}

/// A child environment of `parent` (used for lambda parameter scopes).
pub fn child_env(parent: &Env) -> Env {
    Rc::new(RefCell::new(Scope {
        vars: HashMap::new(),
        parent: Some(parent.clone()),
    }))
}

/// Bind `name` in `env`'s own scope, shadowing any parent binding.
pub fn define(env: &Env, name: &str, value: Value) {
    env.borrow_mut().vars.insert(name.to_string(), value);
}

/// Look `name` up through the scope chain.
fn lookup(env: &Env, name: &str) -> Option<Value> {
    let scope = env.borrow();
    if let Some(v) = scope.vars.get(name) {
        return Some(v.clone());
    }
    scope.parent.as_ref().and_then(|p| lookup(p, name))
}

/// Assign `name`: update the nearest existing binding, or define it in the root
/// scope (AutoLISP variables are global unless localised as lambda parameters).
fn assign(env: &Env, name: &str, value: Value) {
    if update_existing(env, name, &value) {
        return;
    }
    // Not bound anywhere — define it at the root.
    let mut cur = env.clone();
    loop {
        let parent = cur.borrow().parent.clone();
        match parent {
            Some(p) => cur = p,
            None => break,
        }
    }
    cur.borrow_mut().vars.insert(name.to_string(), value);
}

fn update_existing(env: &Env, name: &str, value: &Value) -> bool {
    {
        let mut scope = env.borrow_mut();
        if scope.vars.contains_key(name) {
            scope.vars.insert(name.to_string(), value.clone());
            return true;
        }
    }
    let parent = env.borrow().parent.clone();
    match parent {
        Some(p) => update_existing(&p, name, value),
        None => false,
    }
}

/// Evaluate one form.
pub fn eval(expr: &Value, env: &Env) -> Result<Value, LispError> {
    match expr {
        Value::Symbol(s) => lookup(env, s).ok_or_else(|| LispError::Unbound(s.clone())),
        Value::List(items) => eval_list(items, env),
        // Everything else is self-evaluating (numbers, strings, nil, t, functions).
        other => Ok(other.clone()),
    }
}

/// Evaluate a sequence of forms, returning the last value (`nil` if empty).
pub fn eval_body(body: &[Value], env: &Env) -> Result<Value, LispError> {
    let mut last = Value::Nil;
    for form in body {
        last = eval(form, env)?;
    }
    Ok(last)
}

fn eval_list(items: &[Value], env: &Env) -> Result<Value, LispError> {
    // A list evaluates by treating its head as an operator. `()` is nil and never
    // reaches here (the reader collapses it), so `items` is non-empty.
    let (head, args) = items.split_first().expect("non-empty list");

    if let Value::Symbol(name) = head {
        if let Some(result) = eval_special_form(name, args, env) {
            return result;
        }
    }

    let func = eval(head, env)?;
    let mut argv = Vec::with_capacity(args.len());
    for a in args {
        argv.push(eval(a, env)?);
    }
    apply(&func, &argv)
}

/// Dispatch the special forms (those that do not evaluate all their arguments).
/// Returns `None` for a name that is not a special form, so it falls through to
/// ordinary application.
fn eval_special_form(name: &str, args: &[Value], env: &Env) -> Option<Result<Value, LispError>> {
    Some(match name {
        "quote" => Ok(args.first().cloned().unwrap_or(Value::Nil)),
        "if" => eval_if(args, env),
        "setq" => eval_setq(args, env),
        "defun" => eval_defun(args, env),
        "lambda" => eval_lambda(args, env, None),
        "progn" => eval_body(args, env),
        "cond" => eval_cond(args, env),
        "while" => eval_while(args, env),
        "repeat" => eval_repeat(args, env),
        "foreach" => eval_foreach(args, env),
        "and" => eval_and(args, env),
        "or" => eval_or(args, env),
        _ => return None,
    })
}

fn eval_if(args: &[Value], env: &Env) -> Result<Value, LispError> {
    let test = args
        .first()
        .ok_or_else(|| LispError::BadArgs("if: missing test".into()))?;
    if eval(test, env)?.is_truthy() {
        eval(args.get(1).unwrap_or(&Value::Nil), env)
    } else {
        eval(args.get(2).unwrap_or(&Value::Nil), env)
    }
}

fn eval_setq(args: &[Value], env: &Env) -> Result<Value, LispError> {
    if !args.len().is_multiple_of(2) {
        return Err(LispError::BadArgs("setq: needs symbol/value pairs".into()));
    }
    let mut last = Value::Nil;
    for pair in args.chunks_exact(2) {
        let Value::Symbol(name) = &pair[0] else {
            return Err(LispError::BadArgs("setq: target is not a symbol".into()));
        };
        last = eval(&pair[1], env)?;
        assign(env, name, last.clone());
    }
    Ok(last)
}

fn eval_defun(args: &[Value], env: &Env) -> Result<Value, LispError> {
    let Some(Value::Symbol(name)) = args.first() else {
        return Err(LispError::BadArgs("defun: missing name".into()));
    };
    let lambda = eval_lambda(&args[1..], env, Some(name.clone()))?;
    define(env, name, lambda);
    Ok(Value::Symbol(name.clone()))
}

fn eval_lambda(args: &[Value], env: &Env, name: Option<String>) -> Result<Value, LispError> {
    let raw = match args.first() {
        Some(Value::List(ps)) => ps.as_slice(),
        // `(lambda () ...)` reads its empty arg list as nil.
        Some(Value::Nil) => &[],
        _ => return Err(LispError::BadArgs("lambda: missing parameter list".into())),
    };

    // Parse the parameter list: required names, then optionally `&rest <name>`
    // for a variadic tail and `/ <locals...>` for AutoLISP locals (bound to nil).
    let mut params = Vec::new();
    let mut rest = None;
    let mut locals = Vec::new();
    let mut mode = ParamMode::Required;
    for p in raw {
        let Value::Symbol(s) = p else {
            return Err(LispError::BadArgs(format!(
                "lambda: parameter is not a symbol: {p}"
            )));
        };
        match (s.as_str(), mode) {
            ("/", _) => mode = ParamMode::Local,
            ("&rest", _) => mode = ParamMode::Rest,
            (_, ParamMode::Required) => params.push(s.clone()),
            (_, ParamMode::Rest) => {
                if rest.is_some() {
                    return Err(LispError::BadArgs("lambda: multiple &rest names".into()));
                }
                rest = Some(s.clone());
            }
            (_, ParamMode::Local) => locals.push(s.clone()),
        }
    }

    Ok(Value::Lambda(Rc::new(Lambda {
        params,
        rest,
        locals,
        body: args[1..].to_vec(),
        env: env.clone(),
        name,
    })))
}

#[derive(Clone, Copy)]
enum ParamMode {
    Required,
    Rest,
    Local,
}

fn eval_cond(args: &[Value], env: &Env) -> Result<Value, LispError> {
    for clause in args {
        let Value::List(parts) = clause else {
            return Err(LispError::BadArgs("cond: clause is not a list".into()));
        };
        let Some(test) = parts.first() else {
            continue;
        };
        let tv = eval(test, env)?;
        if tv.is_truthy() {
            return if parts.len() == 1 {
                Ok(tv) // `(cond (x))` yields x when x is true.
            } else {
                eval_body(&parts[1..], env)
            };
        }
    }
    Ok(Value::Nil)
}

fn eval_while(args: &[Value], env: &Env) -> Result<Value, LispError> {
    let Some(test) = args.first() else {
        return Err(LispError::BadArgs("while: missing test".into()));
    };
    let mut last = Value::Nil;
    while eval(test, env)?.is_truthy() {
        last = eval_body(&args[1..], env)?;
    }
    Ok(last)
}

fn eval_repeat(args: &[Value], env: &Env) -> Result<Value, LispError> {
    let n = match args.first().map(|a| eval(a, env)).transpose()? {
        Some(Value::Int(n)) => n.max(0),
        _ => {
            return Err(LispError::BadArgs(
                "repeat: count must be an integer".into(),
            ))
        }
    };
    let mut last = Value::Nil;
    for _ in 0..n {
        last = eval_body(&args[1..], env)?;
    }
    Ok(last)
}

fn eval_foreach(args: &[Value], env: &Env) -> Result<Value, LispError> {
    let Some(Value::Symbol(var)) = args.first() else {
        return Err(LispError::BadArgs("foreach: missing loop variable".into()));
    };
    let list = eval(args.get(1).unwrap_or(&Value::Nil), env)?;
    let items = match list {
        Value::List(v) => v,
        Value::Nil => Vec::new(),
        other => return Err(LispError::BadArgs(format!("foreach: not a list: {other}"))),
    };
    let mut last = Value::Nil;
    for item in items {
        assign(env, var, item);
        last = eval_body(&args[2..], env)?;
    }
    Ok(last)
}

fn eval_and(args: &[Value], env: &Env) -> Result<Value, LispError> {
    let mut last = Value::T;
    for a in args {
        last = eval(a, env)?;
        if !last.is_truthy() {
            return Ok(Value::Nil);
        }
    }
    Ok(last)
}

fn eval_or(args: &[Value], env: &Env) -> Result<Value, LispError> {
    for a in args {
        let v = eval(a, env)?;
        if v.is_truthy() {
            return Ok(v);
        }
    }
    Ok(Value::Nil)
}

/// Apply a callable to already-evaluated arguments.
pub fn apply(func: &Value, args: &[Value]) -> Result<Value, LispError> {
    match func {
        Value::Native(n) => (n.func)(args),
        Value::Lambda(l) => {
            let who = l.name.as_deref().unwrap_or("lambda");
            // With `&rest`, arity is a minimum; without, it is exact.
            if l.rest.is_some() {
                if args.len() < l.params.len() {
                    return Err(LispError::BadArgs(format!(
                        "{who} expects at least {} argument(s), got {}",
                        l.params.len(),
                        args.len()
                    )));
                }
            } else if args.len() != l.params.len() {
                return Err(LispError::BadArgs(format!(
                    "{who} expects {} argument(s), got {}",
                    l.params.len(),
                    args.len()
                )));
            }

            let scope = child_env(&l.env);
            for (p, a) in l.params.iter().zip(args) {
                define(&scope, p, a.clone());
            }
            if let Some(rest) = &l.rest {
                let extra = &args[l.params.len()..];
                let value = if extra.is_empty() {
                    Value::Nil
                } else {
                    Value::List(extra.to_vec())
                };
                define(&scope, rest, value);
            }
            // AutoLISP locals start as nil.
            for loc in &l.locals {
                define(&scope, loc, Value::Nil);
            }
            eval_body(&l.body, &scope)
        }
        other => Err(LispError::NotCallable(other.to_string())),
    }
}
