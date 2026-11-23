use super::{PrivateSettings, disk, redaction};
use anyhow::Result;
use serde_json::Value;

impl PrivateSettings {
    /// Filter exact configured values before ordinary tool readback.
    /// Code can still encode or transform a delegated value; this is not an
    /// information-flow guarantee for arbitrary runtime output.
    pub fn redact_readback(&self, mut value: Value) -> Result<Value> {
        let _lock = disk::lock(&self.directory)?;
        let record = disk::read(&self.directory)?;
        let values = record
            .values
            .values()
            .map(String::as_str)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        scrub(&mut value, &values);
        Ok(value)
    }
}

fn scrub(value: &mut Value, values: &[&str]) {
    match value {
        Value::String(text) => *text = redaction::text(text, values),
        Value::Number(number) if values.contains(&number.to_string().as_str()) => {
            *value = Value::String("[redacted]".into());
        }
        Value::Array(items) => items.iter_mut().for_each(|item| scrub(item, values)),
        Value::Object(fields) => fields.values_mut().for_each(|item| scrub(item, values)),
        _ => {}
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::PrivateSettings;
    use crate::private_settings::Edit;
    use serde_json::json;
    use std::collections::BTreeMap;
    use subspace_framework::core::Secret;

    #[test]
    fn nested_tool_readback_uses_all_saved_values_without_exposing_them() {
        let root = tempfile::tempdir().expect("private settings root");
        let settings = PrivateSettings::new(root.path(), "readback").expect("store");
        settings
            .update(
                0,
                BTreeMap::from([
                    (
                        "TOKEN".into(),
                        Edit::Replace(Secret::new("dummy-private-value")),
                    ),
                    ("NUMERIC_VALUE".into(), Edit::Replace(Secret::new("12345"))),
                ]),
            )
            .expect("synthetic values");
        let output = settings.redact_readback(json!({"text":"prefix dummy-private-value tail", "rows":[["public",12345]], "count":7})).expect("safe readback");
        assert_eq!(output["text"], "prefix [redacted] tail");
        assert_eq!(output["rows"][0][1], "[redacted]");
        assert_eq!(output["count"], 7);
        assert!(!output.to_string().contains("dummy-private-value"));
        assert!(!output.to_string().contains("12345"));
    }
}
