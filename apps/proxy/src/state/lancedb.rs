use std::sync::Arc;

use crate::router::{RouteTarget, SemanticRouteStore};
use anyhow::{anyhow, ensure, Result};
use arrow_array::{
    types::Float32Type, FixedSizeListArray, Float32Array, Float64Array, RecordBatch, StringArray,
};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::table::AddDataMode;
use lancedb::{connect, DistanceType, Table};

#[derive(Clone, Debug, PartialEq)]
pub struct RouteExampleRow {
    pub text: String,
    pub target: RouteTarget,
    pub vector: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouteMatch {
    pub text: String,
    pub target: RouteTarget,
    pub score: f32,
}

#[derive(Clone)]
pub struct LanceDbState {
    table: Table,
    table_name: String,
    vector_dim: i32,
}

impl LanceDbState {
    pub async fn new(db_path: &str, table_name: &str, vector_dim: i32) -> Result<Self> {
        ensure!(vector_dim > 0, "vector_dim must be greater than zero");

        let connection = connect(db_path).execute().await?;
        let table = match connection.open_table(table_name).execute().await {
            Ok(table) => table,
            Err(_) => {
                connection
                    .create_empty_table(table_name, route_schema(vector_dim))
                    .execute()
                    .await?
            }
        };

        Ok(Self {
            table,
            table_name: table_name.to_string(),
            vector_dim,
        })
    }

    pub async fn rebuild_from_examples(&mut self, examples: &[RouteExampleRow]) -> Result<()> {
        validate_examples(examples, self.vector_dim)?;

        let batch = route_batch(examples, self.vector_dim)?;

        self.table
            .add(batch)
            .mode(AddDataMode::Overwrite)
            .execute()
            .await?;

        Ok(())
    }

    pub async fn query_nearest(&self, embedding: &[f32], top_k: usize) -> Result<Vec<RouteMatch>> {
        if top_k == 0 {
            return Ok(Vec::new());
        }

        ensure!(
            embedding.len() == self.vector_dim as usize,
            "embedding dimension {} does not match configured dimension {} for table {}",
            embedding.len(),
            self.vector_dim,
            self.table_name
        );

        if self.table.count_rows(None).await? == 0 {
            return Ok(Vec::new());
        }

        let batches = self
            .table
            .query()
            .limit(top_k)
            .nearest_to(embedding)?
            .distance_type(DistanceType::Cosine)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut matches = Vec::new();
        for batch in batches {
            let texts = batch
                .column_by_name("text")
                .ok_or_else(|| anyhow!("text column missing from LanceDB query results"))?
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| anyhow!("text column has unexpected type"))?;
            let targets = batch
                .column_by_name("target")
                .ok_or_else(|| anyhow!("target column missing from LanceDB query results"))?
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| anyhow!("target column has unexpected type"))?;
            let distance_column = batch
                .column_by_name("_distance")
                .ok_or_else(|| anyhow!("_distance column missing from LanceDB query results"))?;

            for row in 0..batch.num_rows() {
                matches.push(RouteMatch {
                    text: texts.value(row).to_string(),
                    target: parse_target(targets.value(row))?,
                    score: 1.0 - distance_at(distance_column.as_ref(), row)?,
                });
            }
        }

        Ok(matches)
    }
}

#[async_trait]
impl SemanticRouteStore for LanceDbState {
    async fn query(&self, embedding: &[f32], limit: usize) -> Result<Vec<RouteMatch>> {
        self.query_nearest(embedding, limit).await
    }
}

fn route_schema(vector_dim: i32) -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("text", DataType::Utf8, false),
        Field::new("target", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                vector_dim,
            ),
            true,
        ),
    ]))
}

fn route_batch(examples: &[RouteExampleRow], vector_dim: i32) -> Result<RecordBatch> {
    let schema = route_schema(vector_dim);
    let texts = StringArray::from_iter_values(examples.iter().map(|row| row.text.as_str()));
    let targets =
        StringArray::from_iter_values(examples.iter().map(|row| target_label(&row.target)));
    let vectors = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        examples.iter().map(|row| {
            Some(
                row.vector
                    .iter()
                    .copied()
                    .map(Some)
                    .collect::<Vec<Option<f32>>>(),
            )
        }),
        vector_dim,
    );

    Ok(RecordBatch::try_new(
        schema,
        vec![Arc::new(texts), Arc::new(targets), Arc::new(vectors)],
    )?)
}

fn validate_examples(examples: &[RouteExampleRow], vector_dim: i32) -> Result<()> {
    for row in examples {
        ensure!(
            row.vector.len() == vector_dim as usize,
            "example {:?} has vector dimension {}, expected {}",
            row.text,
            row.vector.len(),
            vector_dim
        );
    }
    Ok(())
}

fn target_label(target: &RouteTarget) -> &'static str {
    match target {
        RouteTarget::Cloud => "cloud",
        RouteTarget::Local => "local",
    }
}

fn parse_target(label: &str) -> Result<RouteTarget> {
    match label {
        "cloud" => Ok(RouteTarget::Cloud),
        "local" => Ok(RouteTarget::Local),
        _ => Err(anyhow!("unexpected route target label {}", label)),
    }
}

fn distance_at(array: &dyn arrow_array::Array, row: usize) -> Result<f32> {
    if let Some(distances) = array.as_any().downcast_ref::<Float32Array>() {
        return Ok(distances.value(row));
    }
    if let Some(distances) = array.as_any().downcast_ref::<Float64Array>() {
        return Ok(distances.value(row) as f32);
    }
    Err(anyhow!("_distance column has unsupported type"))
}

#[cfg(test)]
#[path = "lancedb_tests.rs"]
mod tests;
