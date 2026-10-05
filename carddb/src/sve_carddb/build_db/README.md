# Build DB core

This package compiles code-authored declarations into SQLite DDL and owns the
SQLite boundary. The [product family staging importer](../products/README.md)
can supply verified family parents and compose with identity staging in one
transaction. `t0.compile_t0()` supplies the forty production T0 tables from
`docs/schema/build-db.md`. The regional identity staging importer is described in
[registry/preview](../registry/preview/README.md); there is no complete build CLI.

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

## Optional T1 DDL and pipeline readiness

`t1.compile_build(("images", "cr"))` compiles the forty T0 tables plus the four
image and two CR tables: 46 tables. Selecting `images` alone closes to 44 tables;
`cr` alone closes to 42. The default selects only T0. DSL and keywords remain
reserved, unavailable capabilities; disabled nullable references remain NULL-only.
Enabling CR installs the existing `rules_profile_revision.cr_version_id` FK.
This selection is smaller than the minimum 57-table DDL inventory.

`Capability.implemented` means that DDL declarations exist. The separate
`importer_ready` and `validator_ready` flags default to false.
`Registry.require_usable(requested)` checks both flags for the complete resolved
graph, including required FK and query dependencies. All production capabilities
currently fail that gate: complete capability importers and domain validators are
still missing.
These flags are code-maintained registration claims, not evidence generated from
successful SQL compilation. A future preview pipeline must call this gate for its
actual supported subset; a release pipeline must additionally require the full
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
`transaction()` invokes it **after the body, before COMMIT**, on the final graph.
An insert, update or delete can temporarily violate a query check inside the
transaction, allowing related rows to be repaired together. Updating an image,
its decision or size configuration is checked even when no variant row changed.
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

The image checks enforce publishable variant parents and non-original output
settings. Row-local checks cover required source metadata, approved images being
available, WebP format and hash-derived paths. Available/approved source metadata
may have no variants yet. Physical source/bytes matching, decoding, exact
recipe output and the complete five-size image set for both orientations still require the future
importer/image builder/domain validator. SQL checks do not attest those facts.

## Schema version and rebuilds

Compiled schemas carry a positive signed 32-bit `version`, stored in SQLite's
`PRAGMA user_version` during creation and checked before every commit. Legacy
`compile_t0()` uses version 1; the optional T1 registry uses version 5, including
its T0-only selection. Version 2 introduced images/CR, version 3 the remaining T1
groups, and version 4 adopted art-use verification. Version 5 adds the optional
`translation_names` tables (`translation_use` and `translation_selection`). The
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

`t1.compile_minimum()` selects T0 plus images, CR, errata, correction, QA and
related: exactly 57 tables. `compile_minimum(include_en=True)` adds art and the
three region review/divergence tables: exactly 61. The `en` group explicitly
requires `art`; selecting `art` alone is also supported. No artist, baseline or DSL
placeholder tables are created. All importer/validator readiness
flags remain false. These counts attest DDL coverage, not release readiness.

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
classification. Those domain validators and importers are still required before
these DDL groups can pass `require_usable`. Source hashes and historic versions
must come from pinned inputs; schema validation never invents missing evidence.

Art rows come only from confirmed registry records; they carry no separate
decision. The schema does not prove art baseline, classification or raw
evidence validity.

## Saved build inputs

The shared raw source boundary in `sve_carddb.build_inputs` reuses a source
version only when all metadata matches, with `parser_version=NULL`. Each actual
use keeps its own parser and archive pin in an immutable input record. The
[approved contract](../../../../docs/schema/source-archive.md#22-建置-source_record-的投影)
requires that record alongside the saved DB and report.

```python
from sve_carddb.build_bundle import publish_bundle, verify_bundle
from sve_carddb.build_inputs import BuildContext
from sve_carddb.products import populate_product_preview, product_preview_uses

context = BuildContext.from_inputs(
    program_revision,
    {"carddb/uv.lock": dependency_lock_bytes},
    {"regions": list(plan.regions), "languages": language_configuration},
)
expected = product_preview_uses(catalog, plan, stores)


def populate(db):
    return populate_product_preview(
        db,
        catalog,
        plan,
        authored_revision=authored_revision,
        build=context,
        languages=languages,
        stores=stores,
    )


publish_bundle(
    schema,
    new_bundle_directory,
    context,
    expected,
    populate,
    {"identity": plan.report(), "products": catalog.report()},
    stores=stores,
)
verify_bundle(schema, new_bundle_directory, context, expected, stores=stores)
```

Callers pin the actual full program revision, exact dependency input bytes and
configuration; these are independent of the authored revision. Input comparison
is against that independently supplied context and use plan, never just a
recomputed self-hash. The four-file directory contains `build.sqlite`,
`inputs.json`, `report.json` and `seal.json`; the seal binds all three payload
hashes, and the report binds the input-record hash. Publication validates the
complete raw/use/archive closure inside the transaction and uses an atomic
no-overwrite directory rename after fsync. Failed builds publish no bundle.
Existing bundles are immutable; create a new destination for another build.

Offline verification rechecks archived source closure and first-receipt
metadata, expected uses, artifact hashes, schema and DB integrity.
`database.open_database` opens only a closed regular database without sidecars,
using read-only immutable mode; it never installs DDL or reads live manifests.
Low-level rebuild/populate APIs still support temporary work, but saving a DB
without its input record does not satisfy this bundle contract.
