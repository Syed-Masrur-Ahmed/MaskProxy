use std::collections::HashMap;
use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;
use serde_json::Value;

use crate::masker::ner::{Entity, NER};

pub mod ner;

const CONTENT_PART_TEXT_KEYS: &[&str] = &["text"];
const PRIORITY_REGEX: usize = 0;
const PRIORITY_NER: usize = 10;

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b").expect("valid email regex")
});
static PHONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\+?1[-.\s]?)?(?:\(?\d{3}\)?[-.\s]?){2}\d{4}").expect("valid phone regex")
});
static SSN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").expect("valid ssn regex"));

#[derive(Clone, Debug, Default)]
pub struct MappingState {
    counters: HashMap<String, usize>,
    real_to_placeholder: HashMap<String, String>,
    placeholder_to_real: HashMap<String, String>,
}

impl MappingState {
    pub fn placeholder_to_real(&self) -> &HashMap<String, String> {
        &self.placeholder_to_real
    }

    fn placeholder_for(&mut self, kind: &str, value: &str) -> String {
        if let Some(existing) = self.real_to_placeholder.get(value) {
            return existing.clone();
        }

        let next_index = self.counters.get(kind).copied().unwrap_or(0) + 1;
        self.counters.insert(kind.to_string(), next_index);

        let placeholder = format!("<<MASK:{}_{}:MASK>>", kind, next_index);
        self.real_to_placeholder
            .insert(value.to_string(), placeholder.clone());
        self.placeholder_to_real
            .insert(placeholder.clone(), value.to_string());
        placeholder
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaskResult {
    pub masked_body: String,
    pub token_map: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct Masker {
    ner: NER,
}

impl Masker {
    pub fn new(ner: NER) -> Self {
        Self { ner }
    }

    pub async fn mask(&self, body: &str) -> Result<MaskResult> {
        let mut payload: Value = serde_json::from_str(body)?;
        let mut state = MappingState::default();
        self.mask_payload(&mut payload, &mut state).await?;

        Ok(MaskResult {
            masked_body: serde_json::to_string(&payload)?,
            token_map: state.placeholder_to_real,
        })
    }

    async fn mask_payload(&self, payload: &mut Value, state: &mut MappingState) -> Result<()> {
        if let Some(object) = payload.as_object_mut() {
            object.remove("session_id");

            if let Some(messages) = object.get_mut("messages").and_then(Value::as_array_mut) {
                for message in messages {
                    if let Some(content) = message.get_mut("content") {
                        self.mask_content_value(content, state).await?;
                    }
                }
            }

            if let Some(prompt) = object.get_mut("prompt") {
                match prompt {
                    Value::String(text) => {
                        *text = self.mask_text(text, state).await?;
                    }
                    Value::Array(items) => {
                        for item in items.iter_mut() {
                            if let Value::String(text) = item {
                                *text = self.mask_text(text, state).await?;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    async fn mask_content_value(&self, value: &mut Value, state: &mut MappingState) -> Result<()> {
        match value {
            Value::String(text) => {
                *text = self.mask_text(text, state).await?;
            }
            Value::Array(items) => {
                for item in items.iter_mut() {
                    match item {
                        Value::String(text) => {
                            *text = self.mask_text(text, state).await?;
                        }
                        Value::Object(map) => {
                            for key in CONTENT_PART_TEXT_KEYS {
                                if let Some(Value::String(text)) = map.get_mut(*key) {
                                    *text = self.mask_text(text, state).await?;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn mask_text(&self, text: &str, state: &mut MappingState) -> Result<String> {
        let matches = merge_detected_entities(
            text,
            detect_regex_entities(text),
            self.ner.detect_entities(text).await?,
        );
        Ok(mask_text_with_entities(text, &matches, state))
    }
}

#[derive(Clone, Debug)]
struct PrioritizedEntity {
    entity: Entity,
    priority: usize,
}

fn detect_regex_entities(text: &str) -> Vec<Entity> {
    let mut entities = Vec::new();

    push_regex_matches(text, &EMAIL_RE, "EMAIL", false, &mut entities);
    push_regex_matches(text, &PHONE_RE, "PHONE", true, &mut entities);
    push_regex_matches(text, &SSN_RE, "SSN", false, &mut entities);

    entities
}

fn push_regex_matches(
    text: &str,
    pattern: &Regex,
    kind: &str,
    require_non_digit_boundaries: bool,
    entities: &mut Vec<Entity>,
) {
    for capture in pattern.find_iter(text) {
        if require_non_digit_boundaries
            && !has_non_digit_boundaries(text, capture.start(), capture.end())
        {
            continue;
        }

        entities.push(Entity {
            text: capture.as_str().to_string(),
            kind: kind.to_string(),
            start: capture.start(),
            end: capture.end(),
        });
    }
}

fn has_non_digit_boundaries(text: &str, start: usize, end: usize) -> bool {
    let left_ok = if start == 0 {
        true
    } else {
        !text[..start]
            .chars()
            .next_back()
            .is_some_and(|character| character.is_ascii_digit())
    };

    let right_ok = if end == text.len() {
        true
    } else {
        !text[end..]
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
    };

    left_ok && right_ok
}

fn merge_detected_entities(
    text: &str,
    regex_entities: Vec<Entity>,
    ner_entities: Vec<Entity>,
) -> Vec<Entity> {
    let mut candidates: Vec<PrioritizedEntity> = regex_entities
        .into_iter()
        .map(|entity| PrioritizedEntity {
            entity,
            priority: PRIORITY_REGEX,
        })
        .chain(ner_entities.into_iter().map(|entity| PrioritizedEntity {
            entity,
            priority: PRIORITY_NER,
        }))
        .filter(|candidate| {
            candidate.entity.start < candidate.entity.end && candidate.entity.end <= text.len()
        })
        .collect();

    candidates.sort_by(|left, right| {
        let left_span = left.entity.end - left.entity.start;
        let right_span = right.entity.end - right.entity.start;
        right_span
            .cmp(&left_span)
            .then(left.priority.cmp(&right.priority))
            .then(left.entity.start.cmp(&right.entity.start))
            .then(left.entity.end.cmp(&right.entity.end))
    });

    let mut selected: Vec<PrioritizedEntity> = Vec::new();
    for candidate in candidates {
        if selected
            .iter()
            .any(|existing| entities_overlap(&existing.entity, &candidate.entity))
        {
            continue;
        }
        selected.push(candidate);
    }

    selected.sort_by_key(|candidate| candidate.entity.start);
    selected
        .into_iter()
        .map(|candidate| candidate.entity)
        .collect()
}

fn entities_overlap(left: &Entity, right: &Entity) -> bool {
    left.start < right.end && right.start < left.end
}

pub fn mask_text_with_entities(
    text: &str,
    entities: &[Entity],
    state: &mut MappingState,
) -> String {
    if entities.is_empty() {
        return text.to_string();
    }

    let mut matches: Vec<&Entity> = entities
        .iter()
        .filter(|entity| entity.start < entity.end && entity.end <= text.len())
        .collect();
    matches.sort_by_key(|entity| entity.start);

    let mut chunks = String::with_capacity(text.len());
    let mut cursor = 0;

    for entity in matches {
        if entity.start < cursor {
            continue;
        }

        chunks.push_str(&text[cursor..entity.start]);
        chunks.push_str(&state.placeholder_for(&entity.kind, &entity.text));
        cursor = entity.end;
    }

    chunks.push_str(&text[cursor..]);
    chunks
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        detect_regex_entities, has_non_digit_boundaries, mask_text_with_entities,
        merge_detected_entities, MappingState, Masker,
    };
    use crate::masker::ner::{Entity, NER};

    #[test]
    fn mask_text_reuses_placeholders_for_repeated_literals() {
        let text = "Email alice@example.com, then email alice@example.com again.";
        let entities = vec![
            Entity {
                text: "alice@example.com".to_string(),
                kind: "EMAIL".to_string(),
                start: 6,
                end: 23,
            },
            Entity {
                text: "alice@example.com".to_string(),
                kind: "EMAIL".to_string(),
                start: 36,
                end: 53,
            },
        ];
        let mut state = MappingState::default();

        let masked = mask_text_with_entities(text, &entities, &mut state);

        assert_eq!(
            masked,
            "Email <<MASK:EMAIL_1:MASK>>, then email <<MASK:EMAIL_1:MASK>> again."
        );
        assert_eq!(state.placeholder_to_real().len(), 1);
    }

    #[test]
    fn mask_text_skips_zero_length_entities() {
        let text = "Hello world";
        let entities = vec![Entity {
            text: "".to_string(),
            kind: "PERSON_NAME".to_string(),
            start: 5,
            end: 5,
        }];
        let mut state = MappingState::default();

        let masked = mask_text_with_entities(text, &entities, &mut state);

        assert_eq!(masked, "Hello world");
        assert!(state.placeholder_to_real().is_empty());
    }

    #[tokio::test]
    async fn mask_removes_session_id_and_preserves_non_masked_body() {
        let masker = Masker::new(NER::disabled());
        let body = json!({
            "session_id": "abc123",
            "messages": [{"role": "user", "content": "Hello world"}],
        })
        .to_string();

        let masked = masker.mask(&body).await.unwrap();
        let payload: serde_json::Value = serde_json::from_str(&masked.masked_body).unwrap();

        assert!(payload.get("session_id").is_none());
        assert_eq!(payload["messages"][0]["content"], "Hello world");
        assert!(masked.token_map.is_empty());
    }

    #[test]
    fn detect_regex_entities_finds_email_phone_and_ssn() {
        let entities =
            detect_regex_entities("Email alice@example.com, call 415-555-2671, SSN 123-45-6789");

        assert!(entities
            .iter()
            .any(|entity| entity.kind == "EMAIL" && entity.text == "alice@example.com"));
        assert!(entities
            .iter()
            .any(|entity| entity.kind == "PHONE" && entity.text == "415-555-2671"));
        assert!(entities
            .iter()
            .any(|entity| entity.kind == "SSN" && entity.text == "123-45-6789"));
    }

    #[test]
    fn phone_boundary_check_rejects_digits_touching_match() {
        assert!(has_non_digit_boundaries("x415-555-2671y", 1, 13));
        assert!(!has_non_digit_boundaries("9415-555-26710", 1, 13));
    }

    #[test]
    fn merge_detected_entities_prefers_longest_span_over_nested_regex_match() {
        let text = "Alice Smith at alice@example.com";
        let regex_entities = vec![Entity {
            text: "alice@example.com".to_string(),
            kind: "EMAIL".to_string(),
            start: 15,
            end: 32,
        }];
        let ner_entities = vec![Entity {
            text: "Alice Smith at alice@example.com".to_string(),
            kind: "PERSON_NAME".to_string(),
            start: 0,
            end: 32,
        }];

        let merged = merge_detected_entities(text, regex_entities, ner_entities);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].kind, "PERSON_NAME");
    }

    #[test]
    fn merge_detected_entities_keeps_adjacent_regex_and_ner_spans() {
        let text = "John alice@example.com";
        let regex_entities = vec![Entity {
            text: "alice@example.com".to_string(),
            kind: "EMAIL".to_string(),
            start: 5,
            end: 22,
        }];
        let ner_entities = vec![Entity {
            text: "John".to_string(),
            kind: "PERSON_NAME".to_string(),
            start: 0,
            end: 4,
        }];

        let merged = merge_detected_entities(text, regex_entities, ner_entities);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].kind, "PERSON_NAME");
        assert_eq!(merged[1].kind, "EMAIL");
    }

    #[tokio::test]
    async fn mask_uses_regex_and_ner_together() {
        let masker = Masker::new(NER::disabled());
        let body = json!({
            "messages": [{
                "role": "user",
                "content": "Email alice@example.com and call 415-555-2671."
            }],
        })
        .to_string();

        let masked = masker.mask(&body).await.unwrap();
        let payload: serde_json::Value = serde_json::from_str(&masked.masked_body).unwrap();
        let content = payload["messages"][0]["content"].as_str().unwrap();

        assert_eq!(
            content,
            "Email <<MASK:EMAIL_1:MASK>> and call <<MASK:PHONE_1:MASK>>."
        );
        assert_eq!(
            masked.token_map.get("<<MASK:EMAIL_1:MASK>>"),
            Some(&"alice@example.com".to_string())
        );
        assert_eq!(
            masked.token_map.get("<<MASK:PHONE_1:MASK>>"),
            Some(&"415-555-2671".to_string())
        );
    }

    // -----------------------------------------------------------------------
    // Extended regex evaluation suite
    // -----------------------------------------------------------------------

    fn assert_detected(text: &str, kind: &str, expected: &str) {
        let entities = detect_regex_entities(text);
        assert!(
            entities
                .iter()
                .any(|e| e.kind == kind && e.text == expected),
            "Expected {kind}={expected:?} in {text:?}, got: {entities:?}"
        );
    }

    fn assert_not_detected_kind(text: &str, kind: &str) {
        let entities = detect_regex_entities(text);
        assert!(
            !entities.iter().any(|e| e.kind == kind),
            "Expected no {kind} in {text:?}, but got: {:?}",
            entities
                .iter()
                .filter(|e| e.kind == kind)
                .collect::<Vec<_>>()
        );
    }

    // -- EMAIL evals --

    #[test]
    fn email_lowercase() {
        assert_detected("contact bob@widgets.io today", "EMAIL", "bob@widgets.io");
    }

    #[test]
    fn email_uppercase() {
        assert_detected("send to BOB@WIDGETS.IO", "EMAIL", "BOB@WIDGETS.IO");
    }

    #[test]
    fn email_mixed_case() {
        assert_detected(
            "reach Alice.Smith@Example.COM",
            "EMAIL",
            "Alice.Smith@Example.COM",
        );
    }

    #[test]
    fn email_with_plus_tag() {
        assert_detected(
            "use user+tag@gmail.com for signup",
            "EMAIL",
            "user+tag@gmail.com",
        );
    }

    #[test]
    fn email_with_subdomain() {
        assert_detected(
            "write to hr@mail.corp.example.co.uk",
            "EMAIL",
            "hr@mail.corp.example.co.uk",
        );
    }

    #[test]
    fn email_numeric_local() {
        assert_detected(
            "id 12345@example.com is active",
            "EMAIL",
            "12345@example.com",
        );
    }

    #[test]
    fn email_multiple_in_text() {
        let entities = detect_regex_entities("a@b.com and c@d.org are both valid");
        let emails: Vec<&str> = entities
            .iter()
            .filter(|e| e.kind == "EMAIL")
            .map(|e| e.text.as_str())
            .collect();
        assert!(emails.contains(&"a@b.com"), "missing a@b.com: {emails:?}");
        assert!(emails.contains(&"c@d.org"), "missing c@d.org: {emails:?}");
    }

    #[test]
    fn email_not_detected_plain_at() {
        assert_not_detected_kind("meet me @ the cafe at 5pm", "EMAIL");
    }

    #[test]
    fn email_not_detected_no_tld() {
        assert_not_detected_kind("user@localhost", "EMAIL");
    }

    // -- PHONE evals --

    #[test]
    fn phone_dashes() {
        assert_detected("call 212-555-1234 now", "PHONE", "212-555-1234");
    }

    #[test]
    fn phone_dots() {
        assert_detected("call 212.555.1234 now", "PHONE", "212.555.1234");
    }

    #[test]
    fn phone_spaces() {
        assert_detected("call 212 555 1234 now", "PHONE", "212 555 1234");
    }

    #[test]
    fn phone_parens() {
        assert_detected("call (212) 555-1234 now", "PHONE", "(212) 555-1234");
    }

    #[test]
    fn phone_with_country_code() {
        assert_detected("call +1-212-555-1234 for info", "PHONE", "+1-212-555-1234");
    }

    #[test]
    fn phone_country_code_no_sep() {
        assert_detected("call 1-212-555-1234 today", "PHONE", "1-212-555-1234");
    }

    #[test]
    fn phone_multiple() {
        let entities = detect_regex_entities("call 212-555-1234 or 415-555-6789");
        let phones: Vec<&str> = entities
            .iter()
            .filter(|e| e.kind == "PHONE")
            .map(|e| e.text.as_str())
            .collect();
        assert!(
            phones.contains(&"212-555-1234"),
            "missing 212 phone: {phones:?}"
        );
        assert!(
            phones.contains(&"415-555-6789"),
            "missing 415 phone: {phones:?}"
        );
    }

    #[test]
    fn phone_not_detected_short_number() {
        assert_not_detected_kind("call 555-1234 for help", "PHONE");
    }

    #[test]
    fn phone_not_detected_embedded_in_longer_digits() {
        // 16-digit credit card should NOT be matched as a phone
        assert_not_detected_kind("card 4111111111111111", "PHONE");
    }

    // -- SSN evals --

    #[test]
    fn ssn_standard() {
        assert_detected("SSN is 123-45-6789.", "SSN", "123-45-6789");
    }

    #[test]
    fn ssn_multiple() {
        let entities = detect_regex_entities("SSN 123-45-6789 and 987-65-4321");
        let ssns: Vec<&str> = entities
            .iter()
            .filter(|e| e.kind == "SSN")
            .map(|e| e.text.as_str())
            .collect();
        assert!(ssns.contains(&"123-45-6789"), "missing first SSN: {ssns:?}");
        assert!(
            ssns.contains(&"987-65-4321"),
            "missing second SSN: {ssns:?}"
        );
    }

    #[test]
    fn ssn_not_detected_no_dashes() {
        assert_not_detected_kind("number 123456789 here", "SSN");
    }

    #[test]
    fn ssn_not_detected_wrong_grouping() {
        assert_not_detected_kind("code 1234-56-789 is invalid", "SSN");
    }

    // -- Mixed PII evals --

    #[test]
    fn mixed_email_phone_ssn_all_detected() {
        let text = "Email me at hr@acme.com, call 310-555-9876, my SSN is 111-22-3333.";
        let entities = detect_regex_entities(text);
        assert!(
            entities
                .iter()
                .any(|e| e.kind == "EMAIL" && e.text == "hr@acme.com"),
            "missing email"
        );
        assert!(
            entities
                .iter()
                .any(|e| e.kind == "PHONE" && e.text == "310-555-9876"),
            "missing phone"
        );
        assert!(
            entities
                .iter()
                .any(|e| e.kind == "SSN" && e.text == "111-22-3333"),
            "missing ssn"
        );
    }

    #[test]
    fn mixed_repeated_email_gets_same_placeholder() {
        let text = "Send to hr@acme.com and cc hr@acme.com";
        let mut state = MappingState::default();

        let entities = detect_regex_entities(text);
        let merged = merge_detected_entities(text, entities, vec![]);
        let masked = mask_text_with_entities(text, &merged, &mut state);

        assert_eq!(
            masked,
            "Send to <<MASK:EMAIL_1:MASK>> and cc <<MASK:EMAIL_1:MASK>>"
        );
        assert_eq!(state.placeholder_to_real().len(), 1);
    }

    // -- No PII (false positive checks) --

    #[test]
    fn no_pii_plain_text() {
        let entities = detect_regex_entities("The weather is nice today.");
        assert!(entities.is_empty(), "false positives: {entities:?}");
    }

    #[test]
    fn no_pii_numbers_in_text() {
        let entities = detect_regex_entities("Order #12345 has 3 items totaling $99.99");
        assert!(entities.is_empty(), "false positives: {entities:?}");
    }

    #[test]
    fn no_pii_url_not_email() {
        // A URL shouldn't be detected as an email
        assert_not_detected_kind("visit https://example.com/path?q=1", "EMAIL");
    }

    // -- End-to-end masker eval --

    #[tokio::test]
    async fn mask_all_pii_types_in_messages() {
        let masker = Masker::new(NER::disabled());
        let body = json!({
            "messages": [{
                "role": "user",
                "content": "My email is test@example.org, phone 650-555-0199, SSN 999-88-7777."
            }],
        })
        .to_string();

        let masked = masker.mask(&body).await.unwrap();
        let payload: serde_json::Value = serde_json::from_str(&masked.masked_body).unwrap();
        let content = payload["messages"][0]["content"].as_str().unwrap();

        assert!(!content.contains("test@example.org"), "email leaked");
        assert!(!content.contains("650-555-0199"), "phone leaked");
        assert!(!content.contains("999-88-7777"), "ssn leaked");
        assert!(
            content.contains("<<MASK:EMAIL_1:MASK>>"),
            "email not masked"
        );
        assert!(
            content.contains("<<MASK:PHONE_1:MASK>>"),
            "phone not masked"
        );
        assert!(content.contains("<<MASK:SSN_1:MASK>>"), "ssn not masked");
        assert_eq!(masked.token_map.len(), 3);
    }

    #[tokio::test]
    async fn mask_prompt_field_also_masked() {
        let masker = Masker::new(NER::disabled());
        let body = json!({
            "prompt": "Summarize for alice@corp.com"
        })
        .to_string();

        let masked = masker.mask(&body).await.unwrap();
        let payload: serde_json::Value = serde_json::from_str(&masked.masked_body).unwrap();
        let prompt = payload["prompt"].as_str().unwrap();

        assert!(!prompt.contains("alice@corp.com"), "email leaked in prompt");
        assert!(
            prompt.contains("<<MASK:EMAIL_1:MASK>>"),
            "email not masked in prompt"
        );
    }

    #[tokio::test]
    async fn mask_no_pii_returns_empty_token_map() {
        let masker = Masker::new(NER::disabled());
        let body = json!({
            "messages": [{"role": "user", "content": "What is the capital of France?"}],
        })
        .to_string();

        let masked = masker.mask(&body).await.unwrap();
        assert!(masked.token_map.is_empty());
    }

    #[tokio::test]
    async fn mask_content_array_with_text_objects() {
        let masker = Masker::new(NER::disabled());
        let body = json!({
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "Email support@test.io please"},
                    {"type": "text", "text": "Phone is 800-555-1212"}
                ]
            }],
        })
        .to_string();

        let masked = masker.mask(&body).await.unwrap();
        assert!(
            masked.token_map.contains_key("<<MASK:EMAIL_1:MASK>>"),
            "email not in token map"
        );
        assert!(
            masked.token_map.contains_key("<<MASK:PHONE_1:MASK>>"),
            "phone not in token map"
        );
    }
}
