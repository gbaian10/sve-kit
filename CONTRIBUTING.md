# Contributing

Thanks for your interest in sve-kit! Contributions of any size are welcome.

This project is maintained by one person in Taiwan, so most design documents are in
Traditional Chinese. **Language is not a barrier:** issues, pull request discussions and
questions can be in any language, machine translation is fine, and if a Chinese design doc
is unclear, just ask in an issue. Conventions for code and docs are listed under [Languages](#languages).

## Ways to contribute

You do not need to write code. Many of the most useful contributions only need you to
know the game:

Use the issue forms to report problems or suggest features; you can write in any language.

| Contribution                                    | What you need                                                 |
| ----------------------------------------------- | ------------------------------------------------------------- |
| **Report wrong card data** (text, stats, links) | The card number and a link to the official card page          |
| **Check Japanese-English card mappings**        | Links to both official card pages and what you noticed        |
| **Fix or improve translations and terms**       | Read Traditional Chinese; cite the Japanese or English source |
| Annotate card effects                           | Know the rules                                                |
| Report rule or simulator bugs                   | Know the rules; steps to reproduce                            |
| Fix typos and docs                              | Anything                                                      |

Card data and translations are the easiest places for mistakes to slip in —
card numbers do not line up across regions, and there is no official Traditional Chinese
edition — so these reports are especially valuable. The maintainer confirms every
cross-region mapping by hand, so a report does not need to be certain.

**Always say where it comes from:**

- Card data: card number, region (Japanese or English), and the official card page.
- Translations: say whether it is your own translation, an official name from the digital
  Shadowverse games (with a link), or a community term.
- Do not paste card lists, images or translations copied from other sites, and do not
  upload card image files — link to the official page instead.

Data contributions are plain YAML files under `authored/`, split per card set.
The identity registry format is defined in [authored layout](docs/schema/authored-layout.md);
other data layouts remain proposed.

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

Pull requests are squash-merged, so the squash commit records who worked on the change.
Its message ends with these trailers, in this order, after a blank line that follows the body
and any issue references:

| Trailer          | Add one for                                                         | Example                                                |
| ---------------- | ------------------------------------------------------------------- | ------------------------------------------------------ |
| `Co-Authored-By` | every person or AI model that wrote part of the change              | `Co-Authored-By: Codex gpt-6-sol <noreply@openai.com>` |
| `Reviewed-by`    | every reviewer, person or AI model, who approved the final revision | `Reviewed-by: Claude Opus 5.5 <noreply@anthropic.com>` |
| `Acked-by`       | the maintainer, only when they approved this change themselves      | `Acked-by: Maintainer Name <maintainer@example.com>`   |

Name an AI model by its product and version, and the maintainer by the name and email in
`git log`. A change merged under the standing review rules, without the maintainer looking at it,
has no `Acked-by`. If you open a pull request, list everyone and every AI model that wrote part
of it in the description, so the maintainer can credit them in the squash commit.

Some changes are written and reviewed by AI models run by the maintainer. Their pull requests
are opened by bot accounts owned by the maintainer (`…[bot]`). AI review must pass on the local
branch before pushing and opening the pull request; the reviewer then posts one summary of all
rounds' findings, fixes and validation on the pull request. The trailers above record which models
wrote and reviewed the change, and whether the maintainer approved it personally.

## Code comments

AI-generated code tends to over-comment. Keep comments few and short:

- Explain **why**, not what — the code already says what it does.
- Comment only where a reader would otherwise get it wrong.
- One or two lines at most. Longer background belongs in an ADR; reference it by number.
- Do not restate the function body or write comments like "call X here".

```python
# Evolution swaps base stats without a stat-change event; only super evolution's +1 counts (see ADR-0008).
```

## Lint and types

ruff (all rules) and mypy strict run on every commit and in CI; mypy is the type-checking gate.
Type hints are required. pyright strict is an optional local check (a manual hook, not run in CI,
because it bundles a large Node.js runtime); run it when you like and fix what it reports as you go:

```bash
pre-commit run --hook-stage manual pyright   # carddb and docs/schema/er
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

- **In:** code, shared human-written data (`authored/`), formal docs (`docs/`).
- **Out:** anything a program can regenerate (crawled HTML, images, audio, build output),
  personal settings, secrets and local notes.
- Files larger than 1 MiB (1024 KiB) are rejected by pre-commit. Split `authored/` data per card set.

## Setup

Install these tools with their own installers (each is one command; see the linked pages):

| Tool                             | Used for                                                       | Install                                                           |
| -------------------------------- | -------------------------------------------------------------- | ----------------------------------------------------------------- |
| [rustup](https://rustup.rs)      | Rust; the toolchain version is pinned in `rust-toolchain.toml` | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| [uv](https://docs.astral.sh/uv/) | Python (`carddb`, `docs/schema/er`)                            | `curl -LsSf https://astral.sh/uv/install.sh \| sh`                |
| [mise](https://mise.jdx.dev)     | Bun and Node.js, pinned in `mise.toml`                         | `curl https://mise.run \| sh`, then `mise install` in the repo    |

Only Bun and Node.js are managed by mise; do not add Rust or Python to `mise.toml`,
so rustup and uv stay in charge of them. Machine-specific paths such as `SVE_DATA_DIR`
go in `mise.local.toml` (ignored by git), for example:

```toml
[env]
SVE_DATA_DIR = "/path/to/sve-kit-data"
SVE_TEST_SNAPSHOT = "/path/to/cards.jsonl"   # fixed card list the engine card tests read
```

Then:

```bash
uv --directory carddb sync --all-groups
cargo install --locked cargo-deny cargo-machete cargo-llvm-cov cargo-mutants
uv tool install pre-commit
pre-commit install
```

The git hooks run the quick checks on every commit and push. `pytest`, `cargo-test` and `web-test`
are manual hooks, because a full run takes minutes; CI runs each of them when a pull request changes
that component or a shared input. The Python manual hook enforces 90% combined line and branch
coverage; Rust tests enforce 90% line coverage. The same thresholds apply locally and in CI.
Direct `pytest` runs do not enable coverage, so you can run selected files or tests without the
full-suite gate; use the Python manual hook for the complete coverage check.
Run them yourself when you change the code they cover:

```bash
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

Every job uses GitHub-hosted `ubuntu-latest`. While the repository is private, a push to
`main` does not rerun pytest, Rust tests/coverage or Vitest. **Before merging, the PR must
be rebased onto the latest `main` and its CI must be green.** This is a merge prerequisite,
not something the cache-maintenance run verifies: its green `ci-ok` means maintenance
succeeded, not that checks or tests ran again on that commit. The private repository's ruleset
does not enforce this prerequisite; the maintainer and merge coordinator must check it.
When the repository becomes public, `github.event.repository.private` automatically restores
the usual component tests on pushes to `main`, without another workflow edit.

Private `main` starts only the existing `ci-ok` job. The other six jobs are skipped at job
level. In `ci-ok`, a guarded local composite action installs Python dependencies and hook
environments without running hooks, retains existing mypy data without running type checks,
compiles coverage-instrumented Rust tests with `cargo test --no-run` only on a cache miss,
and installs Web dependencies. Its final gate rejects failed, cancelled or incomplete
maintenance. The job keeps the exact required-check name `ci-ok`; on PRs and public `main`
it still aggregates the usual checks. No second maintenance gate job is started.

`CI_PYTEST_WORKERS` is an optional repository variable, default `4`, accepted range `1`–`4`.
CI never uses `-n auto`; Cargo and Vitest are also limited to four workers. The existing
jobs are retained rather than adding a job per test step, so summaries do not add per-job
billing overhead. Concurrency cancels older PR runs; `main` runs are never cancelled by a
newer push. Dependency caches retain their existing keys and only `main` writes them;
PRs read the default-branch caches. Private `main` populates uv, pre-commit, mise/Bun and
Cargo caches under the unchanged component cache keys. Each hook cache is restored,
prepared and saved separately to avoid mixing pre-commit databases. Existing mypy data
is carried forward; PR mypy still validates source hashes and checks changed modules,
so a retained cache is never proof that current types passed. This does not refresh mypy
data for changed code until a full `main` check runs after publication. Rust target caching remains
enabled: official test data is read at runtime and is not embedded in the compiled tests.
Private test data, raw reports and output are never cached. Checkout explicitly cleans
the workspace so test execution does not depend on a previous job's outputs.

When needed and explicitly requested, manually compare the old and new YAML readers with
`uv --directory carddb run pytest manual_tests/yaml_reader_equivalence.py`; CI never runs this check.

`cargo-test` needs `SVE_TEST_SNAPSHOT` (see Setup); CI gets the same file from a private test-data repository.

Now and then, and before a release, run the mutation test. Every surviving mutant
is a bug the tests would not notice; add a test, or explain why it cannot change behaviour:

```bash
pre-commit run --hook-stage manual cargo-mutants
```

## Keeping tools up to date

| What                                                                | How it is updated                                                                               |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| GitHub Actions (pinned by commit SHA)                               | Dependabot, weekly (`.github/dependabot.yml`)                                                   |
| Rust and Python (`carddb`) dependencies                             | Dependabot, weekly, one grouped pull request per ecosystem (version updates)                    |
| Bun dependencies (`sim/web`)                                        | `bun update` and `bun audit` by hand, monthly (Dependabot cannot read `bun.lock` version 2 yet) |
| pre-commit hook versions (`rev:`)                                   | `pre-commit autoupdate` by hand, monthly                                                        |
| `cz-conventional-gitmoji` in the commitizen hook                    | Pinned in `.pre-commit-config.yaml`; bump it together with the `carddb` dev dependency          |
| Node for the markdownlint hook                                      | `language_version` in `.pre-commit-config.yaml`; keep it equal to `node` in `mise.toml`         |
| Bun and Node                                                        | `mise.toml`, by hand                                                                            |
| Rust toolchain                                                      | `rust-toolchain.toml`, by hand, together with `rust-version` in `Cargo.toml`                    |
| cargo tools in CI (`cargo-llvm-cov`, `cargo-deny`, `cargo-machete`) | The `tool:` versions in `.github/workflows/ci.yml`, by hand                                     |

For version updates Dependabot waits 7 days after a release before proposing it; security updates
are proposed at once, one pull request each. Workflow files are also checked by
actionlint (syntax and expressions) and zizmor (security) on every commit and in CI.

## Crawling etiquette

If you run the crawler, keep at least 2 seconds between requests and only fetch
content that is new or changed. Please do not put load on the official sites
or on community-run databases.
