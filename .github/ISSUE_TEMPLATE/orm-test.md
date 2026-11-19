---
name: ORM / SQL compatibility test
about: Report running the admin over another ORM or SQL engine
title: "[orm-test] <ORM> on <database>"
labels: orm-test
---

Read [docs/agnostic-test.md](https://github.com/TincoNomad/actix-crud-admin/blob/main/docs/agnostic-test.md) first. It has the
scenarios, the rules and the reference adapters.

Paste your result:

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

If something failed, also paste the error verbatim. Do not patch the crate before
reporting: the blocker is the finding.
