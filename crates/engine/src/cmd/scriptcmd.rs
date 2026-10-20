//! SCRIPT, MACRO and a small AutoLISP evaluator that can call engine commands.

use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

const MAX_SCRIPT: usize = 2_000_000;
const MAX_LISP: usize = 200_000;
const MAX_LISP_DEPTH: usize = 64;
const MAX_LISP_STEPS: usize = 50_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("script", "Run Script", run_script)
            .menu(&["Tools", "Run Script..."])
            .alias(&["scr"])
            .params("{text? | path?: .scr file} → {lines} (same language as `aad-cli run --script`)")
            .noundo(),
        CommandSpec::new("macro", "Run Macro", run_macro)
            .menu(&["Tools", "Macro"])
            .params("{text} → {lines} (in-memory script; spaces separate inputs)")
            .noundo(),
        CommandSpec::new("lisp", "AutoLISP", run_lisp)
            .menu(&["Tools", "AutoLISP"])
            .alias(&["autolisp"])
            .params(
                "{text | expr: AutoLISP subset — numbers, strings, (+ - * /), setq, getvar, setvar, princ, command} → {result, printed}",
            )
            .noundo(),
    ]
}

fn run_script(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "script";
    let text = if let Some(t) = str_param(p, "text") {
        t.to_string()
    } else if let Some(path) = str_param(p, "path") {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = path;
            return Err(bad(CMD, "reading a script file is not available in this build"));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let meta = std::fs::metadata(path).map_err(|e| bad(CMD, format!("{path}: {e}")))?;
            if meta.len() > MAX_SCRIPT as u64 {
                return Err(bad(CMD, format!("script file larger than {MAX_SCRIPT} bytes")));
            }
            std::fs::read_to_string(path).map_err(|e| bad(CMD, format!("{path}: {e}")))?
        }
    } else {
        return Err(bad(CMD, "give `text` or `path`"));
    };
    if text.len() > MAX_SCRIPT {
        return Err(bad(CMD, format!("script longer than {MAX_SCRIPT} characters")));
    }
    let lines = text.lines().count();
    s.script(&text).map_err(|e| bad(CMD, e.to_string()))?;
    Ok(json!({ "lines": lines }))
}

fn run_macro(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "macro";
    let text = str_param(p, "text").ok_or_else(|| bad(CMD, "`text` is required"))?;
    if text.len() > MAX_SCRIPT {
        return Err(bad(CMD, format!("macro longer than {MAX_SCRIPT} characters")));
    }
    let lines = text.lines().count().max(1);
    s.script(text).map_err(|e| bad(CMD, e.to_string()))?;
    Ok(json!({ "lines": lines }))
}

fn run_lisp(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "lisp";
    let text = str_param(p, "text").or_else(|| str_param(p, "expr")).ok_or_else(|| bad(CMD, "`text` or `expr` is required"))?;
    if text.len() > MAX_LISP {
        return Err(bad(CMD, format!("expression longer than {MAX_LISP} characters")));
    }
    let mut env = LispEnv::default();
    let mut steps = 0usize;
    let val = eval_forms(s, &mut env, text, 0, &mut steps).map_err(|e| bad(CMD, e))?;
    Ok(json!({ "result": val.to_json(), "printed": env.printed }))
}

// ---------------- AutoLISP subset ----------------

#[derive(Clone, Debug)]
enum Atom {
    Nil,
    Num(f64),
    Str(String),
    Sym(String),
    List(Vec<Atom>),
}

