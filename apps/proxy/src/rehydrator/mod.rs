use std::collections::HashMap;
use std::sync::LazyLock;

use anyhow::Result;
use regex::{Captures, Regex};
use serde_json::Value;

static PLACEHOLDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<<MASK:[A-Z_]+_\d+:MASK>>").expect("valid placeholder regex"));

/// Maximum length of a placeholder token. Used to bound the carry-over buffer
/// so we never hold more data than could possibly form one placeholder.
/// Format: `<<MASK:` (7) + type_name + `_` + digits + `:MASK>>` (8)
/// A generous upper bound — real placeholders are much shorter.
const MAX_PLACEHOLDER_LEN: usize = 64;

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

// ---------------------------------------------------------------------------
// StreamingRehydrator — low-level, operates on plain text
// ---------------------------------------------------------------------------

/// Streaming rehydrator that processes text chunks incrementally.
///
/// Maintains a carry-over buffer for data that might contain a partial
/// placeholder spanning chunk boundaries. On each call to [`process_chunk`],
/// complete placeholders are replaced and emittable bytes returned immediately.
/// Any trailing bytes that could be the prefix of a placeholder are held back
/// until the next chunk (or [`flush`]) completes or rules them out.
#[derive(Clone, Debug, Default)]
pub struct StreamingRehydrator {
    carry: String,
}

impl StreamingRehydrator {
    pub fn new() -> Self {
        Self {
            carry: String::new(),
        }
    }

    /// Feed a new text chunk. Returns the bytes safe to emit. Any trailing
    /// partial placeholder is held in the internal carry buffer.
    pub fn process_chunk(
        &mut self,
        chunk: &str,
        token_map: &HashMap<String, String>,
    ) -> String {
        let mut buf = std::mem::take(&mut self.carry);
        buf.push_str(chunk);

        let replaced = replace_placeholders(&buf, token_map);

        let split = find_safe_split(&replaced);
        if split < replaced.len() {
            self.carry = replaced[split..].to_string();
            replaced[..split].to_string()
        } else {
            replaced
        }
    }

    /// Flush any remaining carry-over. Call at end-of-stream.
    pub fn flush(&mut self, token_map: &HashMap<String, String>) -> String {
        if self.carry.is_empty() {
            return String::new();
        }
        let remaining = std::mem::take(&mut self.carry);
        replace_placeholders(&remaining, token_map)
    }
}

// ---------------------------------------------------------------------------
// SseRehydrator — SSE-event-aware, wraps StreamingRehydrator
// ---------------------------------------------------------------------------

/// SSE-aware streaming rehydrator. Buffers raw bytes until complete SSE events
/// are formed, then extracts content deltas from the JSON payload, feeds them
/// through a [`StreamingRehydrator`] (which handles partial placeholders across
/// events), and reconstructs the SSE events with rehydrated content.
///
/// This is necessary because LLM APIs stream tokens one at a time, so a
/// placeholder like `<<MASK:PERSON_1:MASK>>` can be split across multiple SSE
/// events (each carrying a fragment inside its JSON content field).
#[derive(Clone, Debug, Default)]
pub struct SseRehydrator {
    /// Raw bytes waiting for a complete SSE event (`\n\n` terminator).
    event_buf: String,
    /// Handles partial placeholders spanning across content deltas.
    content_rehydrator: StreamingRehydrator,
}

impl SseRehydrator {
    pub fn new() -> Self {
        Self {
            event_buf: String::new(),
            content_rehydrator: StreamingRehydrator::new(),
        }
    }

    /// Feed a raw SSE chunk (may contain partial or multiple events).
    /// Returns rehydrated SSE events that are ready to emit downstream.
    pub fn process_chunk(
        &mut self,
        chunk: &str,
        token_map: &HashMap<String, String>,
    ) -> String {
        self.event_buf.push_str(chunk);
        let mut output = String::new();

        // Process all complete SSE events (terminated by \n\n).
        while let Some(end) = self.event_buf.find("\n\n") {
            let event_end = end + 2;
            let event: String = self.event_buf[..event_end].to_string();
            self.event_buf = self.event_buf[event_end..].to_string();

            output.push_str(&self.process_sse_event(&event, token_map));
        }

        output
    }

