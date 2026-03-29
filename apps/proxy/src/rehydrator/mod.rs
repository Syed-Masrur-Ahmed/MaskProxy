use std::collections::HashMap;
use std::sync::LazyLock;

use anyhow::Result;
use regex::{Captures, Regex};
use serde_json::Value;

static PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<<MASK:[A-Z_]+_\d+:MASK>>").expect("valid placeholder regex")
});

#[derive(Clone, Default)]
pub struct Rehydrator;

impl Rehydrator {
    pub fn new() -> Self {
        Self
    }

    pub fn rehydrate_body(&self, body: &str, token_map: &HashMap<String, String>) -> Result<String> {
        let mut payload: Value = serde_json::from_str(body)?;
        self.rehydrate_value(&mut payload, token_map);
        Ok(serde_json::to_string(&payload)?)
    }

    pub fn rehydrate_text(&self, text: &str, token_map: &HashMap<String, String>) -> String {
        PLACEHOLDER_RE
            .replace_all(text, |captures: &Captures<'_>| {
                token_map
                    .get(&captures[0])
                    .cloned()
                    .unwrap_or_else(|| captures[0].to_string())
            })
            .into_owned()
    }

    fn rehydrate_value(&self, value: &mut Value, token_map: &HashMap<String, String>) {
        match value {
            Value::String(text) => {
                *text = self.rehydrate_text(text, token_map);
            }
            Value::Array(items) => {
                for item in items {
                    self.rehydrate_value(item, token_map);
                }
            }
            Value::Object(map) => {
                for item in map.values_mut() {
                    self.rehydrate_value(item, token_map);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::Rehydrator;

    #[test]
    fn rehydrates_plain_text_tokens() {
        let mut mapping = HashMap::new();
        mapping.insert("<<MASK:PERSON_NAME_1:MASK>>".to_string(), "John Smith".to_string());
        mapping.insert("<<MASK:EMAIL_1:MASK>>".to_string(), "alice@example.com".to_string());

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
        mapping.insert("<<MASK:EMAIL_1:MASK>>".to_string(), "alice@example.com".to_string());

        let rehydrated = Rehydrator::new().rehydrate_text("Hello <<MASK:PERSON_NAME_1:MASK>>", &mapping);

        assert_eq!(rehydrated, "Hello <<MASK:EMAIL_1:MASK>>");
    }

    #[test]
    fn rehydrates_nested_json_body() {
        let mut mapping = HashMap::new();
        mapping.insert("<<MASK:PERSON_NAME_1:MASK>>".to_string(), "John Smith".to_string());

        let body = r#"{"choices":[{"message":{"content":"Hello <<MASK:PERSON_NAME_1:MASK>>"}}]}"#;
        let rehydrated = Rehydrator::new().rehydrate_body(body, &mapping).unwrap();

        assert!(rehydrated.contains("John Smith"));
        assert!(!rehydrated.contains("<<MASK:PERSON_NAME_1:MASK>>"));
    }
}
