//! Recovery must wait for confirmed model and effort restoration.
#![cfg(test)]
use super::fixture::{actor, chat, is_chat};
use super::resilience_tests::{expire_stop, receive, start_waiting, until_ready, waiting_agent};
use super::{ChatCommand, ChatEvent, json, protocol};
use crate::runtime::lock;

#[cfg(test)]
fn selected_agent(load: bool) -> String {
    let script = waiting_agent(load);
    let start = script.find("models() {").unwrap();
    let end = script[start..].find("\ntext()").unwrap() + start;
    let models = r#"selected=fast
effort=low
models() { printf '[{"id":"model","category":"model","type":"select","currentValue":"%s","options":[{"value":"fast"},{"value":"deep"}]},{"id":"effort","category":"thought_level","type":"select","currentValue":"%s","options":[{"value":"low"},{"value":"high"}]}]' "$selected" "$effort"; }"#;
    script[..start].to_owned() + models + &script[end..]
}

#[tokio::test]
async fn recovery_confirms_model_and_effort_before_becoming_ready_on_both_setup_paths() {
    for load in [false, true] {
        let script = selected_agent(load).replace(
            "reply \"$id\" \"{\\\"configOptions\\\":$(models deep)}\" ;;",
            r#"case "$line" in
        *'"configId":"model"'*) selected=deep ;;
        *'"configId":"effort"'*) effort=high ;;
      esac
      reply "$id" "{\"configOptions\":$(models ignored)}" ;;"#,
        );
        let (_directory, mut actor, events) = actor(&script, None, None);
        start_waiting(&mut actor).await.unwrap();
        actor.chats.get_mut(&1).unwrap().models = protocol::model_options(&json!([
            {"id":"model","category":"model","type":"select","currentValue":"deep","options":[{"value":"fast"},{"value":"deep"}]},
            {"id":"effort","category":"thought_level","type":"select","currentValue":"high","options":[{"value":"low"},{"value":"high"}]}
        ]));
        expire_stop(&mut actor).await;
        actor
            .apply(
                chat(ChatCommand::Send {
                    text: "Again".into(),
                    context: vec![],
                    images: vec![],
                }),
                &Default::default(),
            )
            .await
            .unwrap();
        until_ready(&mut actor).await.unwrap();
        assert_eq!(actor.chats[&1].models.current.as_deref(), Some("deep"));
        assert_eq!(actor.chats[&1].models.effort.as_deref(), Some("high"));
        let selected = lock(&events.items).iter().rev().find_map(|event| {
            if let crate::Event::Chat {
                event: ChatEvent::SelectedModel { model, effort },
                ..
            } = event
            {
                Some((model.clone(), effort.clone()))
            } else {
                None
            }
        });
        assert_eq!(selected, Some(("deep".into(), Some("high".into()))));
        assert!(
            !lock(&events.items)
                .iter()
                .any(|event| is_chat(event, |event| matches!(event, ChatEvent::Error(_))))
        );
        while !actor.chats[&1].pending.is_empty() {
            receive(&mut actor).await.unwrap();
        }
        actor.shutdown().await;
    }
}
