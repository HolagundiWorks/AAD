//! The reader — turns source text into [`Value`] forms.
//!
//! A hand-written tokenizer and recursive-descent parser for S-expressions:
//! parentheses for lists, `'` for quote, `"…"` for strings (with `\"`, `\n`,
//! `\t`, `\\` escapes), `;` to end-of-line for comments, and bare tokens read as
//! integers, reals, or symbols. Symbols are case-insensitive in AutoLISP; they
//! are folded to lower case here so `SETQ` and `setq` are the same.

use crate::value::{LispError, Value};

/// Parse a whole program into its top-level forms.
pub fn read_all(src: &str) -> Result<Vec<Value>, LispError> {
    let tokens = tokenize(src)?;
    let mut parser = Parser { tokens, pos: 0 };
    let mut forms = Vec::new();
    while parser.pos < parser.tokens.len() {
        forms.push(parser.parse_form()?);
    }
    Ok(forms)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Open,
    Close,
    Quote,
    Str(String),
    Atom(String),
}

fn tokenize(src: &str) -> Result<Vec<Token>, LispError> {
    let mut tokens = Vec::new();
    let mut chars = src.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() => {
                chars.next();
            }
            ';' => {
                // Comment to end of line.
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '(' => {
                chars.next();
                tokens.push(Token::Open);
            }
            ')' => {
                chars.next();
                tokens.push(Token::Close);
            }
            '\'' => {
                chars.next();
                tokens.push(Token::Quote);
            }
            '"' => {
                chars.next();
                let mut s = String::new();
                loop {
                    match chars.next() {
                        None => return Err(LispError::Parse("unterminated string".into())),
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some('"') => s.push('"'),
                            Some('\\') => s.push('\\'),
                            Some(other) => s.push(other),
                            None => return Err(LispError::Parse("unterminated escape".into())),
                        },
                        Some(other) => s.push(other),
                    }
                }
                tokens.push(Token::Str(s));
            }
            _ => {
                // A bare atom runs until whitespace, a paren, a quote, or a comment.
                let mut a = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || matches!(c, '(' | ')' | '\'' | ';' | '"') {
                        break;
                    }
                    a.push(c);
                    chars.next();
                }
                tokens.push(Token::Atom(a));
            }
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn parse_form(&mut self) -> Result<Value, LispError> {
        let tok = self
            .tokens
            .get(self.pos)
            .ok_or_else(|| LispError::Parse("unexpected end of input".into()))?
            .clone();
        self.pos += 1;
        match tok {
            Token::Open => {
                let mut items = Vec::new();
                loop {
                    match self.tokens.get(self.pos) {
                        None => return Err(LispError::Parse("unclosed '('".into())),
                        Some(Token::Close) => {
                            self.pos += 1;
                            break;
                        }
                        Some(_) => items.push(self.parse_form()?),
                    }
                }
                if items.is_empty() {
                    Ok(Value::Nil) // `()` is nil.
                } else {
                    Ok(Value::List(items))
                }
            }
            Token::Close => Err(LispError::Parse("unexpected ')'".into())),
            // `'x` reads as `(quote x)`.
            Token::Quote => Ok(Value::List(vec![
                Value::Symbol("quote".into()),
                self.parse_form()?,
            ])),
            Token::Str(s) => Ok(Value::Str(s)),
            Token::Atom(a) => Ok(atom(&a)),
        }
    }
}

/// Classify a bare atom as nil, t, an integer, a real, or a (lower-cased) symbol.
fn atom(a: &str) -> Value {
    let lower = a.to_ascii_lowercase();
    match lower.as_str() {
        "nil" => return Value::Nil,
        "t" => return Value::T,
        _ => {}
    }
    if let Ok(i) = a.parse::<i64>() {
        return Value::Int(i);
    }
    // A real needs a digit and either a '.' or an exponent, so a lone "-" or
    // "1e" stays a symbol.
    if a.chars().any(|c| c.is_ascii_digit()) && (a.contains('.') || a.contains(['e', 'E'])) {
        if let Ok(r) = a.parse::<f64>() {
            return Value::Real(r);
        }
    }
    Value::Symbol(lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read1(src: &str) -> Value {
        let forms = read_all(src).unwrap();
        assert_eq!(forms.len(), 1, "expected one form");
        forms.into_iter().next().unwrap()
    }

    #[test]
    fn reads_atoms_of_each_type() {
        assert_eq!(read1("42"), Value::Int(42));
        assert_eq!(read1("-7"), Value::Int(-7));
        assert_eq!(read1("2.5"), Value::Real(2.5));
        assert_eq!(read1("1e3"), Value::Real(1000.0));
        assert_eq!(read1("\"hi\""), Value::Str("hi".into()));
        assert_eq!(read1("nil"), Value::Nil);
        assert_eq!(read1("t"), Value::T);
        assert_eq!(
            read1("Foo"),
            Value::Symbol("foo".into()),
            "symbols fold case"
        );
    }

    #[test]
    fn a_lone_minus_is_a_symbol_not_a_number() {
        assert_eq!(read1("-"), Value::Symbol("-".into()));
        assert_eq!(read1("+"), Value::Symbol("+".into()));
    }

    #[test]
    fn reads_a_nested_list() {
        let v = read1("(setq x (+ 1 2))");
        let Value::List(items) = v else {
            panic!("expected list");
        };
        assert_eq!(items[0], Value::Symbol("setq".into()));
        assert_eq!(items[1], Value::Symbol("x".into()));
        assert_eq!(
            items[2],
            Value::List(vec![
                Value::Symbol("+".into()),
                Value::Int(1),
                Value::Int(2)
            ])
        );
    }

    #[test]
    fn empty_parens_read_as_nil() {
        assert_eq!(read1("()"), Value::Nil);
    }

    #[test]
    fn quote_expands_to_a_quote_form() {
        assert_eq!(
            read1("'foo"),
            Value::List(vec![
                Value::Symbol("quote".into()),
                Value::Symbol("foo".into())
            ])
        );
    }

    #[test]
    fn comments_and_strings_with_escapes() {
        let forms = read_all("; a comment\n(princ \"a\\nb\") ; trailing").unwrap();
        assert_eq!(forms.len(), 1);
        let Value::List(items) = &forms[0] else {
            panic!()
        };
        assert_eq!(items[1], Value::Str("a\nb".into()));
    }

    #[test]
    fn unterminated_string_is_an_error() {
        assert!(matches!(read_all("\"oops"), Err(LispError::Parse(_))));
        assert!(matches!(read_all("(foo"), Err(LispError::Parse(_))));
        assert!(matches!(read_all(")"), Err(LispError::Parse(_))));
    }
}
