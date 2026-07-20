//! # aadt-lisp — Embedded LISP engine (substrate, spec §4.4)
//!
//! An AutoLISP-flavoured interpreter so existing firm routines import with
//! minimal changes: case-insensitive symbols, `nil`/`t`, integer vs. real
//! arithmetic, the familiar special forms (`setq`, `defun`, `if`, `cond`,
//! `while`, `repeat`, `foreach`, `and`, `or`, `lambda`), and a standard library
//! of arithmetic, list, predicate, and string subrs.
//!
//! Its distinguishing feature — spec §4.4's extensibility point unique to AADT —
//! is **read access to AADT Engine outputs**: [`host::install_document`] adds
//! `aadt-*` functions that let a script query the document, the detected
//! elements, and the takeoff. See [`host`].
//!
//! ```
//! # use aadt_lisp::Interpreter;
//! let mut lisp = Interpreter::new();
//! lisp.eval_str("(defun sq (x) (* x x))").unwrap();
//! assert_eq!(lisp.eval_str("(sq 9)").unwrap().to_string(), "81");
//! ```
//!
//! **Scope:** proper (Vec-backed) lists — dotted pairs and `&rest`/optional
//! lambda arguments are deferred; association data uses two-element lists, read
//! with `assoc`/`cadr`. The bridge is read-only; entity creation from LISP builds
//! on the command registry in a later increment.

mod builtins;
pub mod eval;
pub mod host;
mod reader;
mod value;

use std::cell::RefCell;
use std::rc::Rc;

use eval::{eval, new_env, Env};

pub use value::{LispError, Value};

/// A LISP interpreter: a global environment preloaded with the standard library,
/// plus a buffer capturing `princ`/`print` output.
pub struct Interpreter {
    env: Env,
    output: Rc<RefCell<String>>,
}

impl Interpreter {
    /// A new interpreter with the standard library installed.
    pub fn new() -> Self {
        let env = new_env();
        let output = Rc::new(RefCell::new(String::new()));
        builtins::install(&env, &output);
        Self { env, output }
    }

    /// The global environment, for a host to register extra functions into
    /// (e.g. [`host::install_document`]).
    pub fn env(&self) -> &Env {
        &self.env
    }

    /// Read and evaluate every top-level form in `src`, returning the last value.
    /// An empty program yields `nil`.
    pub fn eval_str(&mut self, src: &str) -> Result<Value, LispError> {
        let forms = reader::read_all(src)?;
        let mut last = Value::Nil;
        for form in &forms {
            last = eval(form, &self.env)?;
        }
        Ok(last)
    }

    /// The text emitted by `princ`/`print` so far.
    pub fn output(&self) -> String {
        self.output.borrow().clone()
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> String {
        Interpreter::new().eval_str(src).unwrap().to_string()
    }

    #[test]
    fn arithmetic_promotes_int_to_real() {
        assert_eq!(run("(+ 1 2 3)"), "6");
        assert_eq!(run("(- 10 3 2)"), "5");
        assert_eq!(run("(* 2 3 4)"), "24");
        assert_eq!(run("(/ 12 3)"), "4");
        assert_eq!(run("(+ 1 2.5)"), "3.5", "a real operand yields a real");
        assert_eq!(run("(1+ 4)"), "5");
        assert_eq!(run("(max 3 9 5)"), "9");
    }

    #[test]
    fn division_by_zero_is_an_error() {
        assert!(matches!(
            Interpreter::new().eval_str("(/ 1 0)"),
            Err(LispError::Runtime(_))
        ));
    }

    #[test]
    fn setq_and_symbol_lookup() {
        assert_eq!(run("(setq x 10) (setq y 20) (+ x y)"), "30");
    }

    #[test]
    fn comparisons_are_chained() {
        assert_eq!(run("(< 1 2 3)"), "t");
        assert_eq!(run("(< 1 3 2)"), "nil");
        assert_eq!(run("(= 5 5)"), "t");
        assert_eq!(run("(> 9 4)"), "t");
    }

    #[test]
    fn if_cond_and_logic() {
        assert_eq!(run("(if (> 3 2) \"yes\" \"no\")"), "\"yes\"");
        assert_eq!(run("(cond ((= 1 2) 'a) ((= 1 1) 'b) (t 'c))"), "b");
        assert_eq!(run("(and 1 2 3)"), "3");
        assert_eq!(run("(and 1 nil 3)"), "nil");
        assert_eq!(run("(or nil nil 7)"), "7");
    }

    #[test]
    fn defun_supports_recursion() {
        let fact = "(defun fact (n) (if (<= n 1) 1 (* n (fact (1- n))))) (fact 5)";
        assert_eq!(run(fact), "120");
    }

    #[test]
    fn lambda_closes_over_its_environment() {
        // A returned lambda captures `n`.
        let src = "(defun adder (n) (lambda (x) (+ x n))) (setq add5 (adder 5)) (add5 10)";
        assert_eq!(run(src), "15");
    }

    #[test]
    fn while_repeat_and_foreach_iterate() {
        assert_eq!(
            run("(setq i 0 s 0) (while (< i 5) (setq s (+ s i) i (1+ i))) s"),
            "10"
        );
        assert_eq!(run("(setq c 0) (repeat 3 (setq c (1+ c))) c"), "3");
        assert_eq!(
            run("(setq total 0) (foreach n '(1 2 3 4) (setq total (+ total n))) total"),
            "10"
        );
    }

    #[test]
    fn list_operations() {
        assert_eq!(run("(car '(1 2 3))"), "1");
        assert_eq!(run("(cdr '(1 2 3))"), "(2 3)");
        assert_eq!(run("(cons 0 '(1 2))"), "(0 1 2)");
        assert_eq!(run("(length '(a b c))"), "3");
        assert_eq!(run("(nth 1 '(a b c))"), "b");
        assert_eq!(run("(reverse '(1 2 3))"), "(3 2 1)");
        assert_eq!(run("(append '(1 2) '(3 4))"), "(1 2 3 4)");
        assert_eq!(
            run("(assoc 8 '((0 \"LINE\") (8 \"A-WALL\")))"),
            "(8 \"A-WALL\")"
        );
    }

    #[test]
    fn strings() {
        assert_eq!(run("(strcat \"a\" \"b\" \"c\")"), "\"abc\"");
        assert_eq!(run("(strlen \"hello\")"), "5");
        assert_eq!(run("(itoa 42)"), "\"42\"");
    }

    #[test]
    fn princ_output_is_captured() {
        let mut lisp = Interpreter::new();
        lisp.eval_str("(princ \"hello \") (princ 42)").unwrap();
        assert_eq!(lisp.output(), "hello 42");
    }

    #[test]
    fn unbound_symbol_and_bad_calls_error() {
        assert!(matches!(
            Interpreter::new().eval_str("nope"),
            Err(LispError::Unbound(_))
        ));
        assert!(matches!(
            Interpreter::new().eval_str("(1 2 3)"),
            Err(LispError::NotCallable(_))
        ));
        assert!(matches!(
            Interpreter::new().eval_str("(car)"),
            Err(LispError::BadArgs(_))
        ));
    }

    #[test]
    fn a_realistic_macro_sums_a_list_of_areas() {
        // The kind of routine a firm would actually write.
        let src = "\
            (defun sum (lst)
              (if (null lst) 0 (+ (car lst) (sum (cdr lst)))))
            (sum '(12.0 30.0 18.5))";
        assert_eq!(run(src), "60.5");
    }
}
