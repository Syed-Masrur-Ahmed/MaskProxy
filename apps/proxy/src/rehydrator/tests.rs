use std::collections::HashMap;

use super::{find_safe_split, is_placeholder_prefix, Rehydrator, SseRehydrator, StreamingRehydrator};

// ===========================================================================
// Rehydrator (non-streaming) tests
// ===========================================================================

#[test]
fn rehydrates_plain_text_tokens() {
    let mut mapping = HashMap::new();
    mapping.insert(
        "<<MASK:PERSON_NAME_1:MASK>>".to_string(),
        "John Smith".to_string(),
    );
    mapping.insert(
        "<<MASK:EMAIL_1:MASK>>".to_string(),
        "alice@example.com".to_string(),
    );

    let rehydrated = Rehydrator::new().rehydrate_text(
        "Email <<MASK:PERSON_NAME_1:MASK>> at <<MASK:EMAIL_1:MASK>>.",
        &mapping,
    );

    assert_eq!(rehydrated, "Email John Smith at alice@example.com.");
}

#[test]
fn rehydrate_text_does_not_cascade_placeholder_like_values() {
    let mut mapping = HashMap::new();
    mapping.insert(
        "<<MASK:PERSON_NAME_1:MASK>>".to_string(),
        "<<MASK:EMAIL_1:MASK>>".to_string(),
    );
    mapping.insert(
        "<<MASK:EMAIL_1:MASK>>".to_string(),
        "alice@example.com".to_string(),
    );

    let rehydrated =
        Rehydrator::new().rehydrate_text("Hello <<MASK:PERSON_NAME_1:MASK>>", &mapping);

    assert_eq!(rehydrated, "Hello <<MASK:EMAIL_1:MASK>>");
}

#[test]
fn rehydrates_nested_json_body() {
    let mut mapping = HashMap::new();
    mapping.insert(
        "<<MASK:PERSON_NAME_1:MASK>>".to_string(),
        "John Smith".to_string(),
    );

    let body = r#"{"choices":[{"message":{"content":"Hello <<MASK:PERSON_NAME_1:MASK>>"}}]}"#;
    let rehydrated = Rehydrator::new().rehydrate_body(body, &mapping).unwrap();

    assert!(rehydrated.contains("John Smith"));
    assert!(!rehydrated.contains("<<MASK:PERSON_NAME_1:MASK>>"));
}

#[test]
fn rehydrate_text_no_tokens_returns_unchanged() {
    let mapping = HashMap::new();
    let text = "This text has no placeholders at all.";
    let rehydrated = Rehydrator::new().rehydrate_text(text, &mapping);
    assert_eq!(rehydrated, text);
}

#[test]
fn rehydrate_text_unknown_token_left_in_place() {
    let mapping = HashMap::new();
    let text = "Hello <<MASK:PERSON_NAME_1:MASK>>, welcome.";
    let rehydrated = Rehydrator::new().rehydrate_text(text, &mapping);
    assert_eq!(rehydrated, text, "unknown token should remain as-is");
}

#[test]
fn rehydrate_text_mixed_known_and_unknown_tokens() {
    let mut mapping = HashMap::new();
    mapping.insert(
        "<<MASK:EMAIL_1:MASK>>".to_string(),
        "bob@test.com".to_string(),
    );
    let text = "Contact <<MASK:PERSON_NAME_1:MASK>> at <<MASK:EMAIL_1:MASK>>.";
    let rehydrated = Rehydrator::new().rehydrate_text(text, &mapping);
    assert_eq!(
        rehydrated,
        "Contact <<MASK:PERSON_NAME_1:MASK>> at bob@test.com."
    );
}

#[test]
fn rehydrate_body_invalid_json_returns_error() {
    let mapping = HashMap::new();
    let result = Rehydrator::new().rehydrate_body("not valid json {{{", &mapping);
    assert!(result.is_err(), "invalid JSON should return Err");
}

