use std::collections::HashMap;

use super::Rehydrator;

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
