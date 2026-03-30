use std::fs;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use ort::{session::Session, value::TensorRef};
use serde::Deserialize;
use tokenizers::Tokenizer;

use super::{EmbeddingProvider, RouteTarget};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteExampleConfig {
    pub text: String,
    pub target: RouteTarget,
}

#[derive(Deserialize)]
struct RawRouteExample {
    text: String,
    target: String,
}

pub struct OnnxTextEmbeddingProvider {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    has_token_type_ids: bool,
    sentence_embedding_output_index: Option<usize>,
    token_embeddings_output_index: usize,
    normalize: bool,
}

impl OnnxTextEmbeddingProvider {
    pub fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|error| anyhow!("failed to load routing tokenizer: {error}"))?;

        let session = Session::builder()?
            .commit_from_file(model_path)
            .with_context(|| {
                format!(
                    "failed to load routing embedding model from {}",
                    model_path.display()
                )
            })?;

        let has_token_type_ids = session
            .inputs()
            .iter()
            .any(|input| input.name() == "token_type_ids");
        let sentence_embedding_output_index = session
            .outputs()
            .iter()
            .position(|output| output.name() == "sentence_embedding");
        let token_embeddings_output_index = session
            .outputs()
            .iter()
            .position(|output| output.name() == "token_embeddings")
            .unwrap_or(0);

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            has_token_type_ids,
            sentence_embedding_output_index,
            token_embeddings_output_index,
            normalize: true,
        })
    }
}

impl EmbeddingProvider for OnnxTextEmbeddingProvider {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }

        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|error| anyhow!("failed to tokenize routing input: {error}"))?;

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&value| i64::from(value)).collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&value| i64::from(value))
            .collect();
        let input_ids_shape = vec![1_usize, input_ids.len()];
        let attention_mask_shape = vec![1_usize, attention_mask.len()];

        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow!("routing embedding ONNX session mutex poisoned"))?;

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

        let mut vector = if let Some(output_index) = self.sentence_embedding_output_index {
            extract_sentence_embedding(&outputs[output_index])?
        } else {
            mean_pool_token_embeddings(&outputs[self.token_embeddings_output_index], &attention_mask)?
        };

        if self.normalize {
            l2_normalize(&mut vector);
        }

        Ok(vector)
    }
}

pub fn load_route_examples(path: &Path) -> Result<Vec<RouteExampleConfig>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read routing examples from {}", path.display()))?;
    let entries: Vec<RawRouteExample> = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse routing examples JSON at {}", path.display()))?;

    let examples: Vec<RouteExampleConfig> = entries
        .into_iter()
        .filter_map(|entry| {
            let text = entry.text.trim().to_string();
            if text.is_empty() {
                return None;
            }

            let target = match entry.target.trim().to_ascii_lowercase().as_str() {
                "local" => RouteTarget::Local,
                "cloud" => RouteTarget::Cloud,
                _ => return None,
            };

            Some(RouteExampleConfig { text, target })
        })
        .collect();

    if examples.is_empty() {
        return Err(anyhow!("no valid routing examples found in {}", path.display()));
    }

    Ok(examples)
}

fn extract_sentence_embedding(output: &ort::value::DynValue) -> Result<Vec<f32>> {
    let (shape, values) = output.try_extract_tensor::<f32>()?;
    let dims: Vec<usize> = shape.iter().map(|dimension| *dimension as usize).collect();

    match dims.as_slice() {
        [hidden] if *hidden > 0 => Ok(values[..*hidden].to_vec()),
        [1, hidden] if *hidden > 0 => Ok(values[..*hidden].to_vec()),
        [1, 1, hidden] if *hidden > 0 => Ok(values[..*hidden].to_vec()),
        _ => Err(anyhow!(
            "unexpected sentence_embedding shape {:?} from routing model",
            dims
        )),
    }
}

fn mean_pool_token_embeddings(
    output: &ort::value::DynValue,
    attention_mask: &[i64],
) -> Result<Vec<f32>> {
    let (shape, values) = output.try_extract_tensor::<f32>()?;
    let dims: Vec<usize> = shape.iter().map(|dimension| *dimension as usize).collect();

    if dims.len() != 3 || dims[0] == 0 || dims[1] == 0 || dims[2] == 0 {
        return Err(anyhow!(
            "unexpected token_embeddings shape {:?} from routing model",
            dims
        ));
    }

    let seq_len = dims[1];
    let hidden = dims[2];
    let mut pooled = vec![0.0_f32; hidden];
    let mut token_count = 0.0_f32;

    for token_index in 0..seq_len.min(attention_mask.len()) {
        if attention_mask[token_index] == 0 {
            continue;
        }

        let offset = token_index * hidden;
        for hidden_index in 0..hidden {
            pooled[hidden_index] += values[offset + hidden_index];
        }
        token_count += 1.0;
    }

    if token_count == 0.0 {
        return Ok(Vec::new());
    }

    for value in &mut pooled {
        *value /= token_count;
    }

    Ok(pooled)
}

fn l2_normalize(vector: &mut [f32]) {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return;
    }

    for value in vector {
        *value /= norm;
    }
}