#[test]
fn rehydrate_body_replaces_tokens_in_arrays() {
    let mut mapping = HashMap::new();
    mapping.insert("<<MASK:SSN_1:MASK>>".to_string(), "123-45-6789".to_string());
    let body = r#"{"data":["SSN is <<MASK:SSN_1:MASK>>","no token here"]}"#;
    let rehydrated = Rehydrator::new().rehydrate_body(body, &mapping).unwrap();
    assert!(rehydrated.contains("123-45-6789"));
    assert!(!rehydrated.contains("<<MASK:SSN_1:MASK>>"));
    assert!(rehydrated.contains("no token here"));
}

#[test]
fn rehydrate_body_handles_deeply_nested_objects() {
    let mut mapping = HashMap::new();
    mapping.insert("<<MASK:PHONE_1:MASK>>".to_string(), "555-0100".to_string());
    let body = r#"{"a":{"b":{"c":{"d":"call <<MASK:PHONE_1:MASK>>"}}}}"#;
    let rehydrated = Rehydrator::new().rehydrate_body(body, &mapping).unwrap();
    assert!(rehydrated.contains("call 555-0100"));
}

#[test]
fn rehydrate_text_multiple_tokens_same_type() {
    let mut mapping = HashMap::new();
    mapping.insert("<<MASK:EMAIL_1:MASK>>".to_string(), "a@b.com".to_string());
    mapping.insert("<<MASK:EMAIL_2:MASK>>".to_string(), "c@d.com".to_string());
    let text = "From <<MASK:EMAIL_1:MASK>> to <<MASK:EMAIL_2:MASK>>.";
    let rehydrated = Rehydrator::new().rehydrate_text(text, &mapping);
    assert_eq!(rehydrated, "From a@b.com to c@d.com.");
}

// ===========================================================================
// StreamingRehydrator tests (plain text level)
// ===========================================================================

fn text_map() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("<<MASK:PERSON_NAME_1:MASK>>".into(), "Alice Johnson".into());
    m.insert("<<MASK:EMAIL_1:MASK>>".into(), "alice@example.com".into());
    m.insert("<<MASK:PHONE_1:MASK>>".into(), "555-0100".into());
    m
}

#[test]
fn streaming_complete_placeholder_in_single_chunk() {
    let map = text_map();
    let mut sr = StreamingRehydrator::new();
    let out = sr.process_chunk("Hello <<MASK:PERSON_NAME_1:MASK>> world", &map);
    let flushed = sr.flush(&map);
    let combined = format!("{out}{flushed}");
    assert_eq!(combined, "Hello Alice Johnson world");
}

#[test]
fn streaming_placeholder_split_across_two_chunks() {
    let map = text_map();
    let mut sr = StreamingRehydrator::new();
    let out1 = sr.process_chunk("email <<MASK:EMA", &map);
    let out2 = sr.process_chunk("IL_1:MASK>> ok", &map);
    let flushed = sr.flush(&map);
    let combined = format!("{out1}{out2}{flushed}");
    assert!(combined.contains("alice@example.com"), "got: {combined}");
    assert!(!combined.contains("<<MASK:"));
}

#[test]
fn streaming_placeholder_split_across_three_chunks() {
    let map = text_map();
    let mut sr = StreamingRehydrator::new();
    let o1 = sr.process_chunk("Hello <<MAS", &map);
    let o2 = sr.process_chunk("K:PHONE_1:", &map);
    let o3 = sr.process_chunk("MASK>> world", &map);
    let flushed = sr.flush(&map);
    let combined = format!("{o1}{o2}{o3}{flushed}");
    assert!(combined.contains("555-0100"), "got: {combined}");
    assert!(combined.contains("Hello "));
    assert!(combined.contains(" world"));
}

#[test]
fn streaming_no_placeholders_passes_through() {
    let map = text_map();
    let mut sr = StreamingRehydrator::new();
    let o1 = sr.process_chunk("hello ", &map);
    let o2 = sr.process_chunk("world", &map);
    let flushed = sr.flush(&map);
    assert_eq!(format!("{o1}{o2}{flushed}"), "hello world");
}

