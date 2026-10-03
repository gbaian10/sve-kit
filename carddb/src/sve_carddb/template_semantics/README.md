# Historical template replay

This component consumes inventory v2 through installed, finite semantic versions.
The contract is [template-source-replay](../../../../docs/schema/template-source-replay.md).
It does not adopt definitions, translate text, fetch sources, or write authored inputs.

`versions.py` owns the manifest path/hash and recipe entrypoints. `registry.py`
verifies the producer's regular Git blobs and the installed fixed files separately.
Git content is evidence and is never executed. Semantic calculations under `v1/`
retain parser, partition, parameter, flavor-owner and summary behavior; models and
reviewed source/identity/policy readers are explicit boundaries. The eight existing
recognition matcher paths remain unchanged and pinned. Future behavior needs a new
ID and retained old files; denied IDs must name their reason and reviewed fix.
There is no fallback to the latest parser or whole-host-runtime equality gate for v2.
The legacy v1 consumer retains its original checks.

Formal consumption requires every semantic binding revision and recipe code
revision to be an ancestor of the explicitly pinned main revision, including
cache hits. Missing history refuses consumption. Private candidates may use the
producer's own history as their pinned main for engineering previews; that does
not make a discarded branch commit eligible for formal adoption. The first formal
inventory must be regenerated with the actual merged producer commit.

The complete canonical recipe/context selects a session cache group. The loader
reads every inventory from immutable Git before comparing its full entries. A
separate source plan enumerates raw, field, glossary and identity uses before replay.
All six hash-only streams include unadopted entries and pending/unknown states.
Environment differences are recorded; identical semantic outputs remain valid.
Drift refuses consumption with `replay_output_mismatch` and the affected stream.

The offline CLI uses an explicit host revision and store map:

```sh
uv --directory carddb run python -m sve_carddb.template_semantics generate \
  --repository .. --host FULL_HOST_SHA --store STORE_ID=/outside/archive \
  --legacy /outside/templates.jsonl --kind effect \
  --inputs /outside/effect-inputs.json --output /outside/new-candidate

uv --directory carddb run python -m sve_carddb.template_semantics replay \
  --repository .. --host FULL_HOST_SHA --store STORE_ID=/outside/archive \
  --legacy /outside/templates.jsonl --candidate /outside/new-candidate \
  --index-hash sha256:EXACT_SEPARATELY_RECORDED_HASH --output /outside/new-f1
```

Paths are interpreted from the carddb working directory. Proposed vocabulary,
when required by the pinned references, also requires `--vocabulary` and
`--vocabulary-basis`. Effect inputs contain exactly `recognition_policy`,
`source_batch`, and `references`; those business pins alone govern the recipe.
Flavor inputs are the closed `FlavorReplayInputs` object, including `kind`, its
own batch, identity basis and complete identity batches. A null basis remains a
pending preview. Generation publishes private, size-bounded YAML-compatible JSON
shards and an exact-byte index; it does not claim replay or adoption success.
Independent replay reloads the separately hash-pinned index and every shard.

The replay result is one four-file F1 bundle (`build.sqlite`, `inputs.json`,
`report.json`, `seal.json`). Its program/dependencies are the actual executing host
H. Per-group producer R and environment/semantic evidence remain separate in its
configuration. Raw source records have null parser metadata; distinct uses retain
all parsers, batches and locators. Publication verifies independent expected uses,
DB metadata and archived raw inputs transactionally before publishing the bundle.
Reports and CLI failures do not print official wording.

`--budget` accepts a closed JSON object, format 1, with optional lower
`wall_seconds` and `rss_bytes`. Defaults/hard maxima are 1800 seconds and 6 GiB;
optimization targets are 1200 seconds and 4 GiB. The estimate counts historical
groups and retained outputs, and RSS includes native allocations. A main-thread
watchdog aborts over-budget work with `replay_budget_exceeded`; it never skips
history, raises limits, or publishes a partially verified bundle. There is no
persistent replay cache or historical interpreter environment.

Producer package versions are fixed by `v1/environment-artifacts.json`. Upgrading
any of those packages requires a reviewed, newly registered semantic version
before generating new inventories with that environment. Keep old versions and
files available. Historical consumption verifies the producer's recorded versions;
a different host version is recorded as an environment difference and is accepted
when all six outputs agree. Never edit v1 to accommodate a dependency upgrade.
