//! Strict parsing of the model's output into a tool call.

use serde_json::{Map, Value};

/// A tool call as proposed by the model, not yet validated.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Map<String, Value>,
}

impl ToolCall {
    /// The call as one canonical string (used to spot a repeated call).
    pub fn key(&self) -> String {
        format!("{}@{}", self.name, Value::Object(self.arguments.clone()))
    }
}

/// Why the model output is not a usable tool call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// No `<tool_call>` block and not a JSON object.
    NoCall,
    /// More than one `<tool_call>` block.
    MultipleCalls,
    /// The block is not valid JSON.
    BadJson(String),
    /// The JSON is not `{"name": string, "arguments": object}` (extra keys are refused).
    BadShape(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::NoCall => write!(f, "the model output contains no tool call"),
            ParseError::MultipleCalls => write!(f, "more than one tool call in one reply"),
            ParseError::BadJson(e) => write!(f, "the tool call is not valid JSON: {e}"),
            ParseError::BadShape(e) => write!(f, "the tool call has the wrong shape: {e}"),
        }
    }
}

const OPEN: &str = "<tool_call>";
const CLOSE: &str = "</tool_call>";
/// The longest model reply that is looked at.
const MAX_OUTPUT: usize = 16 * 1024;

/// Extract the one tool call of a model reply: either a single `<tool_call>{…}</tool_call>` block
/// (Qwen's format) or a reply that is nothing but a JSON object. Text that merely resembles a call
/// is not executed.
pub fn parse_tool_call(output: &str) -> Result<ToolCall, ParseError> {
    if output.len() > MAX_OUTPUT {
        return Err(ParseError::BadShape("reply too long".into()));
    }
    let json = match output.matches(OPEN).count() {
        0 => {
            let t = output.trim();
            if t.starts_with('{') && t.ends_with('}') { t } else { return Err(ParseError::NoCall) }
        }
        1 => {
            let start = output.find(OPEN).map(|i| i + OPEN.len()).ok_or(ParseError::NoCall)?;
            let rest = output.get(start..).ok_or(ParseError::NoCall)?;
            let end = rest.find(CLOSE).ok_or_else(|| ParseError::BadShape("unterminated <tool_call>".into()))?;
            rest.get(..end).ok_or(ParseError::NoCall)?.trim()
        }
        _ => return Err(ParseError::MultipleCalls),
    };
    let v: Value = serde_json::from_str(json).map_err(|e| ParseError::BadJson(e.to_string()))?;
    let Value::Object(mut o) = v else { return Err(ParseError::BadShape("not an object".into())) };
    if let Some(k) = o.keys().find(|k| *k != "name" && *k != "arguments") {
        return Err(ParseError::BadShape(format!("unexpected key `{k}`")));
    }
    let name = match o.remove("name") {
        Some(Value::String(n)) if !n.is_empty() && n.len() <= 64 => n,
        _ => return Err(ParseError::BadShape("`name` must be a short string".into())),
    };
    let arguments = match o.remove("arguments") {
        None => Map::new(),
        Some(Value::Object(a)) => a,
        Some(_) => return Err(ParseError::BadShape("`arguments` must be an object".into())),
    };
    Ok(ToolCall { name, arguments })
}