#[test]
fn streaming_empty_chunks() {
    let map = text_map();
    let mut sr = StreamingRehydrator::new();
    let o1 = sr.process_chunk("", &map);
    let o2 = sr.process_chunk("", &map);
    let flushed = sr.flush(&map);
    assert!(o1.is_empty() && o2.is_empty() && flushed.is_empty());
}

#[test]
fn streaming_angle_bracket_at_end_held_then_released() {
    let map = text_map();
    let mut sr = StreamingRehydrator::new();
    let o1 = sr.process_chunk("hello <", &map);
    let o2 = sr.process_chunk("3 done", &map);
    let flushed = sr.flush(&map);
    assert_eq!(format!("{o1}{o2}{flushed}"), "hello <3 done");
}

#[test]
fn streaming_flush_emits_partial_as_is() {
    let map = text_map();
    let mut sr = StreamingRehydrator::new();
    let o1 = sr.process_chunk("data: <<MASK:UNKN", &map);
    let flushed = sr.flush(&map);
    assert_eq!(format!("{o1}{flushed}"), "data: <<MASK:UNKN");
}

#[test]
fn streaming_placeholder_at_exact_chunk_boundary() {
    let map = text_map();
    let mut sr = StreamingRehydrator::new();
    let o1 = sr.process_chunk("<<MASK:EMAIL_1:MASK>>", &map);
    let o2 = sr.process_chunk(" next", &map);
    let flushed = sr.flush(&map);
    assert_eq!(format!("{o1}{o2}{flushed}"), "alice@example.com next");
}

// ===========================================================================
// SseRehydrator tests (SSE-event-aware)
// ===========================================================================

fn sse_map() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("<<MASK:PERSON_NAME_1:MASK>>".into(), "Alice Johnson".into());
    m.insert("<<MASK:EMAIL_1:MASK>>".into(), "alice@example.com".into());
    m.insert("<<MASK:PHONE_1:MASK>>".into(), "555-0100".into());
    m
}

fn openai_sse_event(content: &str) -> String {
    format!(
        "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n\n",
        content
    )
}

#[test]
fn sse_complete_placeholder_in_single_event() {
    let map = sse_map();
    let mut sr = SseRehydrator::new();
    let out = sr.process_chunk(&openai_sse_event("<<MASK:EMAIL_1:MASK>>"), &map);
    let flushed = sr.flush(&map);
    let combined = format!("{out}{flushed}");
    assert!(combined.contains("alice@example.com"), "got: {combined}");
    assert!(!combined.contains("<<MASK:"));
}

#[test]
fn sse_placeholder_split_across_two_events() {
    let map = sse_map();
    let mut sr = SseRehydrator::new();

    let chunk = format!(
        "{}{}",
        openai_sse_event("Hello <<MASK:PER"),
        openai_sse_event("SON_NAME_1:MASK>> world"),
    );
    let out = sr.process_chunk(&chunk, &map);
    let flushed = sr.flush(&map);
    let combined = format!("{out}{flushed}");

    // Verify the full concatenated content is correct.
    let contents = extract_all_contents(&combined);
    let full_text: String = contents.join("");
    assert!(
        full_text.contains("Alice Johnson"),
        "expected rehydrated name in '{full_text}', full output: {combined}"
    );
    assert!(
        !full_text.contains("<<MASK:"),
        "placeholder not rehydrated in '{full_text}'"
    );
}

#[test]
fn sse_placeholder_split_across_three_events() {
    let map = sse_map();
    let mut sr = SseRehydrator::new();

    let chunk = format!(
        "{}{}{}",
        openai_sse_event("call <<MAS"),
        openai_sse_event("K:PHONE_1:"),
        openai_sse_event("MASK>> now"),
    );
    let out = sr.process_chunk(&chunk, &map);
    let flushed = sr.flush(&map);
    let combined = format!("{out}{flushed}");

    let contents = extract_all_contents(&combined);
    let full_text: String = contents.join("");
    assert!(
        full_text.contains("555-0100"),
        "expected phone in '{full_text}'"
    );
}

