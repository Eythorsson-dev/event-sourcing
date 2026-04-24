use serde_json::Value;
use serde_json_path::JsonPath;

/// Evaluate an RFC 9535 JSON Path expression against a serde_json::Value payload.
/// Returns the first matching node, or None if no match.
///
/// Callers (the builder) guarantee that `path` is a valid JsonPath string —
/// `unwrap()` here is safe because invalid paths are rejected at definition-build time.
pub(crate) fn evaluate_path(payload: &Value, path: &str) -> Option<Value> {
    let compiled = JsonPath::parse(path).ok()?;
    compiled.query(payload).first().cloned()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn path_simple_field() {
        let payload = json!({"amount": 42});
        let result = evaluate_path(&payload, "$.amount");
        assert_eq!(result, Some(json!(42)));
    }

    #[test]
    fn path_nested() {
        let payload = json!({"address": {"city": "London"}});
        let result = evaluate_path(&payload, "$.address.city");
        assert_eq!(result, Some(json!("London")));
    }

    #[test]
    fn path_missing_field() {
        let payload = json!({"x": 1});
        let result = evaluate_path(&payload, "$.y");
        assert_eq!(result, None);
    }

    #[test]
    fn path_null_value() {
        let payload = json!({"name": null});
        let result = evaluate_path(&payload, "$.name");
        assert_eq!(result, Some(Value::Null));
    }

    #[test]
    fn path_invalid_expression() {
        // A malformed path string — parse fails, ok()? short-circuits to None
        let payload = json!({"x": 1});
        let result = evaluate_path(&payload, "not-a-path");
        assert_eq!(result, None);
    }
}