    /// Flush remaining data at end-of-stream.
    pub fn flush(&mut self, token_map: &HashMap<String, String>) -> String {
        let mut output = String::new();

        // Process any remaining complete events.
        while let Some(end) = self.event_buf.find("\n\n") {
            let event_end = end + 2;
            let event: String = self.event_buf[..event_end].to_string();
            self.event_buf = self.event_buf[event_end..].to_string();
            output.push_str(&self.process_sse_event(&event, token_map));
        }

        // Emit any incomplete event buffer as-is.
        if !self.event_buf.is_empty() {
            output.push_str(&std::mem::take(&mut self.event_buf));
        }

        // Flush the content rehydrator — emits any partial placeholder as-is.
        let flushed_content = self.content_rehydrator.flush(token_map);
        if !flushed_content.is_empty() {
            // Wrap residual content in a synthetic SSE event so the client can
            // still parse it. This only triggers for malformed responses where
            // a placeholder was never completed.
            output.push_str(&format!(
                "data: {}\n\n",
                serde_json::to_string(&serde_json::json!({
                    "choices": [{"delta": {"content": flushed_content}}]
                }))
                .unwrap_or_default()
            ));
        }

        output
    }

    fn process_sse_event(
        &mut self,
        event: &str,
        token_map: &HashMap<String, String>,
    ) -> String {
        // Extract the data payload after "data: "
        let data = match event.strip_prefix("data: ") {
            Some(d) => d.trim_end_matches('\n'),
            None => return event.to_string(), // comment or other SSE field — pass through
        };

        // Non-JSON terminal events like [DONE] — pass through.
        if !data.starts_with('{') {
            return event.to_string();
        }

        // Parse JSON payload.
        let mut json: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => return event.to_string(),
        };

        // Extract the content delta text.
        let content = match extract_content_delta(&json) {
            Some(c) => c,
            None => return event.to_string(), // no content field — pass through
        };

        // Feed content through the streaming rehydrator.
        let rehydrated = self.content_rehydrator.process_chunk(&content, token_map);

        // Write the rehydrated content back into the JSON.
        set_content_delta(&mut json, &rehydrated);

        format!(
            "data: {}\n\n",
            serde_json::to_string(&json).unwrap_or_else(|_| data.to_string())
        )
    }
}

/// Extract the content delta string from an SSE JSON payload.
/// Supports OpenAI (`choices[0].delta.content`) and Anthropic (`delta.text`).
fn extract_content_delta(json: &Value) -> Option<String> {
    // OpenAI: choices[].delta.content
    if let Some(choices) = json.get("choices").and_then(Value::as_array) {
        for choice in choices {
            if let Some(content) = choice
                .pointer("/delta/content")
                .and_then(Value::as_str)
            {
                return Some(content.to_string());
            }
        }
    }
    // Anthropic: delta.text (content_block_delta events)
    if let Some(text) = json.pointer("/delta/text").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    None
}

/// Write the rehydrated content back into the JSON payload.
fn set_content_delta(json: &mut Value, content: &str) {
    // OpenAI
    if let Some(choices) = json.get_mut("choices").and_then(Value::as_array_mut) {
        for choice in choices.iter_mut() {
            if let Some(c) = choice.pointer_mut("/delta/content") {
                *c = Value::String(content.to_string());
                return;
            }
        }
    }
    // Anthropic
    if let Some(t) = json.pointer_mut("/delta/text") {
        *t = Value::String(content.to_string());
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn replace_placeholders(text: &str, token_map: &HashMap<String, String>) -> String {
    PLACEHOLDER_RE
        .replace_all(text, |captures: &Captures<'_>| {
            token_map
                .get(&captures[0])
                .cloned()
                .unwrap_or_else(|| captures[0].to_string())
        })
        .into_owned()
}

/// Find the byte index at which it's safe to split the buffer — everything
/// before this index can be emitted, everything from this index onward might
/// be the prefix of a `<<MASK:...:MASK>>` placeholder.
fn find_safe_split(s: &str) -> usize {
    let search_start = s.len().saturating_sub(MAX_PLACEHOLDER_LEN);
    let tail = &s[search_start..];

    for (rel_pos, ch) in tail.char_indices() {
        if ch == '<' {
            let abs_pos = search_start + rel_pos;
            let candidate = &s[abs_pos..];
            if is_placeholder_prefix(candidate) {
                return abs_pos;
            }
        }
    }

    s.len()
}

/// Returns true if `s` is a valid prefix of the pattern `<<MASK:...:MASK>>`.
fn is_placeholder_prefix(s: &str) -> bool {
    let expected_prefix = "<<MASK:";

    if s.len() <= expected_prefix.len() {
        return expected_prefix.starts_with(s);
    }

    if !s.starts_with(expected_prefix) {
        return false;
    }

    let rest = &s[expected_prefix.len()..];
    for ch in rest.chars() {
        if !ch.is_ascii_uppercase() && ch != '_' && !ch.is_ascii_digit() && ch != ':' && ch != '>'
        {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests;