impl Atom {
    fn to_json(&self) -> Value {
        match self {
            Atom::Nil => Value::Null,
            Atom::Num(n) => json!(n),
            Atom::Str(s) | Atom::Sym(s) => json!(s),
            Atom::List(a) => Value::Array(a.iter().map(Atom::to_json).collect()),
        }
    }
    fn as_num(&self) -> Option<f64> {
        match self {
            Atom::Num(n) if n.is_finite() => Some(*n),
            _ => None,
        }
    }
    fn as_str(&self) -> Option<&str> {
        match self {
            Atom::Str(s) | Atom::Sym(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Default)]
struct LispEnv {
    vars: std::collections::BTreeMap<String, Atom>,
    printed: String,
}

fn eval_forms(s: &mut Session, env: &mut LispEnv, src: &str, depth: usize, steps: &mut usize) -> std::result::Result<Atom, String> {
    let forms = parse_all(src)?;
    let mut last = Atom::Nil;
    for f in forms {
        last = eval(s, env, &f, depth, steps)?;
    }
    Ok(last)
}

fn eval(s: &mut Session, env: &mut LispEnv, a: &Atom, depth: usize, steps: &mut usize) -> std::result::Result<Atom, String> {
    *steps = steps.saturating_add(1);
    if *steps > MAX_LISP_STEPS || depth > MAX_LISP_DEPTH {
        return Err("expression too deep or too long".into());
    }
    match a {
        Atom::Sym(name) => Ok(env.vars.get(name).cloned().unwrap_or_else(|| Atom::Sym(name.clone()))),
        Atom::List(items) if items.is_empty() => Ok(Atom::Nil),
        Atom::List(items) => {
            let head = match items.first() {
                Some(Atom::Sym(h)) => h.as_str(),
                _ => return Err("a list must start with a function name".into()),
            };
            let args = &items[1..];
            match head {
                "+" | "-" | "*" | "/" => {
                    let mut nums = Vec::new();
                    for a in args {
                        let v = eval(s, env, a, depth + 1, steps)?;
                        nums.push(v.as_num().ok_or_else(|| format!("`{head}` needs numbers"))?);
                    }
                    if nums.is_empty() {
                        return Ok(Atom::Num(if head == "*" { 1.0 } else { 0.0 }));
                    }
                    let n = match head {
                        "+" => nums.iter().sum(),
                        "*" => nums.iter().product(),
                        "-" => {
                            if nums.len() == 1 {
                                -nums[0]
                            } else {
                                nums[1..].iter().fold(nums[0], |a, b| a - b)
                            }
                        }
                        "/" => {
                            let mut a = nums[0];
                            for b in &nums[1..] {
                                if *b == 0.0 {
                                    return Err("division by zero".into());
                                }
                                a /= b;
                            }
                            a
                        }
                        _ => 0.0,
                    };
                    if !n.is_finite() {
                        return Err("result is not finite".into());
                    }
                    Ok(Atom::Num(n))
                }
                "setq" => {
                    if args.len() < 2 || args.len() % 2 != 0 {
                        return Err("setq needs name/value pairs".into());
                    }
                    let mut last = Atom::Nil;
                    for pair in args.as_chunks::<2>().0 {
                        let name = pair[0].as_str().ok_or_else(|| "setq name must be a symbol".to_string())?.to_string();
                        last = eval(s, env, &pair[1], depth + 1, steps)?;
                        env.vars.insert(name, last.clone());
                    }
                    Ok(last)
                }
                "princ" => {
                    let v = args.first().map(|a| eval(s, env, a, depth + 1, steps)).transpose()?.unwrap_or(Atom::Nil);
                    let t = match &v {
                        Atom::Str(s) => s.clone(),
                        Atom::Num(n) => n.to_string(),
                        Atom::Nil => String::new(),
                        other => format!("{other:?}"),
                    };
                    if env.printed.len() + t.len() <= MAX_LISP {
                        env.printed.push_str(&t);
                    }
                    Ok(v)
                }
                "getvar" => {
                    let name = eval(s, env, args.first().ok_or("getvar needs a name")?, depth + 1, steps)?;
                    let key = name.as_str().ok_or("getvar needs a string or symbol")?.to_ascii_uppercase();
                    let d = s.doc().map_err(|e| e.to_string())?;
                    match d.header.get(&key) {
                        Some(aad_doc::HVal::Int(i)) => Ok(Atom::Num(*i as f64)),
                        Some(aad_doc::HVal::Real(r)) if r.is_finite() => Ok(Atom::Num(*r)),
                        Some(aad_doc::HVal::Str(t)) => Ok(Atom::Str(t.clone())),
                        _ => Ok(Atom::Nil),
                    }
                }
                "setvar" => {
                    let name = eval(s, env, args.first().ok_or("setvar needs a name")?, depth + 1, steps)?;
                    let val = eval(s, env, args.get(1).ok_or("setvar needs a value")?, depth + 1, steps)?;
                    let key = name.as_str().ok_or("setvar name")?.to_ascii_uppercase();
                    let d = s.doc_mut().map_err(|e| e.to_string())?;
                    match val {
                        Atom::Num(n) if n.is_finite() => {
                            if n.fract() == 0.0 && n.abs() <= i64::MAX as f64 {
                                d.header.set_i64(&key, n as i64);
                            } else {
                                d.header.set(&key, aad_doc::HVal::Real(n));
                            }
                        }
                        Atom::Str(t) => d.header.set(&key, aad_doc::HVal::Str(t)),
                        _ => return Err("setvar value must be a number or string".into()),
                    }
                    Ok(Atom::Nil)
                }
                "command" => {
                    // (command "LINE" "0,0" "10,0" "") → script lines
                    let mut parts = Vec::new();
                    for a in args {
                        let v = eval(s, env, a, depth + 1, steps)?;
                        match v {
                            Atom::Nil => parts.push(String::new()),
                            Atom::Str(t) if t.is_empty() => parts.push(String::new()),
                            Atom::Str(t) | Atom::Sym(t) => parts.push(t),
                            Atom::Num(n) => parts.push(n.to_string()),
                            Atom::List(_) => return Err("command arguments must be atoms".into()),
                        }
                    }
                    let script = parts.join("\n");
                    s.script(&script).map_err(|e| e.to_string())?;
                    Ok(Atom::Nil)
                }
                other => Err(format!("unknown function `{other}` (this build supports +, -, *, /, setq, princ, getvar, setvar, command)")),
            }
        }
        other => Ok(other.clone()),
    }
}

fn parse_all(src: &str) -> std::result::Result<Vec<Atom>, String> {
    let mut chars: Vec<char> = src.chars().take(MAX_LISP).collect();
    // Strip ; comments to end of line.
    let mut cleaned = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ';' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else {
            cleaned.push(chars[i]);
            i += 1;
        }
    }
    chars = cleaned.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        skip_ws(&chars, &mut i);
        if i >= chars.len() {
            break;
        }
        out.push(parse_one(&chars, &mut i, 0)?);
        if out.len() > 1000 {
            return Err("too many top-level forms".into());
        }
    }
    Ok(out)
}

