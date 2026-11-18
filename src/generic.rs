//! Admin resource built from an existing table instead of hand-written CRUD.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::resource::AdminResource;
use crate::schema::{ColumnKind, ColumnSchema, ForeignKey, RowMap, SchemaSource, TableSchema};
use crate::types::{AdminError, Column, FilterSpec, FormField, ListQuery, ListResult};

/// An [`AdminResource`] driven entirely by the table schema.
///
/// ```no_run
/// # use std::sync::Arc;
/// # use actix_crud_admin::generic::GenericTable;
/// # use actix_crud_admin::schema::SchemaSource;
/// # async fn run(source: Arc<dyn SchemaSource>, registry: &mut actix_crud_admin::AdminRegistry) {
/// let products = GenericTable::new(source, "products").await.unwrap();
/// registry.register(products);
/// # }
/// ```
pub struct GenericTable {
    source: Arc<dyn SchemaSource>,
    table: String,
    slug: String,
    name: String,
    plural_name: String,
    icon: String,
    schema: TableSchema,
}

impl GenericTable {
    /// Loads the table schema. Changes to the table need a restart.
    pub async fn new(source: Arc<dyn SchemaSource>, table: &str) -> Result<Self, AdminError> {
        let schema = source.describe(table).await?;
        let label = humanize(table);
        Ok(Self {
            source,
            table: table.to_string(),
            slug: table.replace('_', "-"),
            name: label.clone(),
            plural_name: label,
            icon: "🗂".to_string(),
            schema,
        })
    }

    /// Override the singular and plural display names.
    pub fn title(mut self, name: &str, plural_name: &str) -> Self {
        self.name = name.to_string();
        self.plural_name = plural_name.to_string();
        self
    }

    /// Override the dashboard icon.
    pub fn icon(mut self, icon: &str) -> Self {
        self.icon = icon.to_string();
        self
    }

    fn editable_columns(&self) -> impl Iterator<Item = &ColumnSchema> {
        self.schema
            .columns
            .iter()
            .filter(|c| !c.primary_key && c.kind != ColumnKind::Binary)
    }

    fn build_fields(&self, options: &HashMap<String, Vec<(String, String)>>) -> Vec<FormField> {
        self.editable_columns()
            .map(|col| {
                let mut field = match &col.foreign_key {
                    Some(_) => FormField::select(
                        &col.name,
                        &humanize(&col.name),
                        options
                            .get(&col.name)
                            .map(|opts| {
                                opts.iter().map(|(v, l)| (v.as_str(), l.as_str())).collect()
                            })
                            .unwrap_or_default(),
                    ),
                    None => match col.kind {
                        ColumnKind::Boolean => FormField::boolean(&col.name, &humanize(&col.name)),
                        ColumnKind::Number => FormField::number(&col.name, &humanize(&col.name)),
                        ColumnKind::Date => FormField::date(&col.name, &humanize(&col.name)),
                        _ => FormField::text(&col.name, &humanize(&col.name)),
                    },
                };
                if !col.nullable && !col.has_default {
                    field = field.required();
                }
                field
            })
            .collect()
    }

    /// Coerce submitted form strings into database-friendly values.
    fn normalize(&self, data: HashMap<String, Value>) -> RowMap {
        let mut out = RowMap::new();
        for col in self.editable_columns() {
            if col.kind == ColumnKind::Boolean {
                let on = data.get(&col.name).map(is_truthy).unwrap_or(false);
                out.insert(col.name.clone(), Value::Bool(on));
                continue;
            }
            let raw = data.get(&col.name).map(as_text).unwrap_or_default();
            if raw.is_empty() {
                if col.has_default {
                    // Let the database apply its default.
                } else if col.nullable {
                    out.insert(col.name.clone(), Value::Null);
                }
                // Empty required values are reported by validate(); skip them here.
                continue;
            }
            out.insert(col.name.clone(), Value::String(raw));
        }
        out
    }
}

