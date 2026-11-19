# Diesel + PostgreSQL Example

This example shows how to integrate `actix-crud-admin` with a Diesel/PostgreSQL backend, following the `src/admin/` + `src/models/` convention.

> This is a nested package, so `cargo test` at the repository root does not build
> it. Run `cargo check` inside this folder after changing the public API.

## Requirements

- PostgreSQL server running locally.
- `DATABASE_URL` environment variable.

## Setup

1. Copy the environment file:

```bash
cp .env.example .env
# Edit .env with your PostgreSQL credentials
```

2. Create the database:

```bash
createdb actix_admin_example
```

3. Run the example:

```bash
cargo run
```

The application will run pending Diesel migrations on startup and create an initial admin user `admin` / `admin`.

4. Open `http://127.0.0.1:8080/admin` and log in.

## Project layout

```text
src/
├── main.rs          # Server setup, migrations, AdminSite wiring
├── db.rs            # Diesel connection pool
├── schema.rs        # Diesel table definitions
├── models/
│   ├── mod.rs
│   └── product.rs   # Product model (Queryable, Insertable)
└── admin/
    ├── mod.rs       # Builds AdminRegistry
    └── product.rs   # ProductAdmin AdminResource implementation
```

## Admin CLI

This example uses `JsonUserStore` for authentication. To manage users from the command line:

```bash
# From the actix-crud-admin crate directory
cargo run --bin admin-cli -- createsuperuser --file /path/to/example/users.json
```

For a production setup, replace `JsonUserStore` with your own `UserStore` implementation backed by the same PostgreSQL database.
