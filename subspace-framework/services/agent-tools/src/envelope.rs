//! Shared metadata for content that tools return as untrusted evidence.

use serde_json::{Map, Value};

/// Adds the common evidence fields without replacing payload-owned values.
pub fn untrusted(source: &str, payload: Value) -> Value {
    let mut object = match payload {
        Value::Object(object) => object,
        value => Map::from_iter([("data".into(), value)]),
    };
    object.entry("source").or_insert_with(|| source.into());
    object.entry("untrusted").or_insert(Value::Bool(true));
    object.entry("truncated").or_insert(Value::Bool(false));
    object.entry("coverage").or_insert(Value::Null);
    if !object.contains_key("fetched_at") && !object.contains_key("refreshed_at") {
        object.insert("fetched_at".into(), Value::Null);
    }
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::untrusted;
    use serde_json::json;

    #[test]
    fn envelope_adds_defaults_and_preserves_known_freshness() {
        let value = untrusted(
            "cache",
            json!({"refreshed_at": 42, "truncated": true, "items": []}),
        );
        assert_eq!(value["source"], "cache");
        assert_eq!(value["untrusted"], true);
        assert_eq!(value["refreshed_at"], 42);
        assert_eq!(value["truncated"], true);
        assert!(value.get("coverage").is_some());
        assert!(value.get("fetched_at").is_none());
    }
}
