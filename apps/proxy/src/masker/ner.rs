use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context, Result};
use ort::{session::Session, value::TensorRef};
use serde_json::Value;
use tokenizers::Tokenizer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entity {
    pub text: String,
    pub kind: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct NerPrediction {
    start: usize,
    end: usize,
    label: String,
    score: f32,
    text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParsedEntityLabel {
    prefix: String,
    kind: String,
}

#[derive(Clone, Debug, PartialEq)]
struct TokenLabelScore {
    label: String,
    score: f32,
    start: usize,
    end: usize,
}

#[derive(Debug)]
struct OnnxTokenClassificationBackend {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    id_to_label: HashMap<usize, String>,
    has_token_type_ids: bool,
}

#[derive(Clone, Debug)]
pub struct NER {
    backend: Option<Arc<OnnxTokenClassificationBackend>>,
}

impl NER {
    pub fn new(model_path: &str) -> Result<Self> {
        if model_path.trim().is_empty() {
            return Ok(Self::disabled());
        }

        let model_path = PathBuf::from(model_path);
        if !model_path.exists() {
            return Err(anyhow!(
                "NER model file not found: {}",
                model_path.display()
            ));
        }

        let artifact_dir = model_path
            .parent()
            .context("NER model path must have a parent directory")?;
        let tokenizer_path = artifact_dir.join("tokenizer.json");
        let labels_path = artifact_dir.join("labels.json");

        if !tokenizer_path.exists() {
            return Err(anyhow!(
                "NER tokenizer file not found next to model: {}",
                tokenizer_path.display()
            ));
        }
        if !labels_path.exists() {
            return Err(anyhow!(
                "NER labels file not found next to model: {}",
                labels_path.display()
            ));
        }

        let backend =
            OnnxTokenClassificationBackend::new(&model_path, &tokenizer_path, &labels_path)?;
        Ok(Self {
            backend: Some(Arc::new(backend)),
        })
    }

    pub fn disabled() -> Self {
        Self { backend: None }
    }

    pub fn is_disabled(&self) -> bool {
        self.backend.is_none()
    }

    pub async fn detect_entities(&self, text: &str, threshold: f32) -> Result<Vec<Entity>> {
        let Some(backend) = &self.backend else {
            return Ok(Vec::new());
        };

        let predictions = backend.predict(text)?;
        let entities = predictions
            .into_iter()
            .filter(|prediction| prediction.score >= threshold)
            .filter_map(|prediction| {
                normalize_label(&prediction.label).map(|kind| Entity {
                    text: text[prediction.start..prediction.end].to_string(),
                    kind: kind.to_string(),
                    start: prediction.start,
                    end: prediction.end,
                })
            })
            .collect();

        Ok(entities)
    }
}

impl OnnxTokenClassificationBackend {
    fn new(model_path: &Path, tokenizer_path: &Path, labels_path: &Path) -> Result<Self> {
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|error| anyhow!("failed to load tokenizer: {error}"))?;
        let id_to_label = load_labels(labels_path)?;

        let session = Session::builder()?
            .commit_from_file(model_path)
            .with_context(|| format!("failed to load ONNX model from {}", model_path.display()))?;
        let has_token_type_ids = session
            .inputs()
            .iter()
            .any(|input| input.name() == "token_type_ids");

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            id_to_label,
            has_token_type_ids,
        })
    }

    fn predict(&self, text: &str) -> Result<Vec<NerPrediction>> {
        if text.is_empty() {
            return Ok(Vec::new());
        }

        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|error| anyhow!("failed to tokenize NER input: {error}"))?;

        let input_ids: Vec<i64> = encoding
            .get_ids()
            .iter()
            .map(|&value| i64::from(value))
            .collect();
        let input_ids_shape = vec![1_usize, input_ids.len()];

        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&value| i64::from(value))
            .collect();
        let attention_mask_shape = vec![1_usize, attention_mask.len()];

        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow!("NER ONNX session mutex poisoned"))?;

        let outputs = if self.has_token_type_ids {
            let token_type_ids: Vec<i64> = encoding
                .get_type_ids()
                .iter()
                .map(|&value| i64::from(value))
                .collect();
            let token_type_ids_shape = vec![1_usize, token_type_ids.len()];

            session.run(ort::inputs! {
                "input_ids" => TensorRef::from_array_view((input_ids_shape.as_slice(), input_ids.as_slice()))?,
                "attention_mask" => TensorRef::from_array_view((attention_mask_shape.as_slice(), attention_mask.as_slice()))?,
                "token_type_ids" => TensorRef::from_array_view((token_type_ids_shape.as_slice(), token_type_ids.as_slice()))?,
            })?
        } else {
            session.run(ort::inputs! {
                "input_ids" => TensorRef::from_array_view((input_ids_shape.as_slice(), input_ids.as_slice()))?,
                "attention_mask" => TensorRef::from_array_view((attention_mask_shape.as_slice(), attention_mask.as_slice()))?,
            })?
        };

        let (shape, logits) = outputs[0].try_extract_tensor::<f32>()?;
        let dims: Vec<usize> = shape.iter().map(|dimension| *dimension as usize).collect();
        if dims.len() != 3 || dims[0] == 0 || dims[1] == 0 || dims[2] == 0 {
            return Ok(Vec::new());
        }

        let seq_len = dims[1];
        let num_labels = dims[2];
        let offsets = encoding.get_offsets();
        let mut token_predictions = Vec::new();

        for token_index in 0..seq_len.min(offsets.len()) {
            let start_offset = token_index * num_labels;
            let row = &logits[start_offset..start_offset + num_labels];
            let probabilities = softmax(row);
            let label_index = argmax(&probabilities);

            let Some(label) = self.id_to_label.get(&label_index).cloned() else {
                continue;
            };
            let (start, end) = offsets[token_index];
            if start == end {
                continue;
            }

            token_predictions.push(TokenLabelScore {
                label,
                score: probabilities[label_index],
                start,
                end,
            });
        }

        Ok(merge_token_predictions(text, &token_predictions))
    }
}

