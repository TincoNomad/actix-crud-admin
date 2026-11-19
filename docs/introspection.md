# Admin for an existing table

`GenericTable` turns a table that **already exists** into an admin panel. No structs,
no `list_columns()`, no `form_fields()`, no five CRUD methods: the crate reads the
schema from the database.

It is the equivalent of Django's `admin.site.register(Product)`, but reading the
table instead of a model.

## What it detects

- Column names and types (`TEXT`, `INTEGER`, `REAL`, `BOOLEAN`, `DATE`, `uuid`, ...).
- The primary key (hidden from the form: the database generates it).
- Foreign keys, which become a `<select>` populated from the target table.
- `NOT NULL` columns as required fields.
- Binary columns (`BLOB`, `bytea`): shown in the list, hidden from the form.
- `DEFAULT` values: the field is optional and left out of the insert, so the
  database applies its default.

## SQLite, in 3 steps

```toml
[dependencies]
actix-crud-admin = { version = "0.3.0", features = ["sqlite"] }
```

```rust
use std::sync::Arc;
use actix_crud_admin::{AdminRegistry, AdminResource, GenericTable, SchemaSource};
use actix_crud_admin::sqlite::SqliteSource;

let source: Arc<dyn SchemaSource> = Arc::new(SqliteSource::connect("app.db").await?);

let mut registry = AdminRegistry::new();
registry.register(GenericTable::new(source.clone(), "products").await?);
registry.register(GenericTable::new(source, "categories").await?);
```

That is it. `products` and `categories` show up in the dashboard with list, search,
pagination, sorting, create, edit and delete.

If you already own a `SqlitePool`, reuse it:

```rust
let source = SqliteSource::from_pool(pool.clone());
```

## PostgreSQL, in 3 steps

```toml
[dependencies]
actix-crud-admin = { version = "0.3.0", features = ["postgres"] }
```

```rust
use actix_crud_admin::postgres::PostgresSource;

let source: Arc<dyn SchemaSource> =
    Arc::new(PostgresSource::connect(&std::env::var("DATABASE_URL")?).await?);
```

With Supabase use the **session pooler** (port 5432, not the transaction pooler on
6543) and append `?sslmode=require` to the URL. The crate reads the `public` schema.

The adapter keeps no prepared statement cache, so it is also safe behind a
transaction pooler.

## Names and icons

```rust
registry.register(
    GenericTable::new(source, "products")
        .await?
        .title("Product", "Products")
        .icon("📦"),
);
```

## Permissions and writes

Readers can browse, search, filter and export. Only a superuser can create, update
or delete (see the `UserStore` trait: `is_superuser` is stored in the session).

## Security

`GenericTable` exposes **only** the tables you register, one by one. There is no
"register everything" switch. That is intentional: exposing the users table exposes
the password hashes.

Rule of thumb: register business tables, never authentication, session, log or
migration tables.

## Known limits

- **No filters on text or boolean columns yet.** Foreign keys can be filtered; search,
  sort, pagination, CSV export and bulk delete all work.
- **It does not create tables.** Migrations are still yours (Diesel, SQLx, plain SQL).
  If the client already has the database, this is exactly what you want.
- **The schema is read on startup** and cached. A new column shows up after a restart
  or after calling `clear_cache()`.
- **No business validation.** Only `NOT NULL` is validated. Rules like "price > 0"
  need a hand-written resource.
- **Foreign keys:** the select loads up to 500 rows. Larger catalogs need a lookup
  screen.
- **Text primary keys:** insert works, but "save and continue" cannot recover the
  generated id and sends you back to the list.
- **Single-column primary keys.** Composite keys are not supported.
- **The CSV is a report, not an import.** Foreign keys export as labels and booleans
  as `true`/`false`; if you need to re-import, use the ids.
- **Timestamps** render as date inputs, so editing one loses the time part. Columns
  with a default are safe because they are not submitted when empty.

## Other engines and ORMs

The contract is the `SchemaSource` trait. Implement it once per project and you get
the admin for **any** ORM, because it only needs to run SQL:

```rust
#[async_trait]
impl SchemaSource for MyDieselSource {
    async fn describe(&self, table: &str) -> Result<TableSchema, AdminError> {
        // SELECT column_name, data_type, is_nullable, ... FROM information_schema.columns
        // WHERE table_name = ...  (or PRAGMA table_info for SQLite)
    }
    async fn rows(&self, table: &str, query: ListQuery) -> Result<ListResult, AdminError> { .. }
    async fn row(&self, table: &str, id: &str) -> Result<serde_json::Value, AdminError> { .. }
    async fn insert(&self, table: &str, data: RowMap) -> Result<serde_json::Value, AdminError> { .. }
    async fn update(&self, table: &str, id: &str, data: RowMap) -> Result<serde_json::Value, AdminError> { .. }
    async fn delete(&self, table: &str, id: &str) -> Result<(), AdminError> { .. }
    async fn options(&self, table: &str, value_column: &str)
        -> Result<Vec<(String, String)>, AdminError> { .. }
}
```

With Diesel, `diesel::sql_query(...)` and `QueryableByName` are enough for each method.
The crate ships **SQLite** and **PostgreSQL** implementations; MySQL does not exist
yet. Both adapters cache the schema, so a list page costs one round trip per
foreign key column, not one per record.

## When not to use this

- If the form needs business logic, implement `AdminResource` by hand
  ([Forms and database integration](forms.md)).
- If the table has 50 columns and the admin must show 6, `GenericTable` shows all of
  them.
- If you need per-user permissions or legal-grade audit, that does not exist yet.
