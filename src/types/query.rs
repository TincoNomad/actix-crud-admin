use serde::Deserialize;

/// Query parameters for the list view.
///
/// `filters` is filled by the handler from `?filter_<column>=<value>` params,
/// not deserialized by serde_urlencoded.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
    pub search: Option<String>,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    #[serde(skip)]
    pub filters: std::collections::HashMap<String, String>,
}

/// Results of a list operation.
pub struct ListResult {
    pub rows: Vec<serde_json::Value>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}

/// A column offered as a filter in the list view.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FilterSpec {
    pub key: String,
    pub label: String,
    pub options: Vec<(String, String)>,
}
