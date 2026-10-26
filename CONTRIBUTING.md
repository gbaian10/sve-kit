# Contributing

Thanks for your interest in sve-kit! Contributions of any size are welcome.

This project is maintained by one person in Taiwan, so most design documents are in
Traditional Chinese. **Language is not a barrier:** issues, pull request discussions and
questions can be in any language, machine translation is fine, and if a Chinese design doc
is unclear, just ask in an issue. Conventions for code and docs are listed under [Languages](#languages).

## Ways to contribute

Use the [issue forms](https://github.com/gbaian10/sve-kit/issues/new/choose) to
report card data, mapping, translation or rule problems, or suggest features.
You do not need to write code, and any language is welcome. Include the card
number, region and official source URL where relevant.

For data contribution formats, see [authored layout](docs/schema/domains/authored-layout.md).

## Contribution licenses and sources

- Material you intentionally submit for inclusion through a PR or issue uses its
  destination path's terms in [LICENSING.md](LICENSING.md). Your own translation
  contributions use CC0; rights in the official source text remain unchanged.
  You offer only rights you hold and can grant. No separate copyright assignment,
  CLA or DCO sign-off is required.
- Distinguish your own suggestions from official or community wording. Cite the
  source and its date or version where known. Official SVE Japanese and English
  card wording may be included in Git; card images and audio remain excluded.
  This decision does not extend to original inputs from other games such as SV1
  or Worlds Beyond. Link to other third-party sources instead of copying their
  contents; neither a source link nor this repository policy grants permission
  to relicense official or third-party material.

## Parsed card inputs

The reviewed Japanese and English SVE baseline is stored in `card-data/`, with
a generator and validator. Offline builds read these admitted inputs directly. Follow the
[parsed-input contract](docs/schema/ingest/parsed-card-inputs.md) for generation,
fixed card-number ranges and validation against the previous data tree. Keep raw
archives and old observations: parsing is reproducible only from preserved sources,
and updating a hash does not authorize replacing reviewed history.
Card JSON contains readable records and minimal source references. Adjacent
`.evidence.json` files hold build evidence with shared tables for repeated values;
the complete reader requires both files and separate immutable batch selections,
while reading card wording needs only the card file.
Generation and the `card-data` CI job validate schema, hashes, closure and history;
the CI job runs when card-data, carddb source/dependencies or its workflow/helpers
change and compares with the PR base tree. A missing base tree fails validation.
Ordinary builds use the trusted reader without repeating those checks. Local data
edits must pass `sve-carddb parsed-cards validate` before building. Offline text and
image-index builds default to parsed inputs; comparison runs explicitly set
`source_mode: "archive"`. Missing parsed inputs never fall back to HTML.
Publishable files contain source URLs and hashes, never archive store names, local
paths, credentials, card images or audio. A data PR must show its file count, byte
sizes, included fields and representative records for maintainer review. Runtime
consumers continue to use versioned snapshots.

## Effect author storage

Edit the format 2 main shards under `authored/rules/author-effects/` and the named
body shards under `authored/rules/effect-bodies/`. Per-owner files under
`authored/rules/effect-anchors/` are generated, committed exceptions to the usual
human-maintained data rule: regenerate them with explicit `author write --storage anchors`,
never edit their references or hashes by hand. Build and `author check` only compare
committed anchors; a mismatch rejects the whole card. See the
[authoring commands](docs/dsl/authoring-tools.md) for source profiles, selection and validation.
A change to the shared adapter or schema resources can stale many owners; update the
inputs and all affected anchors in the same PR using `write --all`, then run the full check.
Pinned resources include `author-nodes.schema.json` and the raw bytes of
`author-storage.schema.json`, `effect-body-storage.schema.json` and `effect-anchors.schema.json`;
even formatting the three storage schemas can stale every owner's anchors.
Keep existing review literals and disputed verdicts; refreshing anchors does not renew a review.

## Languages

| What                               | Language                                    |
| ---------------------------------- | ------------------------------------------- |
| Issues and pull request discussion | **Any language is welcome**                 |
| Code, identifiers, code comments   | English                                     |
| Commit messages                    | Any language (the format below is required) |
| ADRs and design docs in `docs/`    | Traditional Chinese for now                 |
| README                             | English                                     |

## Commit messages

The format is checked by commitizen on every commit, and again in CI for every commit in a pull
request and for the pull request title (a squash merge uses it as the commit title). CI requires the
gitmoji; locally the gitmojify hook adds it for you. Messages that start with `Merge`, `Revert`,
`Pull request`, `fixup!`, `squash!` or `amend!` are let through unchecked. The scope is optional:

```text
<gitmoji> <type>(<scope>): <description>
<gitmoji> <type>: <description>
```

```text
✨ feat(carddb): add Japanese card list crawler
🐛 fix(sim/web): 修正長按選單
📝 docs: 新增 ADR 0001
```

Use the component as the scope, so `git log --grep '(carddb)'` shows one component's history:

| Scope        | Component                                    |
| ------------ | -------------------------------------------- |
| `carddb`     | Card data pipeline                           |
| `authored`   | Human-maintained data                        |
| `dsl`        | Effect DSL schema                            |
| `sim/engine` | Rules engine                                 |
| `sim/server` | Game server                                  |
| `sim/web`    | Web client                                   |
| `docs`       | ADRs and design docs                         |
| _(none)_     | CI, tools, top-level docs (`ci` issue label) |

Each component is versioned on its own, and its next version is worked out from the commits
that carry its scope. So keep one component per commit: when a change touches several components,
split it into one commit per component. Commits without a scope never bump a component version.
Write the description and body in one natural language; do not repeat the same text in two languages.
The gitmoji, type and scope are fixed tokens and do not count.

### Trailers

Credit authors and reviewers in the PR description so their contributions can be
recorded in the squash commit. Use `Acked-by` only when the maintainer personally
approved that change. See [AGENTS.md](AGENTS.md) for automated contribution and
review procedures.

## Coding standards

Reviewers apply this section to every pull request. Formatting, lint, types and
the commit format are enforced by tools and are not repeated here.

### Design

- Prefer deep modules: a small interface over substantial behaviour. Merge modules
  that only forward calls to each other.
- Nothing has shipped yet: replace an old format or algorithm outright and regenerate
  its fixtures, without a compatibility path.
- The project has one maintainer on one machine. Add locks, ledgers, receipts, replay
  or extra verification layers only for a stated requirement the maintainer agreed to.
- Keep one-off migration and conversion scripts out of the repository.
- Manage dependencies through package managers and lockfiles rather than assuming
  the maintainer's system library versions. Pass machine-specific paths and host
  names through configuration.
- Linux is currently supported for development, tests and builds. macOS work is
  deferred to [#315](https://github.com/gbaian10/sve-kit/issues/315); Windows is
  unsupported, with known blockers recorded in [#316](https://github.com/gbaian10/sve-kit/issues/316).
  This does not limit the platforms of Web users.

### Names and interfaces

- One name, one meaning: do not import a type under the name of a different type
  (for example `SourceRegion as Region`).
- Import only public names from another module; make a helper public before reusing it.

### Schema and contracts

Snapshot wire changes must pass the [format lock check](docs/schema/export/snapshot-contract.md#格式指紋鎖定): update the lock in the same PR while unfrozen, or bump the format under [transport §1.1](docs/schema/export/snapshot-transport.md#11-版本與相容判斷) once frozen.

### Tests

- Test behaviour through the public interface. A test that restates the
  implementation (copied constants, private call order or counts, source text) is a defect.
- Do not mock the code under test.
- Build expensive setup once. Anything that loads the real repository (author set,
  public profile, catalog, build database or real-current results) is a session-scoped
  fixture shared by every test that needs it; tests copy before they write.
- Keep one real-data end-to-end test per behaviour. Rejection paths, edge cases and
  parameter combinations use the smallest synthetic input that shows them; never
  parametrize a full-repository run.
- CI fails any test whose setup, call and teardown together exceed 30 seconds
  (`SVE_TEST_TIME_LIMIT` overrides the limit); local runs warn instead. An exception
  requires `@pytest.mark.slow(reason="concrete reason")`. Shared preparation is
  counted only for its first consumer. For an expensive shared fixture, put
  `@slow_fixture(reason="concrete reason")` below `@pytest.fixture(scope="session")`
  (import it from `tests.support.time_budget`); its setup and finalizers are exempt
  from the consumer's budget. Empty or missing reasons are errors. Record the
  touched files' pytest time and explain remaining slow tests in the PR.
- Architecture boundary, fresh-import and isolation tests are deliberate structural
  checks. A new boundary rule comes with a negative case that proves it fails.
- Never print official card wording in test output.

### Comments

AI-generated code tends to over-comment. Keep comments few and short:

- Explain **why**, not what — the code already says what it does.
- Comment only where a reader would otherwise get it wrong.
- One or two lines at most. Longer background belongs in an ADR; reference it by number.
- Do not restate the function body or write comments like "call X here".

```python
# Evolution swaps base stats without a stat-change event; only super evolution's +1 counts (see ADR-0008).
```

### Documentation

- Use [the project glossary](GLOSSARY.md) for project-specific names. Resolve
  new or renamed terms there first: one or two sentences, rejected aliases, no
  implementation details; group related terms naturally.
- Write an ADR only for a hard-to-reverse decision with a surprising rationale and
  a real trade-off. Use one decision and a short context/decision/reason paragraph;
  optional sections must add useful information. See [the ADR index](docs/adr/README.md)
  for numbering and supersession. Put contracts in `docs/schema/` or `docs/dsl/`,
  and progress, scheduling and open questions in issues or milestones.

- Verify references to implemented modules, classes, files and commands; label
  planned ones explicitly.
- Keep claims about current behaviour within what the tests guarantee. Distinguish
  design requirements from implemented guarantees.

### Choosing libraries

Use the standard library or a maintained, mature package for general-purpose
parsers, schema validation, signatures, locks, queues and CLI plumbing. Keep
project-specific rules at the package boundary rather than writing a parser or
reimplementing those general-purpose facilities. Avoiding a dependency is not
itself a reason to write a replacement.

Record only the license, Linux and macOS support, required behavior, performance
and size, and maintenance status when choosing a library. If available libraries
cannot meet the requirements, document the specific gaps for review. Add a new
dependency in a small dependency-only PR before integrating it into application
code.

## Lint and types

ruff (all rules) and mypy strict run on every commit and in CI; mypy is the type-checking gate.
Type hints are required. pyright strict is an optional local check (a manual hook, not run in CI,
because it bundles a large Node.js runtime); run it when you like and fix what it reports as you go:

```bash
pre-commit run --hook-stage manual pyright   # carddb and tools/schema-er
```

Follow the rules; if a rule is wrong for the whole project, propose changing the config
instead of silencing it everywhere.

When a rule must be silenced, use rule **names** (not codes) and give the reason:

```python
x = random.random()  # ruff: ignore[suspicious-non-cryptographic-random-usage] -- timing jitter
```

```python
# ruff: file-ignore[magic-value-comparison] -- parser tests compare literal page values
```

- One line: `# ruff: ignore[...]` at the end of that line
- A whole file (the same rule repeated many times): `# ruff: file-ignore[...]` at the top
- The old `# noqa` form is rejected by the linter

Rust: clippy runs on every commit with `-D warnings` and every group on, including
`restriction`; the few lints that contradict each other are allowed in `Cargo.toml` with
the reason. The toolchain version is pinned in `rust-toolchain.toml`. Silence a lint with `#[expect]` and a reason;
use `#[allow]` only when the lint does not fire everywhere the attribute applies:

```rust
#[expect(clippy::unnecessary_wraps, reason = "scripts mix steps with engine errors")]
```

## What goes into git

- **In:** code, shared human-written data (`authored/`), formal docs (`docs/`),
  reviewed parsed card inputs (`card-data/`, the explicit reproducible-data exception),
  and permitted official SVE Japanese/English test fixtures.
- **Out:** regenerable data outside those explicit exceptions (crawl caches, images, audio, build output),
  personal settings, secrets and local notes.
- Files larger than 1 MiB (1024 KiB) are rejected by pre-commit. Split `authored/` data per card set.

## Setup

Install these tools with their own installers (each is one command; see the linked pages):

| Tool                             | Used for                                                       | Install                                                           |
| -------------------------------- | -------------------------------------------------------------- | ----------------------------------------------------------------- |
| [rustup](https://rustup.rs)      | Rust; the toolchain version is pinned in `rust-toolchain.toml` | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| [uv](https://docs.astral.sh/uv/) | Python (`carddb`, `tools/schema-er`)                           | `curl -LsSf https://astral.sh/uv/install.sh \| sh`                |
| [mise](https://mise.jdx.dev)     | Bun and Node.js plus shared project environment variables      | `curl https://mise.run \| sh`, then `mise install` in the repo    |

mise manages Bun and Node.js versions and shared project environment variables.
Rust and Python toolchains remain managed by rustup and uv; do not add them to
`mise.toml`. Machine-specific paths and secrets stay in ignored `mise.local.toml`
or a private parent-directory mise configuration. No project variable currently
has a repo-level mise default: export roots must be outside the repo, and source
history, backups and image version state need explicit persistent locations.
Do not declare unset variables as empty strings or placeholder paths.

Precedence is explicit CLI option > caller environment > `mise.local.toml` >
private parent configuration > repo default. For future approved repo defaults,
use `{{ env.X | default(value=…) }}` so values from parent mise configuration
survive. Private and local settings use `get_env`, which checks the caller's
OS environment and preserves overrides supplied by a shell or CI:

```toml
[env]
SVE_DATA_DIR = "{{ get_env(name='SVE_DATA_DIR', default='/absolute/local/data') }}"
SVE_EXPORT_DIR = "{{ get_env(name='SVE_EXPORT_DIR', default='/absolute/local/export') }}"
SVE_CARDDB_PRIVATE_DIR = "{{ get_env(name='SVE_CARDDB_PRIVATE_DIR', default='/absolute/local/carddb-private') }}"
```

These paths are examples; replace them in private configuration. An activated
shell can retain old values: restart it or explicitly export updated values after
changing configuration. Empty `SVE_EXPORT_DIR`, `SVE_CARDDB_PRIVATE_DIR` and
`SVE_PREVIEW_DIR` environment values are treated as unset.
Web uses the synthetic fixture for an empty `SVE_EXPORT_DIR` and leaves
`/cdn-preview` unconfigured for an empty `SVE_PREVIEW_DIR`. carddb still requires
an explicit CLI root when its environment value is empty; an empty CLI value is
rejected. Non-empty roots must be absolute paths. Export keeps
its existing resolved-path checks separating public output, private state and
immutable inputs; image production uses a separate persistent private root for `media-state.json` and its backups.
Missing configuration fails only commands that require it. Help, readers and
synthetic tests work without private roots. CI sets its own required values in
workflow steps and does not depend on local mise configuration.

| Variable | Purpose | Reader | Required / default |
| --- | --- | --- | --- |
| `SVE_DATA_DIR` | Latest source cache and manifest | carddb crawler and manifest commands | Required for live-data commands; no default; pytest replaces it with a temporary root |
| `SVE_EXPORT_DIR` | Public export root | carddb `snapshot export-offline` (`--preview-dir`), publish `snapshot upload` (`--export-dir`), Web `/cdn` | Export/snapshot upload require CLI or env; no default; Web uses synthetic fixture when unset or empty |
| `SVE_CARDDB_PRIVATE_DIR` | Text inputs and reports | carddb `snapshot export-offline` (`--private-dir`) | CLI or env required; no default; recipe and `--bundle-dir` remain explicit |
| `SVE_PREVIEW_DIR` | Optional second local snapshot root | Web `/cdn-preview` | Optional; no default; unset or empty leaves the root unconfigured; exporter uses `SVE_EXPORT_DIR` |
| `SVE_ARCHIVE_ROOT` | Immutable source store | carddb archive operations | Required when using archive configuration; no default |
| `SVE_ARCHIVE_STORE_ID` | Archive store identity | carddb archive operations | Required by configured archive operations; no default |
| `SVE_ARCHIVE_BACKUP_ROOT` | Immutable source backup | carddb seal/backup operations | Required as applicable; no default |
| `SVE_ARCHIVE_RESTORE_ROOT` | Restore-check destination | carddb restore-check | Required as applicable; no default |
| `SVE_EXTRA_ROOTS` | Read-only symlink target allowlist (`os.pathsep` separated) | carddb manifest check | Optional; Settings defaults to no extra roots |
| `SVE_INTERVAL` | HTTP request interval | carddb `ingest.config.Settings` | Optional; 2.5 seconds, minimum 2 |
| `SVE_JITTER` | HTTP request jitter | carddb `ingest.config.Settings` | Optional; 0.5 seconds, non-negative |
| `SVE_TIMEOUT` | HTTP timeout | carddb `ingest.config.Settings` | Optional; 30 seconds, positive |
| `SVE_USER_AGENT` | Browser User-Agent | carddb `ingest.config.Settings` | Optional; existing browser UA |
| `SVE_BREAKER_THRESHOLD` | HTTP circuit breaker threshold | carddb `ingest.config.Settings` | Optional; 5, positive integer |
| `SVE_TEST_SNAPSHOT` | Generated temporary JP test JSONL | Rust card tests | Set by `.github/ci/rust_tests.py`; direct Cargo tests fail if it is missing |
| `SVE_R2_ACCESS_KEY_ID`, `SVE_R2_SECRET_ACCESS_KEY` | R2 credentials | publish upload/remote GC SDK | Required for remote execution only; no default; keep secrets outside the repo |
| `R2_ACCOUNT_ID`, `R2_DEV_BUCKET` | R2 account and target bucket | publish upload/remote GC | Execution requires CLI or env; no default |
| `SVE_PREVIEW_CONFIGURED` | Browser preview-root flag | Vite → Web UI | Derived by Vite; do not configure manually |
| `SVE_VOICE_ORIGIN` | Planned voice source selection | No implemented reader (build-db design only) | Unimplemented; no default |

`SVE_CDN_DIR` has been replaced by `SVE_EXPORT_DIR`. HTTP tuning defaults and
validation stay in carddb `ingest.config.Settings`, rather than being duplicated in mise.
Local paths and R2 credentials must never be injected into browser code.

Then:

```bash
uv --directory carddb sync --all-groups
uv --directory publish sync
cargo install --locked cargo-deny cargo-machete cargo-llvm-cov cargo-mutants
uv tool install pre-commit
pre-commit install
```

The git hooks run the quick checks on every commit and push. `pytest`, `cargo-test` and `web-test`
are manual hooks, because a full run takes minutes; CI runs each of them when a pull request changes
that component or a shared input. The Python manual hook enforces 90% combined line and branch
coverage; Rust tests enforce 90% line coverage for both project and fork PRs.
Python project and fork PRs use the same complete test scope and 90% combined line and branch threshold.
publish has its own 92% combined line and branch coverage gate, including fork PRs,
and requires no private test data.
Direct `pytest` runs do not enable coverage, so you can run selected files or tests without the
full-suite gate; use the Python manual hook for the complete coverage check.
Web tests enforce 85% branch coverage (`coverage.thresholds` in `sim/web/vite.config.ts`) when CI runs
Vitest with `--coverage`; local `vitest` runs do not enable it.

carddb and publish tests each use their own session-wide isolation guard. It supplies a temporary
`SVE_DATA_DIR` and requires file-backed SQLite databases and source writes to stay
under the pytest temporary root, including resolved symlink targets. An unset or
non-temporary `SVE_DATA_DIR` is rejected before data I/O. Each xdist worker permits
the shared pytest run root for immutable fixture templates. Standard-library
temporary directories also use this isolated root, rather than the ambient system
temporary directory. SQLite in-memory
databases, coverage data files and Python bytecode caches remain available;
these artifact exceptions do not permit arbitrary manifest files outside the
temporary root. Bytecode caches under any `__pycache__` directory are allowed,
including importlib's temporary `.pyc.<id>` files, so a first import of a standard
library or dependency module can populate its cache.

Use `httpx.MockTransport` or a localhost fake server. Default HTTP transports
reject external URLs before DNS, and a Python audit hook also rejects external
DNS, TCP and UDP operations. The audit hook and HTTP patches belong to the test
session, so an individual test's `monkeypatch.undo()` cannot remove them. The
guard checks both UDP `sendto` and `sendmsg`; filesystem Unix sockets are allowed
only under the pytest temporary root, including at bind time. Abstract Unix
socket addresses are outside the supported scope. Do not modify the session
guard's private state or remove its session patches.

Known limits: SQLite `ATTACH` and `VACUUM INTO` bypass the connection audit;
`os.mkfifo` has no covered audit event; and an `open` audit event omits `dir_fd`,
so relative open paths are checked against the current working directory.
Tests must not use these operations to access paths outside the temporary root.
Already-open file descriptors are not revalidated for each write. The
guard covers Python I/O in each pytest worker; it is not an operating-system
sandbox for subprocesses. Existing Git fixture subprocesses operate offline on
synthetic repositories. Do not add tests that invoke external network tools.

carddb tests follow the source layers under `carddb/tests/`: `core`, `contracts`,
`ingest`, `parse`, `build`, `domains/<domain>`, `images`, `export` and `workflows`.
Cross-layer boundary and isolation checks live in `architecture`; shared fixtures,
fake sites and pytest plugins live in `support`. The root `conftest.py` registers
session fixtures and plugins for all layers. Pinned and synthetic fixture files stay
in `fixtures`.

Layer directions in carddb and publish are enforced by
`carddb/tests/architecture/test_core_boundary.py`,
`carddb/tests/architecture/test_contracts_boundary.py`,
`carddb/tests/architecture/test_pipeline_boundaries.py`,
`carddb/tests/architecture/test_module_imports.py` and
`publish/tests/test_import_boundary.py`; those tests are the authority.

Run them yourself when you change the code they cover:

```bash
pre-commit run --hook-stage manual pytest-publish # publish tests with a separate 92% gate
pre-commit run --hook-stage manual pytest       # carddb tests with the 90% combined line/branch gate
pre-commit run --hook-stage manual cargo-test   # engine tests with the 90% line-coverage gate
pre-commit run --hook-stage manual web-test     # sim/web Vitest (CI calls the same package scripts in separate steps)
```

### CI tests and summaries

CI keeps the existing component jobs and the required `ci-ok` gate. Python lint and types
run through pre-commit; pytest runs once in its own step with `--cov`, the same combined
line + branch coverage threshold of 90%, `--durations=30` and JUnit. Cargo llvm-cov likewise
runs directly with its 90% line threshold. Web formatting, lint, types, Vitest, dependency
checks and build use the existing package scripts in separate steps. Local manual test hooks
remain available. The hook ownership table marks direct steps so pre-commit skips them.

CI also uploads the carddb, Web and Rust coverage reports (file names and line hits, no card text) and the
Web bundle size to Codecov for reference. Codecov statuses are informational; the gates above stay in the
test steps, and a failed or slow upload never fails the job (each upload step has its own timeout).
The carddb pytest results go to Codecov Test Analytics only after `.github/ci/junit_public.py` rewrites them:
parameters become a short hash, failure messages and captured output are dropped, because the raw JUnit
can hold card wording. The Web results come from `sim/web/scripts/ci/public-junit-reporter.ts`, which names
a case by a hash of its title and records each Vitest retry as a failed attempt, so tests that pass only on
retry show up as flaky (`--retry=2` hides them from the built-in reports).

Draft pull requests run only the quick checks (repository hooks, commit messages and the
title). The Python, Rust, Web and card-data jobs are skipped and `ci-ok` fails with a note that tests
were not run. Marking the PR ready for review starts the full run, which decides `ci-ok`.

In pull requests the repository-wide hooks check only the files the PR changes, as the local
commit hook does; the gitleaks scan still covers the PR's whole commit range. Changes to shared
hook configuration, workflow/action metadata or Python lock inputs trigger a full scan, as do
schema input deletions or renames. The `full_scan_inputs` list in `.github/workflows/ci.yml`
includes Markdown, EditorConfig, Git attributes, gitleaks, Taplo and uv settings. Pushes
to `main` also check all files.

Open **Actions → a workflow run → Summary** for test totals, passed/skipped/failed counts,
elapsed time and coverage. Python and Web include the slowest 30 cases and counts/time per
source file. Case times include setup/teardown and overlap under parallel execution; they
are not wall time. Rust reports aggregate test-binary counts/time and build/coverage wall time.
Rust per-case timings are not collected by the stable libtest reporter.
The **Failed tests** section lists all reported failures regardless of duration: Python
file/function with parameters removed, Web file/case ordinal, and Rust libtest test paths.

Raw test output and JUnit may contain official card wording. They stay in runner temporary
files, are removed after the job and are never uploaded or cached. Summaries omit failure
messages, captured output, parameter values and arbitrary Web case descriptions; use the
safe file/function location (or Web case ordinal) to reproduce failures locally. A missing
report is reported explicitly and cannot make a failing test step pass.

Every job uses GitHub-hosted `ubuntu-latest`. **Before merging, the PR must be rebased onto
the latest `main` and its CI must be green**; the ruleset does not enforce this prerequisite,
so the maintainer and merge coordinator must check it.

`CI_PYTEST_WORKERS` is an optional repository variable, default `4`, accepted range `1`–`4`.
CI never uses `-n auto`; Cargo and Vitest are also limited to four workers. The existing
jobs are retained rather than adding a job per test step, so summaries do not add per-job
billing overhead. Concurrency cancels older PR runs; `main` runs are never cancelled by a
newer push. Dependency caches retain their existing keys and only `main` writes them;
PRs read the default-branch caches. Each hook cache is restored,
prepared and saved separately to avoid mixing pre-commit databases. Existing mypy data
is carried forward; PR mypy still validates source hashes and checks changed modules,
so a retained cache is never proof that current types passed. Rust target caching remains
enabled: official test data is read at runtime and is not embedded in the compiled tests.
Official test content, raw reports and output are never cached. Checkout explicitly cleans
the workspace so test execution does not depend on a previous job's outputs.

### Official fixtures and fork PRs

The 16 SVE JP/EN parser fixtures are public under `carddb/tests/fixtures/official_jp/`
and `official_en/`. `official-pages.json` retains their original file and field hashes;
`OFFICIAL-SOURCES.md` records provenance and the limits of the curated source pages.
All project and fork runs execute these cases alongside the synthetic boundary tests.

Rust uses a temporary JSONL generated by carddb from the admitted `card-data/` and
the explicit JP batch in `tests/engine/card-input.json`. It contains every current
printing and face; the engine continues to apply its existing authored effects,
rule names and token selection. This is a test adapter, not a production snapshot
or a separate maintained card list. Update the batch pin explicitly when changing
the test baseline. `.github/ci/rust_tests.py` generates a fresh file, replaces any
inherited `SVE_TEST_SNAPSHOT`, runs Cargo, and removes the temporary data. Both the
manual `cargo-test` hook and CI use it. Project and fork PRs run the same complete
workspace with the 90% line coverage gate, without private credentials. Direct
Cargo snapshot tests fail if the generated input is missing; they never skip.

The permanently private repository keeps its old JSONL, 19 original files and full
history; do not delete, publish or rewrite it. Python does not read that repository.
The 16 public SVE fixtures retain their field-level hashes, and synthetic SV1/SVWB
pages exercise the other parser shapes without copying third-party wording.
Project and fork Python runs execute the same public fixtures and synthetic tests
with the complete 90% combined line and branch coverage gate. Generated JSONL,
official test content and raw test logs are not cached or uploaded as artifacts.

Now and then, and before a release, run the mutation test. Every surviving mutant
is a bug the tests would not notice; add a test, or explain why it cannot change behaviour:

```bash
pre-commit run --hook-stage manual cargo-mutants
```

## Keeping tools up to date

| What                                                                | How it is updated                                                                                   |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| GitHub Actions (pinned by commit SHA)                               | Dependabot, weekly (`.github/dependabot.yml`)                                                       |
| Rust and Python (`carddb`, `publish`) dependencies                  | Dependabot, weekly, one grouped pull request per ecosystem (version updates)                        |
| Bun dependencies (`sim/web`)                                        | `bun update` and `bun audit` by hand, monthly (Dependabot cannot read `bun.lock` version 2 yet)     |
| pre-commit hook versions (`rev:`)                                   | Dependabot, weekly, one grouped pull request                                                        |
| `cz-conventional-gitmoji` in the commitizen hook                    | Pinned in `.pre-commit-config.yaml`; bump it with its hook's `rev:` and the `carddb` dev dependency |
| Node for the markdownlint hook                                      | `language_version` in `.pre-commit-config.yaml`; keep it equal to `node` in `mise.toml`             |
| Image for the lychee link-check hook                                | Tag and digest in `.pre-commit-config.yaml`, by hand                                                |
| Bun and Node                                                        | `mise.toml`, by hand                                                                                |
| Rust toolchain                                                      | `rust-toolchain.toml`, by hand, together with `rust-version` in `Cargo.toml`                        |
| cargo tools in CI (`cargo-llvm-cov`, `cargo-deny`, `cargo-machete`) | The `tool:` versions in `.github/workflows/ci.yml`, by hand                                         |

When a carddb dependency update (including Dependabot) changes `carddb/pyproject.toml`,
also re-lock `publish/uv.lock` with `uv --directory publish lock`, or publish's
`--locked` checks will fail.

For version updates Dependabot waits 7 days after a release before proposing it; security updates
are proposed at once, one pull request each. Workflow files are also checked by
actionlint (syntax and expressions) and zizmor (security) on every commit and in CI.
GitHub secret scanning with push protection, Dependabot alerts and malware alerts are enabled
for the repository; gitleaks still checks commits locally and in CI.

## Crawling etiquette

If you run the crawler, keep at least 2 seconds between requests and only fetch
content that is new or changed. Please do not put load on the official sites
or on community-run databases.