fn skip_ws(c: &[char], i: &mut usize) {
    while *i < c.len() && c[*i].is_whitespace() {
        *i += 1;
    }
}

fn parse_one(c: &[char], i: &mut usize, depth: usize) -> std::result::Result<Atom, String> {
    if depth > MAX_LISP_DEPTH {
        return Err("expression too nested".into());
    }
    skip_ws(c, i);
    let Some(&ch) = c.get(*i) else {
        return Err("unexpected end of expression".into());
    };
    if ch == '(' {
        *i += 1;
        let mut items = Vec::new();
        loop {
            skip_ws(c, i);
            let Some(&n) = c.get(*i) else {
                return Err("unclosed list".into());
            };
            if n == ')' {
                *i += 1;
                break;
            }
            items.push(parse_one(c, i, depth + 1)?);
            if items.len() > 1000 {
                return Err("list too long".into());
            }
        }
        Ok(Atom::List(items))
    } else if ch == '"' {
        *i += 1;
        let mut s = String::new();
        while *i < c.len() && c[*i] != '"' {
            if c[*i] == '\\' && *i + 1 < c.len() {
                *i += 1;
            }
            s.push(c[*i]);
            *i += 1;
            if s.len() > MAX_LISP {
                return Err("string too long".into());
            }
        }
        if *i >= c.len() {
            return Err("unclosed string".into());
        }
        *i += 1;
        Ok(Atom::Str(s))
    } else {
        let start = *i;
        while *i < c.len() && !c[*i].is_whitespace() && c[*i] != '(' && c[*i] != ')' {
            *i += 1;
        }
        let tok: String = c[start..*i].iter().collect();
        if tok.eq_ignore_ascii_case("nil") {
            Ok(Atom::Nil)
        } else if let Ok(n) = tok.parse::<f64>() {
            if n.is_finite() { Ok(Atom::Num(n)) } else { Err("number is not finite".into()) }
        } else if tok.is_empty() {
            Err("empty token".into())
        } else {
            Ok(Atom::Sym(tok))
        }
    }
}
