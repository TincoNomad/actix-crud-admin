# Admin para una tabla existente

`GenericTable` convierte una tabla que **ya existe** en un panel de administración.
No necesitas structs, ni `list_columns()`, ni `form_fields()`, ni cinco métodos de CRUD:
el crate lee el esquema de la base de datos.

Es el equivalente a `admin.site.register(Product)` de Django, pero leyendo la tabla
en vez de un modelo.

## Qué detecta

- Nombres y tipos de columna (`TEXT`, `INTEGER`, `REAL`, `BOOLEAN`, `DATE`, ...).
- La primary key (no aparece en el formulario: la genera la base).
- Las claves foráneas, y con eso arma el `<select>` leyendo la tabla destino.
- Columnas `NOT NULL` como campos obligatorios.
- Columnas binarias (`BLOB`): se muestran en el listado, pero no en el formulario.

## SQLite, en 3 pasos

```toml
[dependencies]
actix-crud-admin = { version = "0.2.0", features = ["sqlite"] }
```

```rust
use std::sync::Arc;
use actix_crud_admin::{AdminRegistry, AdminResource, GenericTable, SchemaSource};
use actix_crud_admin::sqlite::SqliteSource;

let source: Arc<dyn SchemaSource> = Arc::new(SqliteSource::connect("sqlite:app.db").await?);

let mut registry = AdminRegistry::new();
registry.register(GenericTable::new(source.clone(), "products").await?);
registry.register(GenericTable::new(source, "categories").await?);
```

Eso es todo. `products` y `categories` aparecen en el dashboard con listado,
búsqueda, paginación, orden, alta, edición y borrado.

Si ya tienes un `SqlitePool`, reúsalo:

```rust
let source = SqliteSource::from_pool(pool.clone());
```

## Nombres e iconos

```rust
registry.register(
    GenericTable::new(source, "products")
        .await?
        .title("Product", "Products")
        .icon("📦"),
);
```

## Seguridad

`GenericTable` **solo** expone las tablas que registras, una por una. No hay
un "registrar todo". Eso es a propósito: si expones la tabla de usuarios, expones
los hashes de contraseñas.

Regla práctica: registra tablas de negocio, nunca tablas de autenticación,
sesiones, logs ni migraciones.

## Límites conocidos

- **No crea tablas.** Las migraciones siguen siendo tuyas (Diesel, SQLx, SQL manual).
  Si el cliente ya tiene la base de datos, esto es justo lo que quieres.
- **El esquema se lee al arrancar.** Una columna nueva aparece después de reiniciar.
- **Sin validación de negocio.** Solo se valida `NOT NULL`. Reglas como "precio > 0"
  necesitan un recurso escrito a mano.
- **Filtros solo por clave foránea.** Las columnas de texto y los booleanos
  todavía no se ofrecen como filtro. El resto (buscar, ordenar, paginar,
  exportar CSV y borrar en lote) sí está.
- **Claves foráneas:** el select trae hasta 500 filas. Para catálogos grandes
  hace falta un buscador de FK.
- **Primary keys de texto:** el alta funciona, pero el "guardar y continuar"
  no puede recuperar el id generado, así que te devuelve al listado.

## Otros motores y ORMs

El contrato es el trait `SchemaSource`. Implementarlo una vez por proyecto te da
el admin para **cualquier** ORM, porque solo necesita ejecutar SQL:

```rust
#[async_trait]
impl SchemaSource for MyDieselSource {
    async fn describe(&self, table: &str) -> Result<TableSchema, AdminError> {
        // SELECT column_name, data_type, is_nullable, ... FROM information_schema.columns
        // WHERE table_name = ...  (o PRAGMA table_info para SQLite)
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

Con Diesel, `diesel::sql_query(...)` y `QueryableByName` bastan para cada método.
El crate trae implementaciones para **SQLite** (`features = ["sqlite"]`) y
**PostgreSQL** (`features = ["postgres"]`); MySQL todavía no.

En Postgres se asume el esquema `public` y las claves primarias de una sola
columna. Con Supabase usa el session pooler (puerto 5432) y `?sslmode=require`.

## Cuándo NO usar esto

- Si necesitas lógica de negocio en el formulario → implementa `AdminResource` a mano
  ([Formularios y base de datos](forms.md)).
- Si la tabla tiene 50 columnas y el admin debe mostrar 6 → `GenericTable` las muestra
  todas.
- Si necesitas permisos por usuario o auditoría → todavía no existe.
