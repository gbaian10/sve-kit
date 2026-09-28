# Build DB core

This package compiles code-authored declarations into SQLite DDL and owns the
SQLite boundary. It contains no production table declarations, importers or CLI
integration. T0 declarations are supplied separately.

```python
from sve_carddb.build_db import (
    Capability,
    Column,
    Kind,
    Registry,
    Table,
    compile_schema,
    create_database,
)

registry = Registry(
    tables=(Table("example", (Column("id", Kind.ID),), ("id",)),),
    capabilities=(Capability("core", ("example",)),),
)
schema = compile_schema(registry, ("core",))
with create_database(schema) as db:
    with db.transaction():
        db.insert("example", {"id": "synthetic:a"})
    assert db.rows("example")[0].values["id"] == "synthetic:a"
```

- `Registry.resolve` closes explicit capability dependencies and required FK
  targets, including cycles. Unknown and unimplemented capabilities fail. Each
  table has one owner; unimplemented capabilities may reserve future table names.
- Disabled nullable FKs retain their columns, omit the unavailable FK and add
  `IS NULL` checks to all nullable columns in that key. Required columns in a
  compound key remain usable. Enabling a capability compiles the real FK; it
  does not migrate an existing database. Target columns/keys can only be checked
  once the future table declaration is available.
- `Column.fixed` declares an ordinary non-null TEXT column with DEFAULT and CHECK,
  suitable for the fixed kind component of a vocabulary FK. Define the FK itself
  with `ForeignKey`. `Unique.where` declares a partial unique index. Partial
  indexes cannot serve as FK target keys.
- `Check.sql` and `Unique.where` are trusted SQL written by the schema author.
  They must never contain imported values or user input. Identifiers use a strict
  whitelist, declaration constants are quoted, and row values are bound parameters.
- `Kind.INT` and `Kind.UINT` use safe-integer bounds; `Kind.UINT32` permits
  0 through 4294967295. Domain rules such as reserved allocation ranges remain
  explicit table checks or importer validation. TEXT may be empty; ID may not.
- JSON columns require a named JSON Schema passed to `compile_schema`. Validators
  use Draft 2020-12 with local references, explicit constraints and no optional
  format checker. Schema IDs, external references and format assertions are
  rejected. JSON uses the existing canonical-json-v1 boundary: duplicate keys,
  floats, unsafe integers and invalid Unicode fail. A `Json(value)` wrapper
  distinguishes JSON null (`Json(None)`) from SQL NULL (`None`).
- Tables use `STRICT, WITHOUT ROWID`, explicit NOT NULL keys and deferred FKs with
  ON UPDATE/DELETE RESTRICT. SQLite still performs lossless affinity conversions;
  the typed API additionally rejects coerced inputs such as `"1"` for an integer
  or `1` for a Bool. Reads validate storage classes and reconstruct Bool/Json.
- `create_database(schema, path)` requires a new file; omission selects an isolated
  in-memory DB. It never replaces an existing file. This API creates databases;
  opening existing builds and migrations are outside its scope. Failed schema
  installation rolls back DDL but can leave the newly created empty file.
- All writes require `transaction()`. The entire transaction is rolled back on an
  unhandled body, verification or commit failure. Nested transactions are rejected.
  Each commit runs foreign_key_check and integrity_check with enforcement enabled;
  batch related rows in one transaction. RESTRICT deletes/updates remain immediate,
  while insert-time FK resolution is deferred. Reads return checked `Row` objects,
  never sqlite cursors or raw `Any`.

`CompiledSchema.sql` is deterministic for fixed declarations, independent of
registry/request ordering. A raw scratch connection must install the deterministic
`sve_json_valid` and `sve_fullmatch` functions using `database.install_functions`,
and enable `PRAGMA foreign_keys=ON`, before executing its DDL. The normal factory
does both. No compiled DDL or build databases are checked in.

See SQLite's [foreign key rules](https://www.sqlite.org/foreignkeys.html) and
[STRICT tables](https://www.sqlite.org/stricttables.html) for native constraint
semantics. Project table requirements remain in `docs/schema/build-db.md` and
`docs/schema/implementation-tiers.md`.
