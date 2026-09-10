use super::ResponseEvent;
use super::ResponsesStreamEvent;
use super::process_responses_event;
use codex_protocol::models::ResponseItem;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn partial_added_items_start_before_streamed_content() {
    // Observed live: a provider omits the not-yet-streamed array on added items.
    for (item, field) in [
        (
            json!({"type":"reasoning", "id":"rs-live", "status":"in_progress"}),
            "summary",
        ),
        (
            json!({"type":"message", "id":"msg-live", "role":"assistant", "status":"in_progress"}),
            "content",
        ),
    ] {
        let event: ResponsesStreamEvent = serde_json::from_value(json!({
            "type":"response.output_item.added", "item":item
        }))
        .unwrap();
        let mut expected = item;
        expected[field] = json!([]);
        let expected: ResponseItem = serde_json::from_value(expected).unwrap();
        let Some(ResponseEvent::OutputItemAdded(actual)) = process_responses_event(event).unwrap()
        else {
            panic!("partial item must be emitted before its deltas");
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn partial_item_handling_preserves_existing_content() {
    for item in [
        json!({"type":"reasoning", "id":"rs-live", "summary":[{"type":"summary_text", "text":"A summary"}]}),
        json!({"type":"message", "id":"msg-live", "role":"assistant", "content":[{"type":"output_text", "text":"A message"}]}),
    ] {
        let event: ResponsesStreamEvent = serde_json::from_value(json!({
            "type":"response.output_item.added", "item":item
        }))
        .unwrap();
        let expected: ResponseItem = serde_json::from_value(item).unwrap();
        let Some(ResponseEvent::OutputItemAdded(actual)) = process_responses_event(event).unwrap()
        else {
            panic!("complete added item must remain available");
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn partial_item_defaults_do_not_repair_completed_or_invalid_items() {
    for event in [
        json!({"type":"response.output_item.done", "item":{"type":"reasoning", "id":"rs-live"}}),
        json!({"type":"response.output_item.done", "item":{"type":"message", "id":"msg-live", "role":"assistant"}}),
        json!({"type":"response.output_item.added", "item":{"type":"reasoning", "id":"rs-live", "summary":null}}),
        json!({"type":"response.output_item.added", "item":{"type":"message", "id":"msg-live", "role":"assistant", "content":null}}),
    ] {
        let event: ResponsesStreamEvent = serde_json::from_value(event).unwrap();
        assert!(process_responses_event(event).unwrap().is_none());
    }
}