#[test]
fn sse_events_arriving_across_tcp_chunks() {
    let map = sse_map();
    let mut sr = SseRehydrator::new();

    // First TCP chunk: complete event + start of next
    let out1 = sr.process_chunk(
        &format!(
            "{}data: {{\"choices\":[{{\"delta\":{{\"content\":\"<<MASK:EMA",
            openai_sse_event("Contact "),
        ),
        &map,
    );
    // Second TCP chunk: rest of the event
    let out2 = sr.process_chunk("IL_1:MASK>>\"}}]}\n\n", &map);
    let flushed = sr.flush(&map);
    let combined = format!("{out1}{out2}{flushed}");

    let contents = extract_all_contents(&combined);
    let full_text: String = contents.join("");
    assert!(
        full_text.contains("alice@example.com"),
        "got: {full_text}"
    );
}

#[test]
fn sse_no_placeholders_passes_through() {
    let map = sse_map();
    let mut sr = SseRehydrator::new();

    let events = format!(
        "{}{}",
        openai_sse_event("hello"),
        openai_sse_event(" world"),
    );
    let out = sr.process_chunk(&events, &map);
    let flushed = sr.flush(&map);
    let combined = format!("{out}{flushed}");

    let contents = extract_all_contents(&combined);
    assert_eq!(contents.join(""), "hello world");
}

#[test]
fn sse_done_event_passes_through() {
    let map = sse_map();
    let mut sr = SseRehydrator::new();

    let events = format!("{}data: [DONE]\n\n", openai_sse_event("hi"));
    let out = sr.process_chunk(&events, &map);
    let flushed = sr.flush(&map);
    let combined = format!("{out}{flushed}");

    assert!(combined.contains("[DONE]"), "got: {combined}");
}

#[test]
fn sse_anthropic_format() {
    let map = sse_map();
    let mut sr = SseRehydrator::new();

    let event = "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"<<MASK:EMAIL_1:MASK>>\"}}\n\n";
    let out = sr.process_chunk(event, &map);
    let flushed = sr.flush(&map);
    let combined = format!("{out}{flushed}");

    assert!(
        combined.contains("alice@example.com"),
        "got: {combined}"
    );
    assert!(!combined.contains("<<MASK:"));
}

#[test]
fn sse_anthropic_placeholder_split() {
    let map = sse_map();
    let mut sr = SseRehydrator::new();

    let events = concat!(
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"Hi <<MASK:PER\"}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"SON_NAME_1:MASK>>!\"}}\n\n",
    );
    let out = sr.process_chunk(events, &map);
    let flushed = sr.flush(&map);
    let combined = format!("{out}{flushed}");

    let contents = extract_all_anthropic_contents(&combined);
    let full_text: String = contents.join("");
    assert!(
        full_text.contains("Alice Johnson"),
        "got: {full_text}"
    );
}

#[test]
fn sse_realistic_full_sequence() {
    let map = sse_map();
    let mut sr = SseRehydrator::new();

    let chunks = [
        openai_sse_event("Contact "),
        openai_sse_event("<<MASK:PER"),
        openai_sse_event("SON_NAME_1:MASK>>"),
        openai_sse_event(" at "),
        openai_sse_event("<<MASK:EMAIL_1:MASK>>"),
        "data: [DONE]\n\n".to_string(),
    ];

    let mut combined = String::new();
    for chunk in &chunks {
        combined.push_str(&sr.process_chunk(chunk, &map));
    }
    combined.push_str(&sr.flush(&map));

    let contents = extract_all_contents(&combined);
    let full_text: String = contents.join("");
    assert!(
        full_text.contains("Alice Johnson"),
        "expected person, got: {full_text}"
    );
    assert!(
        full_text.contains("alice@example.com"),
        "expected email, got: {full_text}"
    );
    assert!(combined.contains("[DONE]"));
}

