use super::process_tests::{
    apply_next_response, confirm_model, confirmation_count, reject_model, started_confirmation_chat,
};
use super::{ChatEvent, Event};
use serde_json::json;

#[tokio::test]
async fn setting_acknowledgements_confirm_only_a_matching_successful_selection() {
    let (_directory, mut chat, mut receiver, events) = started_confirmation_chat(
        r#"    *set-thinking*) printf '%s\n' '{"type":"response","id":"set-thinking","command":"set_thinking_level","success":true}' ;;
    *thinking-state*) printf '%s\n' '{"type":"response","id":"thinking-state","command":"get_state","success":true,"data":{"sessionId":"fake-1","model":{"provider":"unknown","id":"unknown"},"thinkingLevel":"off"}}' ;;
"#,
        8,
    )
    .await;
    assert!(
        !super::super::lock(&events.items)
            .iter()
            .any(|event| matches!(
                event,
                Event::Chat {
                    event: ChatEvent::ModelConfirmed { .. },
                    ..
                }
            ))
    );
    chat.effort = Some("medium".into());
    chat.command(json!({"type":"set_thinking_level","id":"set-thinking","level":"high"}))
        .await
        .unwrap();
    apply_next_response(&mut chat, &mut receiver, &events).await;
    assert_eq!(chat.effort.as_deref(), Some("medium"));
    apply_next_response(&mut chat, &mut receiver, &events).await;
    assert_eq!(chat.effort.as_deref(), Some("off"));
    confirm_model(&mut chat, &events).await;
    assert_eq!(chat.model.as_deref(), Some("local/second"));
    assert_eq!(chat.effort.as_deref(), Some("off"));
    assert_eq!(confirmation_count(&events), 1);
    reject_model(&mut chat, &events).await;
    assert_eq!(confirmation_count(&events), 1);
    chat.stop().await;
}

#[tokio::test]
async fn stored_default_refreshes_the_model_without_persisting_confirmation() {
    let (_directory, mut chat, mut receiver, events) = started_confirmation_chat(
        r#"    *set-model-*) id=$(printf '%s\n' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p'); printf '{"type":"response","id":"%s","command":"set_model","success":true}\n' "$id" ;;
    *model-state-*) id=$(printf '%s\n' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p'); printf '{"type":"response","id":"%s","command":"get_state","success":true,"data":{"sessionId":"fake-1","model":{"provider":"local","id":"second"},"thinkingLevel":"off"}}\n' "$id" ;;
"#,
        9,
    )
    .await;
    chat.set_model("local/second", None, false).await.unwrap();
    apply_next_response(&mut chat, &mut receiver, &events).await;
    apply_next_response(&mut chat, &mut receiver, &events).await;
    assert_eq!(chat.model.as_deref(), Some("local/second"));
    assert_eq!(confirmation_count(&events), 0);
    chat.stop().await;
}