fn load_labels(path: &Path) -> Result<HashMap<usize, String>> {
    let raw: Value = serde_json::from_str(
        &std::fs::read_to_string(path)
            .with_context(|| format!("failed to read label map from {}", path.display()))?,
    )?;

    if let Some(items) = raw.as_array() {
        return Ok(items
            .iter()
            .enumerate()
            .filter_map(|(index, value)| value.as_str().map(|label| (index, label.to_string())))
            .collect());
    }

    if let Some(object) = raw.as_object() {
        if object.keys().all(|key| key.parse::<usize>().is_ok()) {
            return Ok(object
                .iter()
                .filter_map(|(key, value)| {
                    Some((key.parse::<usize>().ok()?, value.as_str()?.to_string()))
                })
                .collect());
        }

        if object.values().all(|value| value.as_u64().is_some()) {
            return Ok(object
                .iter()
                .filter_map(|(key, value)| Some((value.as_u64()? as usize, key.to_string())))
                .collect());
        }
    }

    Err(anyhow!("unsupported ONNX label map format"))
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max_value = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits
        .iter()
        .map(|value| (*value - max_value).exp())
        .collect();
    let total: f32 = exps.iter().sum();
    exps.into_iter().map(|value| value / total).collect()
}

fn argmax(values: &[f32]) -> usize {
    let mut best_index = 0;
    let mut best_value = values[0];
    for (index, value) in values.iter().copied().enumerate().skip(1) {
        if value > best_value {
            best_index = index;
            best_value = value;
        }
    }
    best_index
}

fn parse_label(label: &str) -> ParsedEntityLabel {
    let normalized = label.to_uppercase();
    if let Some((prefix, suffix)) = normalized.split_once('-') {
        if matches!(prefix, "B" | "I" | "L" | "U" | "E" | "S") {
            return ParsedEntityLabel {
                prefix: prefix.to_string(),
                kind: suffix.to_string(),
            };
        }
    }

    ParsedEntityLabel {
        prefix: String::new(),
        kind: normalized,
    }
}

fn should_extend_entity(
    parsed: &ParsedEntityLabel,
    current_kind: &str,
    start: usize,
    current_end: Option<usize>,
) -> bool {
    let Some(current_end) = current_end else {
        return false;
    };

    if parsed.kind != current_kind {
        return false;
    }

    if matches!(parsed.prefix.as_str(), "I" | "L" | "E") {
        return start >= current_end;
    }

    if matches!(parsed.prefix.as_str(), "B" | "") {
        return start >= current_end && (start - current_end) <= 1;
    }

    false
}

fn merge_token_predictions(text: &str, tokens: &[TokenLabelScore]) -> Vec<NerPrediction> {
    let mut predictions = Vec::new();
    let mut current_start: Option<usize> = None;
    let mut current_end: Option<usize> = None;
    let mut current_kind: Option<String> = None;
    let mut current_scores: Vec<f32> = Vec::new();

    for token in tokens {
        let parsed = parse_label(&token.label);
        if parsed.kind == "O" {
            flush_prediction(
                text,
                &mut predictions,
                &mut current_start,
                &mut current_end,
                &mut current_kind,
                &mut current_scores,
            );
            continue;
        }

        if current_kind.is_none() {
            current_start = Some(token.start);
            current_end = Some(token.end);
            current_kind = Some(parsed.kind);
            current_scores = vec![token.score];
            continue;
        }

        if should_extend_entity(
            &parsed,
            current_kind.as_deref().unwrap_or_default(),
            token.start,
            current_end,
        ) {
            current_end = Some(current_end.unwrap_or(token.end).max(token.end));
            current_scores.push(token.score);
            continue;
        }

        flush_prediction(
            text,
            &mut predictions,
            &mut current_start,
            &mut current_end,
            &mut current_kind,
            &mut current_scores,
        );
        current_start = Some(token.start);
        current_end = Some(token.end);
        current_kind = Some(parsed.kind);
        current_scores = vec![token.score];
    }

    flush_prediction(
        text,
        &mut predictions,
        &mut current_start,
        &mut current_end,
        &mut current_kind,
        &mut current_scores,
    );

    predictions
}

