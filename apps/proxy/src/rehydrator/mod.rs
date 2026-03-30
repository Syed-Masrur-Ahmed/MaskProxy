use std::collections::HashMap;
use std::sync::LazyLock;

use anyhow::Result;
use regex::{Captures, Regex};
use serde_json::Value;

static PLACEHOLDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<<MASK:[A-Z_]+_\d+:MASK>>").expect("valid placeholder regex"));

#[derive(Clone, Default)]
pub struct Rehydrator;

impl Rehydrator {
    pub fn new() -> Self {
        Self
    }

    pub fn rehydrate_body(
        &self,
        body: &str,
        token_map: &HashMap<String, String>,
    ) -> Result<String> {
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
mod tests;
