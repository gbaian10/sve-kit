# Build DB core

This package compiles code-authored declarations into SQLite DDL and owns the
SQLite boundary. `t0.compile_t0()` supplies the forty production T0 tables from
`docs/schema/build-db.md`. There are no importers or CLI integration.

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
- Tables use `STRICT`, explicit NOT NULL keys and deferred FKs with
  ON UPDATE/DELETE RESTRICT. SQLite still performs lossless affinity conversions;
  the typed API additionally rejects coerced inputs such as `"1"` for an integer
  or `1` for a Bool. Reads validate storage classes and reconstruct Bool/Json.
  `Table.without_rowid` defaults to true. T0 `text_unit` uses a rowid table because
  synthetic long-text measurements favored it; its public primary key is still
  the explicit text ID. Reproduce the comparison with
  `uv --directory carddb run python -m tests.benchmark_build_db` (10,000 rows,
  three repetitions, four text lengths, including commit-time verification).
- `create_database(schema, path)` requires a new file; omission selects an isolated
  in-memory DB. It never replaces an existing file. This API creates databases;
  opening existing builds and migrations are outside its scope. Failed schema
  installation rolls back DDL but can leave the newly created empty file.
- All writes require `transaction()`. The entire transaction is rolled back on an
  unhandled body, verification or commit failure. Nested transactions are rejected.
  Each commit runs foreign_key_check and integrity_check with enforcement enabled;
  batch related rows in one transaction. Importers should use a few large
  transactions: full verification scans the entire database on each commit.
  There is no unchecked batch/commit mode, and the final transaction must pass
  both checks. RESTRICT deletes/updates remain immediate,
  while insert-time FK resolution is deferred. Reads return checked `Row` objects,
  never sqlite cursors or raw `Any`.

`CompiledSchema.sql` is deterministic for fixed declarations, independent of
registry/request ordering. A raw scratch connection must install the deterministic
`sve_json_valid`, `sve_fullmatch`, `sve_sorted_unique` and `sve_symbol_valid`
functions using `database.install_functions`,
and enable `PRAGMA foreign_keys=ON`, before executing its DDL. The normal factory
does both. No compiled DDL or build databases are checked in.

T0 keeps five future nullable references (art, CR version, DSL document/load and
keyword) as NULL-only columns. The registry reserves their targets without
creating empty tables. T0 support rows require `missing_dsl`, null candidate/DSL/
engine fields, `not_applicable`, nonempty sorted reasons and `automatic=false`.
The nonempty reasons check implements [snapshot-format §7](../../../../docs/schema/snapshot-format.md#7-發布閘門與變動報告),
which requires reasons for every non-passed support state. These are derived
support rows for snapshot construction (build-db §10), not raw candidate input.
The fixture's `missing_dsl` reason code is an example, not a required value or
an exhaustive reason-code vocabulary.
When implementing the engine capability, replace those T0-specific checks with
the full support-state and evidence constraints; simply registering DSL tables
does not lift the T0 engine restriction.

The six JSON columns have named schemas. `ParameterSchema` reuses the public
definition; spellings and localizations are objects in the build DB, not wire
tuples. SQL also checks ordered unique parameter names, bounds and spelling
references to enabled domains. Localization strings remain literal data: this
module does not invent or execute a placeholder language.

`text_unit.lang` and `search_alias.lang` reference `language.code`, as explicitly
specified in build-db §2, §4 and §15. A language tag must be registered before
commit so that text and aliases have corresponding language configuration.
Registering an additional language permits it; the FK does not restrict the
registry to the three initial languages. This does not automatically turn other
Lang-typed fields or JSON members into SQL foreign keys.

DDL enforces row-local checks and FKs expressible with the declared columns.
Import/validation stages must still verify cross-row policies: printing/product
and observation/revision region equality, face counts, restriction/profile name
regions, nonoverlapping intervals, exact content hashes, immutable history,
allocation ranges, decision membership/evidence, language and symbol JSON
reference closure, route derivation/alias collisions, and polymorphic search
aliases. For example, observation has no logical region column: its composite
FK guarantees the revision's face, while its region must be compared with the
printing by the validator. Passing this schema is not a release acceptance gate.

See SQLite's [foreign key rules](https://www.sqlite.org/foreignkeys.html) and
[STRICT tables](https://www.sqlite.org/stricttables.html) for native constraint
semantics. Project table requirements remain in `docs/schema/build-db.md` and
`docs/schema/implementation-tiers.md`.
