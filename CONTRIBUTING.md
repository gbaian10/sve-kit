# Contributing

Thanks for your interest in sve-kit! Contributions of any size are welcome.

This project is maintained by one person in Taiwan, so most design documents are in
Traditional Chinese. **Language is not a barrier:** issues, pull request discussions and
questions can be in any language, machine translation is fine, and if a Chinese design doc
is unclear, just ask in an issue. Conventions for code and docs are listed under [Languages](#languages).

## Ways to contribute

You do not need to write code. Many of the most useful contributions only need you to
know the game:

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

The format is checked by commitizen on every commit. The scope is optional:

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

| Scope        | Component                                       |
| ------------ | ----------------------------------------------- |
| `carddb`     | Card data pipeline                              |
| `authored`   | Human-maintained data                           |
| `dsl`        | Effect DSL schema                               |
| `sim/engine` | Rules engine                                    |
| `sim/server` | Game server                                     |
| `sim/web`    | Web client                                      |
| `docs`       | ADRs and design docs                            |
| _(none)_     | Repo-wide changes (CI, tooling, top-level docs) |

Each component is versioned on its own, and its next version is worked out from the commits
that carry its scope. So keep one component per commit: when a change touches several components,
split it into one commit per component. Commits without a scope never bump a component version.
Write the description and body in one natural language; do not repeat the same text in two languages.
The gitmoji, type and scope are fixed tokens and do not count.

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
| Docker                           | The markdownlint hook                                          | your platform's Docker install                                    |

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

Run the checks before committing:

```bash
uv --directory carddb run ruff check
uv --directory carddb run mypy
uv --directory carddb run pytest
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo llvm-cov --locked --workspace --fail-under-lines 90   # tests + coverage
```

Now and then, and before a release, run the mutation test. Every surviving mutant
is a bug the tests would not notice; add a test, or explain why it cannot change behaviour:

```bash
pre-commit run --hook-stage manual cargo-mutants
```

## Crawling etiquette

If you run the crawler, keep at least 2 seconds between requests and only fetch
content that is new or changed. Please do not put load on the official sites
or on community-run databases.
