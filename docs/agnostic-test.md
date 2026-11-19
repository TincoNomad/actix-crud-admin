# Testing the ORM and SQL claims

`actix-crud-admin` ships two backends, both built on **SQLx**: SQLite and PostgreSQL.
The `SchemaSource` trait is supposed to be independent of that choice, but it
has never been implemented over another ORM. This is the test that settles it.

You do not need to be a maintainer, and you do not need to send a perfect result.
A report that says "Diesel cannot do this, here is the error" is as useful as a
passing one.

## What the claim means

There are two axes, and both are open:

| Axis | Question | Known answers |
|---|---|---|
| ORM | Can you implement `SchemaSource` over Diesel / SeaORM without changing the trait? | only SQLx |
| Database | Does the approach survive another SQL dialect? | SQLite, PostgreSQL |

A useful report names the pair: `Diesel on PostgreSQL`, `SeaORM on MySQL`. The
most valuable missing cells are any **non-SQLx** adapter and any **MySQL** adapter.

## What you need

- A database you can create tables in (Postgres, MySQL, or SQLite).
- Your ORM of choice, in a scratch crate.
- `actix-crud-admin` from the repository, with no database feature enabled:
  ```toml
  actix-crud-admin = { path = "../actix-crud-admin" }
  ```
- 2 to 4 hours.

## What to do

1. Write a struct that stores your pool and implements `SchemaSource`.
2. Fill the seven methods: `describe`, `rows`, `row`, `insert`, `update`, `delete`,
   `options`.
3. Build a `GenericTable` over one of your tables and render the list and the form.
4. Walk the scenarios below and note where you had to work around the trait.

The reference implementations to read are `src/sqlite.rs` and `src/postgres.rs`.
They are deliberately plain: no helper library, no clever abstractions.

## Rules

- **Do not modify the crate.** If the trait has to change, that is the finding.
  Report it first; the patch comes after, if it comes.
- **Do not use SQLx inside the adapter.** That would defeat the experiment.
- **Report before patching.** A half-finished adapter plus an honest blocker is
  worth more than a working adapter built on a silent workaround.
- **Time-box it.** After 4 hours, write what you have, including where you stopped.

## Scenarios

| # | Table | What it tests |
|---|---|---|
| 1 | `id` integer PK, two text columns | the baseline |
| 2 | a foreign key to another table | the select must show the label, not the id |
| 3 | 30 columns | whether `describe` scales |
| 4 | a text primary key | insert without a generated id |
| 5 | a self-referencing foreign key | the label lookup does not recurse |
| 6 | nullable columns with values and nulls | empty values round-trip |
| 7 | a database view | read-only tables |
| 8 | a composite primary key | expected to fail; document how |

Scenario 8 is not a trick. Composite keys are not supported by the adapters
shipped today, and knowing how another backend behaves there is useful.

## Report

Paste this in the pull request, the issue, or wherever you want to send it:

```
- orm:            Diesel 2.x / SeaORM 1.x / other
- db:             Postgres 16 / MySQL 8 / SQLite 3 / other
- adapter uses:   only that ORM   (say so if SQLx leaked in)
- compiles:       yes / no
- lines:          ~120
- blocked at:     describe / rows / insert / dynamic columns / nothing
- trait change:   none / which method and why
- time:           2h
- link:           gist or fork
```

The `adapter uses` line matters: an adapter that says "Diesel" but borrows SQLx
proves nothing.

## What happens with your result

- **Works with no trait change** → the ORM claim becomes a fact, and your report
  is linked from the README table.
- **Needs a trait change** → the trait was coupled to SQLx. Your case drives the
  redesign, and you get credited for it.
- **Impossible with that ORM** → the README stops claiming "any ORM" and starts
  saying "any driver with runtime column access". Less catchy, more true.

## What is explicitly out of scope

- MySQL support inside the crate. A report showing it works is welcome; shipping
  the adapter is a separate decision.
- Nested transactions, connection pooling strategy, or performance work.
- Anything that requires changing the public API beyond the trait.

*Results are published in the README as soon as there is one. If you prefer to
stay anonymous, say so and the row goes in without a name.*
