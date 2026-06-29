use serde_json::Value;

pub fn strip_cv_metadata(cv: &mut Value) {
    if let Some(obj) = cv.as_object_mut() {
        obj.remove("constraints");
    }
}

/// Parse `satisfied` field defensively — handles bool, string, number.
pub fn parse_satisfied(val: &Value) -> bool {
    match val.get("satisfied") {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.trim().to_lowercase() == "true",
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) != 0,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_satisfied_handles_types() {
        assert!(parse_satisfied(&json!({"satisfied": true})));
        assert!(!parse_satisfied(&json!({"satisfied": false})));
        assert!(parse_satisfied(&json!({"satisfied": "true"})));
        assert!(parse_satisfied(&json!({"satisfied": " True "})));
        assert!(!parse_satisfied(&json!({"satisfied": "false"})));
        assert!(parse_satisfied(&json!({"satisfied": 1})));
        assert!(!parse_satisfied(&json!({"satisfied": 0})));
        assert!(!parse_satisfied(&json!({})));
    }
}