#[async_trait]
impl AdminResource for GenericTable {
    fn name(&self) -> &str {
        &self.name
    }

    fn plural_name(&self) -> &str {
        &self.plural_name
    }

    fn slug(&self) -> &str {
        &self.slug
    }

    fn icon(&self) -> &str {
        &self.icon
    }

    fn list_columns(&self) -> Vec<Column> {
        self.schema
            .columns
            .iter()
            .map(|c| {
                let label = column_label(c);
                match c.kind {
                    ColumnKind::Boolean => Column::boolean(&c.name, &label),
                    ColumnKind::Number => Column::number(&c.name, &label),
                    ColumnKind::Date => Column::date(&c.name, &label),
                    _ => Column::text(&c.name, &label),
                }
            })
            .collect()
    }

    fn form_fields(&self) -> Vec<FormField> {
        self.build_fields(&HashMap::new())
    }

    async fn form_fields_dynamic(&self) -> Vec<FormField> {
        let mut options = HashMap::new();
        for col in self.editable_columns() {
            if let Some(ForeignKey { table, column }) = &col.foreign_key {
                if let Ok(opts) = self.source.options(table, column).await {
                    options.insert(col.name.clone(), opts);
                }
            }
        }
        self.build_fields(&options)
    }

    fn searchable_fields(&self) -> Vec<&str> {
        self.schema
            .columns
            .iter()
            .filter(|c| c.kind == ColumnKind::Text)
            .map(|c| c.name.as_str())
            .collect()
    }

    async fn validate(&self, data: &HashMap<String, Value>) -> Result<(), HashMap<String, String>> {
        let mut errors = HashMap::new();
        for col in self.editable_columns() {
            if col.nullable || col.has_default || col.kind == ColumnKind::Boolean {
                continue;
            }
            let empty = data
                .get(&col.name)
                .map(as_text)
                .unwrap_or_default()
                .is_empty();
            if empty {
                errors.insert(
                    col.name.clone(),
                    format!("{} is required.", humanize(&col.name)),
                );
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    async fn filters(&self) -> Vec<FilterSpec> {
        let mut specs = Vec::new();
        for col in self.editable_columns() {
            if let Some(fk) = &col.foreign_key {
                let options = self
                    .source
                    .options(&fk.table, &fk.column)
                    .await
                    .unwrap_or_default();
                if !options.is_empty() {
                    specs.push(FilterSpec {
                        key: col.name.clone(),
                        label: column_label(col),
                        options,
                    });
                }
            }
        }
        specs
    }

    async fn list(&self, query: ListQuery) -> Result<ListResult, AdminError> {
        self.source.rows(&self.table, query).await
    }

    async fn get(&self, id: &str) -> Result<Value, AdminError> {
        self.source.row(&self.table, id).await
    }

    async fn create(&self, data: HashMap<String, Value>) -> Result<Value, AdminError> {
        self.source.insert(&self.table, self.normalize(data)).await
    }

    async fn update(&self, id: &str, data: HashMap<String, Value>) -> Result<Value, AdminError> {
        self.source
            .update(&self.table, id, self.normalize(data))
            .await
    }

    async fn delete(&self, id: &str) -> Result<(), AdminError> {
        self.source.delete(&self.table, id).await
    }
}

/// `category_id` (a foreign key) -> `Category`; everything else is humanized.
fn column_label(column: &ColumnSchema) -> String {
    let name = if column.foreign_key.is_some() {
        column.name.strip_suffix("_id").unwrap_or(&column.name)
    } else {
        &column.name
    };
    humanize(name)
}

/// `product_categories` -> `Product categories`
pub(crate) fn humanize(name: &str) -> String {
    let spaced = name.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub(crate) fn as_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::String(s) => matches!(s.as_str(), "1" | "true" | "on" | "yes"),
        Value::Number(n) => n.as_i64().map(|i| i != 0).unwrap_or(true),
        _ => false,
    }
}
