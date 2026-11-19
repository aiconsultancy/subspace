use super::{
    DEFINITION_SCHEMA, HostEvent, HostMessage, LivePatch, MessageContext, PROTOCOL_SCHEMA,
    WorkerEvent, WorkerMessage, limits,
};

#[test]
fn protocol_round_trips_correlated_action_completion_and_rejects_unknown_versions() {
    let context = MessageContext::new("request-one", "session-one", 3);
    let host = HostMessage::new(
        context.clone(),
        HostEvent::Action {
            action: "refresh".into(),
            values: std::collections::BTreeMap::new(),
        },
    );
    let bytes = host.to_line().unwrap();
    let decoded: HostMessage = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, host);
    let worker = WorkerMessage::new(
        context,
        WorkerEvent::Result {
            patch: LivePatch::default(),
        },
    );
    assert_eq!(
        WorkerMessage::parse(&worker.to_line().unwrap()).unwrap(),
        worker
    );
    let mut json = serde_json::to_value(worker).unwrap();
    json["context"]["version"] = 2.into();
    assert!(WorkerMessage::parse(&serde_json::to_vec(&json).unwrap()).is_err());
    json["context"]["version"] = 1.into();
    json["event"]["host_callback"] = "run".into();
    assert!(WorkerMessage::parse(&serde_json::to_vec(&json).unwrap()).is_err());
}

#[test]
fn oversized_malformed_frames_and_errors_are_rejected() {
    assert!(WorkerMessage::parse(&vec![b' '; limits::FRAME_BYTES + 1]).is_err());
    assert!(WorkerMessage::parse(b"not JSON").is_err());
    let message = WorkerMessage::new(
        MessageContext::new("request-one", "session-one", 0),
        WorkerEvent::Error {
            message: "x".repeat(limits::TEXT_BYTES + 1),
        },
    );
    assert!(WorkerMessage::parse(&message.to_line().unwrap()).is_err());
    let invalid = WorkerMessage::new(
        MessageContext::new("../request", "session-one", 0),
        WorkerEvent::Ready,
    );
    assert!(invalid.to_line().is_err());
}

#[test]
fn published_schemas_parse_and_all_local_references_resolve() {
    let definition: serde_json::Value = serde_json::from_str(DEFINITION_SCHEMA).unwrap();
    let protocol: serde_json::Value = serde_json::from_str(PROTOCOL_SCHEMA).unwrap();
    assert_eq!(definition["properties"]["version"]["const"], 1);
    assert_eq!(
        protocol["$defs"]["context"]["properties"]["version"]["const"],
        1
    );
    assert!(references_resolve(&definition, &definition));
    assert!(references_resolve(&protocol, &protocol));
    let example = super::ProjectDefinition::parse(super::EXAMPLE_DEFINITION.as_bytes()).unwrap();
    assert_eq!(example.id, "native-demo");
    let fixture = super::tests::fixture();
    assert_eq!(
        super::ProjectDefinition::parse(&serde_json::to_vec(&fixture).unwrap()).unwrap(),
        fixture
    );
}

fn references_resolve(value: &serde_json::Value, schema: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => object.iter().all(|(key, child)| {
            if key == "$ref"
                && child
                    .as_str()
                    .is_some_and(|reference| reference.starts_with('#'))
            {
                return child
                    .as_str()
                    .is_some_and(|reference| schema.pointer(&reference[1..]).is_some());
            }
            references_resolve(child, schema)
        }),
        serde_json::Value::Array(array) => {
            array.iter().all(|child| references_resolve(child, schema))
        }
        _ => true,
    }
}
