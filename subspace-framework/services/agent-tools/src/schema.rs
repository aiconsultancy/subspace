//! JSON schemas for `action`-tagged tools.
//!
//! Model APIs accept a flat object schema for function parameters; a top-level
//! `oneOf` is dropped or collapsed to one variant, which hid every action but
//! the last from the model. So each schema is one object: an `action` enum, every
//! field as an optional property, and the required fields per action in the
//! descriptions. The Rust enums that decode the call stay strict.

use super::{Arguments, MAX_BATCH};
use serde_json::{Map, Value, json};

/// One action: its name, what it does, and its fields as (name, schema, required).
#[derive(Clone, Debug)]
pub struct Action {
    pub(crate) name: &'static str,
    purpose: &'static str,
    fields: Vec<(&'static str, Value, bool)>,
}

/// One field of a non-action tool.
#[derive(Clone, Debug)]
pub struct Field {
    name: &'static str,
    schema: Value,
    required: bool,
}

pub fn action(
    name: &'static str,
    purpose: &'static str,
    fields: Vec<(&'static str, Value, bool)>,
) -> Action {
    Action {
        name,
        purpose,
        fields,
    }
}

pub fn field(name: &'static str, schema: Value, required: bool) -> Field {
    Field {
        name,
        schema,
        required,
    }
}

/// "ado_query (query; optional limit): Run WIQL." for the action description.
fn usage(action: &Action) -> String {
    let names = |required: bool| -> Vec<&str> {
        action
            .fields
            .iter()
            .filter(|(_, _, needed)| *needed == required)
            .map(|(name, _, _)| *name)
            .collect()
    };
    let (required, optional) = (names(true), names(false));
    let mut parts = Vec::new();
    if !required.is_empty() {
        parts.push(required.join(", "));
    }
    if !optional.is_empty() {
        parts.push(format!("optional {}", optional.join(", ")));
    }
    let fields = if parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", parts.join("; "))
    };
    format!("{}{fields}: {}", action.name, action.purpose)
}

/// One flat object schema over `actions`: the arguments of a read tool, or
/// one operation of a write tool.
pub fn flat(actions: &[Action]) -> Arguments {
    Arguments::Actions {
        actions: actions.to_vec(),
        batched: false,
    }
}

pub(crate) fn flat_schema(actions: &[Action]) -> Value {
    let names: Vec<&str> = actions.iter().map(|action| action.name).collect();
    let guide: Vec<String> = actions.iter().map(usage).collect();
    let mut properties = Map::new();
    properties.insert(
        "action".into(),
        json!({"type":"string","enum":names,"description":guide.join("\n")}),
    );
    for (name, schema, _) in actions.iter().flat_map(|action| &action.fields) {
        let merged = merge(properties.remove(*name), schema);
        properties.insert((*name).into(), merged);
    }
    json!({"type":"object","additionalProperties":false,"required":["action"],"properties":properties})
}

/// The arguments of a write tool: 1 to 20 operations over `actions`.
pub fn batch(actions: &[Action]) -> Arguments {
    Arguments::Actions {
        actions: actions.to_vec(),
        batched: true,
    }
}

pub fn single(fields: Vec<Field>) -> Arguments {
    Arguments::Single { fields }
}

pub(crate) fn actions_schema(actions: &[Action], batched: bool) -> Value {
    if batched {
        let mut operations =
            json!({"type":"array","maxItems":MAX_BATCH,"items":flat_schema(actions)});
        operations["minItems"] = json!(1);
        json!({"type":"object","additionalProperties":false,"required":["operations"],"properties":{"operations":operations}})
    } else {
        flat_schema(actions)
    }
}

pub(crate) fn single_schema(fields: &[Field]) -> Value {
    let properties: Map<String, Value> = fields
        .iter()
        .map(|field| (field.name.into(), field.schema.clone()))
        .collect();
    let required: Vec<&str> = fields
        .iter()
        .filter(|field| field.required)
        .map(|field| field.name)
        .collect();
    let mut schema = json!({
        "type":"object",
        "additionalProperties":false,
        "properties":properties,
    });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    schema
}

/// The same field name can mean different things per action, such as a
/// numeric `id` and a text `id`; the property then accepts either.
fn merge(existing: Option<Value>, schema: &Value) -> Value {
    match existing {
        None => schema.clone(),
        Some(known) if known == *schema => known,
        Some(known) => {
            let mut options = match known.get("anyOf").and_then(Value::as_array) {
                Some(options) => options.clone(),
                None => vec![known],
            };
            if !options.contains(schema) {
                options.push(schema.clone());
            }
            json!({ "anyOf": options })
        }
    }
}

pub fn text() -> Value {
    json!({"type":"string"})
}

pub fn id() -> Value {
    json!({"type":"integer","minimum":1})
}

/// A comment or description body.
pub fn body() -> Value {
    json!({"type":"string","maxLength":65536})
}

#[cfg(test)]
mod tests {
    use super::{action, batch, field, flat, id, single, text};
    use serde_json::{Value, json};

    #[test]
    fn schemas_are_flat_objects_that_list_every_action_and_merge_field_types() {
        let schema = flat(&[
            action(
                "item",
                "One item.",
                vec![("id", id(), true), ("page", id(), false)],
            ),
            action("risk", "One risk.", vec![("id", text(), true)]),
            action("scope", "Boundaries.", vec![]),
        ])
        .input_schema();
        assert!(schema.get("oneOf").is_none(), "no top-level oneOf");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["action"]["enum"],
            json!(["item", "risk", "scope"])
        );
        let id_types: Vec<&Value> = schema["properties"]["id"]["anyOf"]
            .as_array()
            .unwrap()
            .iter()
            .map(|option| &option["type"])
            .collect();
        assert_eq!(id_types, ["integer", "string"]);
        let guide = schema["properties"]["action"]["description"]
            .as_str()
            .unwrap();
        assert!(guide.contains("item (id; optional page): One item."));
        assert!(guide.contains("scope: Boundaries."));
        let write =
            batch(&[action("note", "A note.", vec![("body", text(), true)])]).input_schema();
        assert_eq!(write["properties"]["operations"]["maxItems"], 20);
        assert_eq!(write["required"], json!(["operations"]));
        assert_eq!(write["properties"]["operations"]["minItems"], 1);
        let single = single(vec![field("query", text(), true)]).input_schema();
        assert_eq!(
            single,
            json!({"type":"object","additionalProperties":false,"required":["query"],"properties":{"query":{"type":"string"}}})
        );
    }
}