fn flush_prediction(
    text: &str,
    predictions: &mut Vec<NerPrediction>,
    current_start: &mut Option<usize>,
    current_end: &mut Option<usize>,
    current_kind: &mut Option<String>,
    current_scores: &mut Vec<f32>,
) {
    let (Some(start), Some(end), Some(kind)) = (
        current_start.take(),
        current_end.take(),
        current_kind.take(),
    ) else {
        current_scores.clear();
        return;
    };

    if start >= end || current_scores.is_empty() {
        current_scores.clear();
        return;
    }

    let score = current_scores.iter().sum::<f32>() / current_scores.len() as f32;
    predictions.push(NerPrediction {
        start,
        end,
        label: kind,
        score,
        text: text[start..end].to_string(),
    });
    current_scores.clear();
}

fn normalize_label(label: &str) -> Option<&'static str> {
    match label.to_uppercase().as_str() {
        "PER" | "PERSON" | "PERSON_NAME" => Some("PERSON_NAME"),
        "LOC" | "LOCATION" => Some("LOCATION"),
        "ORG" | "ORGANIZATION" => Some("ORGANIZATION"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        argmax, merge_token_predictions, normalize_label, parse_label, should_extend_entity,
        softmax, TokenLabelScore,
    };

    #[test]
    fn normalize_label_keeps_supported_entities() {
        assert_eq!(normalize_label("PER"), Some("PERSON_NAME"));
        assert_eq!(normalize_label("PERSON"), Some("PERSON_NAME"));
        assert_eq!(normalize_label("LOC"), Some("LOCATION"));
        assert_eq!(normalize_label("ORG"), Some("ORGANIZATION"));
        assert_eq!(normalize_label("MISC"), None);
    }

    #[test]
    fn parse_label_understands_bio_prefixes() {
        let parsed = parse_label("B-PER");
        assert_eq!(parsed.prefix, "B");
        assert_eq!(parsed.kind, "PER");

        let parsed = parse_label("PER");
        assert_eq!(parsed.prefix, "");
        assert_eq!(parsed.kind, "PER");
    }

    #[test]
    fn should_extend_entity_merges_split_b_tags() {
        let parsed = parse_label("B-PER");
        assert!(should_extend_entity(&parsed, "PER", 2, Some(1)));
    }

    #[test]
    fn merge_token_predictions_merges_split_person_name_tokens() {
        let text = "Hiroshi Tanaka";
        let predictions = merge_token_predictions(
            text,
            &[
                TokenLabelScore {
                    label: "B-PER".to_string(),
                    score: 0.91,
                    start: 0,
                    end: 2,
                },
                TokenLabelScore {
                    label: "B-PER".to_string(),
                    score: 0.93,
                    start: 2,
                    end: 7,
                },
                TokenLabelScore {
                    label: "I-PER".to_string(),
                    score: 0.95,
                    start: 8,
                    end: 14,
                },
            ],
        );

        assert_eq!(predictions.len(), 1);
        assert_eq!(predictions[0].label, "PER");
        assert_eq!(predictions[0].text, "Hiroshi Tanaka");
    }

    #[test]
    fn merge_token_predictions_flushes_on_o_label() {
        let text = "John met Alice";
        let predictions = merge_token_predictions(
            text,
            &[
                TokenLabelScore {
                    label: "B-PER".to_string(),
                    score: 0.9,
                    start: 0,
                    end: 4,
                },
                TokenLabelScore {
                    label: "O".to_string(),
                    score: 0.99,
                    start: 5,
                    end: 8,
                },
                TokenLabelScore {
                    label: "B-PER".to_string(),
                    score: 0.92,
                    start: 9,
                    end: 14,
                },
            ],
        );

        assert_eq!(predictions.len(), 2);
        assert_eq!(predictions[0].text, "John");
        assert_eq!(predictions[1].text, "Alice");
    }

    #[test]
    fn softmax_and_argmax_pick_highest_logit() {
        let probabilities = softmax(&[1.0, 2.0, 4.0]);
        let best_index = argmax(&probabilities);

        assert_eq!(best_index, 2);
        let total: f32 = probabilities.iter().sum();
        assert!((total - 1.0).abs() < 1e-5);
    }
}
