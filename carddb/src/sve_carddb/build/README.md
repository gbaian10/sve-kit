# Build DB core

This package compiles code-authored declarations into SQLite DDL and owns the
SQLite boundary. The [product family staging importer](../domains/products/README.md)
can supply verified family parents and compose with identity staging in one
transaction. `tests.support.schema_helpers.compile_t0()` supplies the 41 production T0 tables from
`docs/schema/build/build-db.md`. The regional identity staging importer is described in
[registry/preview](../domains/registry/preview/README.md); there is no complete build CLI.

```python
from sve_carddb.build import (
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
  `uv --directory carddb run python -m tests.build.benchmark_build_db` (10,000 rows,
  three repetitions, four text lengths, including commit-time verification).
- `create_database(schema, path)` requires a new file; omission selects an isolated
  in-memory DB. It never replaces an existing file. This API creates databases;
  opening existing builds and migrations are outside its scope. Failed schema
  installation rolls back DDL but can leave the newly created empty file.
- All writes require `transaction()`. The entire transaction is rolled back on an
  unhandled body, verification or commit failure. Nested transactions are rejected.
  Each commit confirms FK and CHECK enforcement, then runs foreign_key_check and
  the selected cross-table checks; batch related rows in one transaction.
  Importers should use a few large transactions: full verification scans the
  entire database on each commit.
  There is no unchecked batch/commit mode, and the final transaction must pass
  these checks. RESTRICT deletes/updates remain immediate,
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
The nonempty reasons check implements [snapshot-format §7](../../../../docs/schema/export/snapshot-format.md#7-發布閘門與變動報告),
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
semantics. Project table requirements remain in `docs/schema/build/build-db.md` and
`docs/schema/build/implementation-tiers.md`.

## Optional T1 DDL and pipeline readiness

`t1.compile_build(("cr",))` compiles the 42 T0 tables plus the two CR
tables: 44 tables. The default selects only T0. Image metadata is supplied by
the separate image index, without image tables in this database.
DSL remains a reserved, unavailable capability; disabled nullable references remain
NULL-only. The optional `keyword` capability includes seven manual mechanism tables
and their glossary/translation dependencies. The offline workflow supplies the H1
producer with partial coverage; see `docs/schema/domains/keyword-mechanics.md`.
Enabling CR installs the existing
`rules_profile_revision.cr_version_id` FK. This selection is smaller than the
minimum 55-table DDL inventory.

`Capability.implemented` means that DDL declarations exist. The separate
`importer_ready` and `validator_ready` flags default to false.
These flags are code-maintained registration claims, not evidence generated from
successful SQL compilation. A release pipeline must additionally require the full
minimum table set and conditional groups. Creating empty tables does not enable a
preview capability or turn unknown coverage into complete coverage.

## Cross-table verification and transactions

`QueryCheck` is a trusted, code-authored SELECT that returns a row when an invariant
is violated. Its `tables` declaration lists every referenced table; these are
required dependencies in capability closure. The compiler attaches these checks
to `CompiledSchema.query_checks` in deterministic table order. Like `Check.sql`,
this SQL must never come from imported values. Use bounded existence queries,
not queries returning imported content.

`Database.verify()` runs FK/check-enforcement checks, `foreign_key_check`,
`integrity_check`, the build schema version check, then all selected query checks.
`transaction()` invokes it with `integrity=False` **after the body, before COMMIT**,
on the final graph. Explicit verification runs the full `integrity_check`.
Before persisting a bundle, `save()` checks enforcement and runs `integrity_check`
with CHECK constraints temporarily ignored: writes already enforce CHECK, while
the save check retains page, index, UNIQUE and NOT NULL validation. CHECK enforcement
is restored even if the check fails, and is verified again before backup.
An insert, update or delete can temporarily violate a query check inside the
transaction, allowing related rows to be repaired together. Updating a parent is checked even when no child row changed.
A failing query raises `sqlite3.IntegrityError` naming the check, without printing
source rows. Any unhandled body, verification or commit failure rolls back the
**whole transaction**, including parent updates and child writes. A caught
intermediate error cannot bypass the final verification. Nested transactions
remain unsupported. Direct `verify()` outside `transaction()` only checks and
raises; it does not itself roll back an externally owned transaction.

These checks execute through the typed SQLite boundary. They are **not triggers**
and are not contained in `CompiledSchema.sql`; external/raw SQL writers must not
treat executing DDL alone as validated build creation. Use the Database API and
run complete domain validation before publication.

## Schema version and rebuilds

Compiled schemas carry a positive signed 32-bit `version`, stored in SQLite's
`PRAGMA user_version` during creation and checked before every commit. Legacy
The test helper `compile_t0()` uses version 1; the optional T1 registry uses version 10, including
its T0-only selection. Version 9 removes the obsolete image tables; image
metadata and size configuration belong to the image index and public contract.
Version 10 adds `face_wording` for wording classifications and shared bases.
The
version identifies the declaration generation; the selected capability closure
determines the actual table set. It is independent
of crawl-manifest schema versions and public snapshot format/data versions.

`rebuild_database(schema, destination, populate)` creates a private sibling
candidate file and calls `populate(database)` inside one transaction. The callback
must import from pinned inputs using typed writes; it must not start another
transaction. It does not copy old DB rows, infer missing data or disable FKs.
All rows are therefore checked against newly enabled constraints. After a
successful commit and connection close, the candidate atomically replaces the
destination. Creation, population, verification or replacement failures clean up
the candidate and leave an existing destination unchanged. No partially built
candidate is handed to readers. A missing destination is also supported.

The destination must be an exclusively owned, offline build artifact with all
connections closed; callers provide that ownership. Symlink destinations and
existing SQLite WAL/SHM/journal sidecars are rejected. This helper is not a
concurrency lock, a crash-durable release transaction or an in-place migration.
It cannot protect against unrelated processes opening/changing the destination
concurrently. Source archives and frozen inputs are never replacement targets.

## Minimum T1 and optional EN groups

`tests.support.schema_helpers.compile_minimum()` selects T0 plus CR, errata, correction, QA and
related: exactly 56 tables. `compile_minimum(include_en=True)` adds `art`, `artist`,
`art_artist` and the three region review/divergence tables: exactly 62. The `en`
group explicitly requires `art`; selecting `art` alone is also supported. The `art` group also
carries `artist` and `art_artist`; no baseline or DSL placeholder tables are
created. All importer/validator readiness flags remain false. These counts attest
DDL coverage, not release readiness.

Errata and correction before/after values reuse the public `CorrectionValue` and
`ErrataChange` schemas, including their field-dependent types, through local
schema references and a CHECK over the row's field and JSON values. Numeric JSON
null uses `Json(None)`; the columns themselves are non-nullable. The build value
domain does not widen the authored correction input's effect/card_type whitelist.

New query checks validate errata face/region and applicability adoption,
supersedes ownership, correction adoption/application output requirements and
revision scope, reskin confirmation/reverse duplicates, confirmed-none evidence
adoption and aligned/divergent review adoption. Foreign keys bind correction
printing/face, related target printing/card, and art card/face ownership.
Disabled art references are NULL-only; enabling art installs the existing
printing_face composite FK. Switching from an earlier build to version 4 uses
the same rebuild helper and revalidates all imported references before replacement.

The schema can retain pending reviews, unadopted needs_review corrections,
conflict applications without outputs, unknown dates and unnumbered QA.
Successful text correction applications require a result text unit; structured
rule-data corrections require a revision. Referenced revisions always match the
correction face and printing region. Reskins have one confirmed target per source
card, no reverse duplicate, no count or inherited DSL, and require authored
provenance from the confirmed registry. Other relation kinds are not assigned those reskin-only restrictions.

Checks do not calculate bundle freshness or prove semantic equivalence, exact
before/after/result content, complete errata scope,
reskin source evidence for all faces/regions, or adopted art baseline/alternate
classification. Those domain validators and importers are still required for
complete build validation. Source hashes and historic versions
must come from pinned inputs; schema validation never invents missing evidence.

Art rows come only from confirmed registry records; they carry no separate
decision. The schema does not prove art baseline, classification or raw
evidence validity.

## Saved build inputs

The shared raw source boundary in `sve_carddb.build.source_rows` reuses a source
version only when all metadata matches, with `parser_version=NULL`. Each actual
use keeps its own parser and archive pin in an immutable input record. The
[approved contract](../../../../docs/schema/ingest/source-archive.md#22-建置-source_record-的投影)
requires that record alongside the saved DB and report.

```python
from sve_carddb.core.provenance import BuildContext
from sve_carddb.build.output import save

context = BuildContext.from_inputs(program_revision, configuration)
with create_database(schema) as db:
    with db.transaction():
        record = populate_product_preview(
            db,
            catalog,
            plan,
            authored_revision=authored_revision,
            build=context,
            languages=languages,
            stores=stores,
        )
    save(db, new_build_directory, record.content(), report)
```

The input record summarizes actual uses. Frozen raw and metadata checks occur at
source reads; owners and references are validated by domain importers. Database
transactions retain FK, STRICT, CHECK and cross-table checks. Saving checks physical
integrity without repeating CHECK validation, then backs up this same
completed DB into a temporary directory, writes inputs/report, fsyncs and performs
a no-overwrite rename. No build seal or second population is required.
