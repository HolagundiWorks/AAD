//! A small JSON Schema validator for tool arguments: `type`, `enum`, `properties`, `required`,
//! `additionalProperties: false`, `items`, string length and number bounds. Anything else in a
//! schema is ignored, so registry schemas stay within this subset.

use serde_json::Value;

/// Check `value` against `schema`; the error names the offending argument.
pub fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    check(schema, value, "arguments")
}

fn type_ok(t: &str, v: &Value) -> bool {
    match t {
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "object" => v.is_object(),
        "array" => v.is_array(),
        "null" => v.is_null(),
        _ => false,
    }
}

fn check(schema: &Value, v: &Value, at: &str) -> Result<(), String> {
    if let Some(t) = schema.get("type").and_then(Value::as_str)
        && !type_ok(t, v)
    {
        return Err(format!("{at} must be of type {t}"));
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && !options.contains(v)
    {
        return Err(format!("{at} must be one of {}", options.iter().map(Value::to_string).collect::<Vec<_>>().join(", ")));
    }
    if let Some(s) = v.as_str() {
        let n = s.chars().count() as u64;
        if schema.get("maxLength").and_then(Value::as_u64).is_some_and(|m| n > m) {
            return Err(format!("{at} is too long"));
        }
        if schema.get("minLength").and_then(Value::as_u64).is_some_and(|m| n < m) {
            return Err(format!("{at} is too short"));
        }
    }
    if let Some(x) = v.as_f64() {
        if schema.get("minimum").and_then(Value::as_f64).is_some_and(|m| x < m) {
            return Err(format!("{at} is below the minimum"));
        }
        if schema.get("maximum").and_then(Value::as_f64).is_some_and(|m| x > m) {
            return Err(format!("{at} is above the maximum"));
        }
    }
    if let Some(o) = v.as_object() {
        let props = schema.get("properties").and_then(Value::as_object);
        for r in schema.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if !o.contains_key(r) {
                return Err(format!("{at}.{r} is required"));
            }
        }
        for (k, val) in o {
            match props.and_then(|p| p.get(k)) {
                Some(s) => check(s, val, &format!("{at}.{k}"))?,
                None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                    return Err(format!("{at}.{k} is not an allowed argument"));
                }
                None => {}
            }
        }
    }
    if let (Some(a), Some(items)) = (v.as_array(), schema.get("items")) {
        if schema.get("maxItems").and_then(Value::as_u64).is_some_and(|m| a.len() as u64 > m) {
            return Err(format!("{at} has too many items"));
        }
        for (i, x) in a.iter().enumerate() {
            check(items, x, &format!("{at}[{i}]"))?;
        }
    }
    Ok(())
}