#[test]
fn sse_multiple_placeholders_same_event() {
    let map = sse_map();
    let mut sr = SseRehydrator::new();

    let event = openai_sse_event("<<MASK:PERSON_NAME_1:MASK>> (<<MASK:EMAIL_1:MASK>>)");
    let out = sr.process_chunk(&event, &map);
    let flushed = sr.flush(&map);
    let combined = format!("{out}{flushed}");

    let contents = extract_all_contents(&combined);
    let full_text: String = contents.join("");
    assert!(full_text.contains("Alice Johnson"));
    assert!(full_text.contains("alice@example.com"));
}

// ===========================================================================
// is_placeholder_prefix unit tests
// ===========================================================================

#[test]
fn prefix_detection_valid_prefixes() {
    assert!(is_placeholder_prefix("<"));
    assert!(is_placeholder_prefix("<<"));
    assert!(is_placeholder_prefix("<<M"));
    assert!(is_placeholder_prefix("<<MA"));
    assert!(is_placeholder_prefix("<<MAS"));
    assert!(is_placeholder_prefix("<<MASK"));
    assert!(is_placeholder_prefix("<<MASK:"));
    assert!(is_placeholder_prefix("<<MASK:E"));
    assert!(is_placeholder_prefix("<<MASK:EMAIL_1"));
    assert!(is_placeholder_prefix("<<MASK:EMAIL_1:"));
    assert!(is_placeholder_prefix("<<MASK:EMAIL_1:MASK"));
    assert!(is_placeholder_prefix("<<MASK:EMAIL_1:MASK>"));
    assert!(is_placeholder_prefix("<<MASK:EMAIL_1:MASK>>"));
}

#[test]
fn prefix_detection_invalid_prefixes() {
    assert!(!is_placeholder_prefix("<a"));
    assert!(!is_placeholder_prefix("<<m")); // lowercase
    assert!(!is_placeholder_prefix("<<MASK:email")); // lowercase after colon
    assert!(!is_placeholder_prefix("<<NOTMASK:"));
}

// ===========================================================================
// find_safe_split unit tests
// ===========================================================================

#[test]
fn safe_split_no_angle_brackets() {
    let s = "hello world";
    assert_eq!(find_safe_split(s), s.len());
}

#[test]
fn safe_split_complete_placeholder_emits_all() {
    let s = "hello <<MASK:EMAIL_1:MASK>> world";
    assert_eq!(find_safe_split(s), s.len());
}

#[test]
fn safe_split_trailing_partial() {
    let s = "hello <<MASK:EMA";
    assert_eq!(find_safe_split(s), 6); // split at the first `<`
}

#[test]
fn safe_split_single_trailing_angle() {
    let s = "hello <";
    assert_eq!(find_safe_split(s), 6); // `<` could start `<<MASK:...`
}

// ===========================================================================
// Test helpers
// ===========================================================================

/// Extract all OpenAI `choices[0].delta.content` values from SSE output.
fn extract_all_contents(sse_output: &str) -> Vec<String> {
    let mut contents = Vec::new();
    for line in sse_output.split("\n\n") {
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data: ") {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(content) = json
                    .pointer("/choices/0/delta/content")
                    .and_then(serde_json::Value::as_str)
                {
                    contents.push(content.to_string());
                }
            }
        }
    }
    contents
}

/// Extract all Anthropic `delta.text` values from SSE output.
fn extract_all_anthropic_contents(sse_output: &str) -> Vec<String> {
    let mut contents = Vec::new();
    for line in sse_output.split("\n\n") {
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data: ") {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(text) = json
                    .pointer("/delta/text")
                    .and_then(serde_json::Value::as_str)
                {
                    contents.push(text.to_string());
                }
            }
        }
    }
    contents
}
