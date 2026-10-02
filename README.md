# sve-kit

Unofficial toolkit for Shadowverse: EVOLVE — card database and battle simulator.
Not affiliated with or endorsed by Cygames or Bushiroad.

## Layout

Main paths only. Items marked _(planned)_ do not exist yet.

```text
sve-kit/
├── AGENTS.md              Project architecture and data rules (for AI coding agents)
├── CONTRIBUTING.md        Language, commit and comment conventions
├── CLAUDE.md              Imports AGENTS.md and CONTRIBUTING.md for Claude Code
├── .cz.toml               Commit message rules (commitizen, gitmoji)
├── .pre-commit-config.yaml
│
├── carddb/                Card data pipeline (Python, uv)
│   ├── pyproject.toml     Package: sve-carddb
│   ├── src/sve_carddb/    Crawl → parse → merge authored/ → build SQLite → export JSON
│   ├── tests/
│   ├── .cache/            Disposable temp files            (git-ignored, planned)
│   └── dist/              Build output: SQLite, snapshots  (git-ignored, planned)
│
├── authored/              Human-maintained data (YAML), split per card set
│                          Cross-region card IDs, zh-Hant translations, effect DSL data
├── dsl/                   Effect DSL JSON Schema — the only authority for its grammar
│
├── sim/                   Battle simulator (not started)
│   ├── engine/            Rules engine, shared by server / browser (WASM) / desktop (planned)
│   ├── server/            Authoritative game server (planned)
│   └── web/               Web client: game board, deck builder, card browser (planned)
│
└── docs/                  ADRs (docs/adr), DSL specs (docs/dsl), terminology; schema planned
```

## How the parts fit

```text
dsl/ ──validates──> authored/ ──read by──> carddb/ ──exports──> JSON snapshot ──read by──> sim/
```

- The versioned snapshot exported by `carddb` is the single source of truth for card data.
  `sim/` may load, bundle or cache snapshots (e.g. for offline use), but never maintains a separate card table.
- The card browser, deck builder and game board are one web app on one domain.
  Card images, voice files and snapshots are served from a `cdn.` subdomain; the game server runs on `ws.`.
- Anything a program can regenerate stays out of git; shared, human-written project data and docs go in.

## Raw data

Crawled HTML, card images, voice files and their source manifest are **not** stored in this repo.
Set `SVE_DATA_DIR` to a directory outside the repo:

```text
$SVE_DATA_DIR/
├── raw/jp/          Original HTML from the Japanese site
├── raw/en/          Original HTML from the English site
├── media/images/    Card images
├── media/audio/     Voice files
└── manifest/        Source URL, fetch time, ETag and hash of every file — cannot be rebuilt
```

## Development

### Adding reviewed JP errata sources

`crawl errata-new --urls reviewed-errata-urls.json` accepts a JSON array of exact
HTTPS announcement URLs on `shadowverse-evolve.com/errata/<slug>`. Validate the
selection with `--dry-run` first; live fetching requires the maintainer's separate
authorization. This entry point adds sources only: it skips trusted existing
sources without changing their metadata and stops on damaged, missing, archived,
or conflicting existing destinations. It never repairs or overwrites them.

Every request uses the configured browser User-Agent and a gap of at least two
seconds. Retryable failures get at most three attempts, then that URL is reported
as failed and the next selected URL is tried. Access denial, repeated rate limits,
the circuit breaker, and **any redirect** stop the run. A redirect to another
selected URL also stops: review its destination before changing the selection;
trailing slashes are not inferred. Page links and images are never fetched.

Bodies require HTTP 200, HTML media type, valid UTF-8, a nonempty title and a
nonempty `main`, `article`, or `.entry-content` container. This is a conservative
structural check, not verification of a correction or its dates. The command
stores exact body bytes under `raw/jp/errata/` with manifest kind `errata`; it
prints metadata only and does not import formal errata or change card text.

The container selectors have not been verified against a saved official JP
errata/news page. Once live fetching is authorized, first dry-run the reviewed
selection, then fetch **one** of the 16 approved URLs. Check its stored raw,
manifest metadata and body structure offline, seal and independently back up
that pilot batch, and pass its restore check before fetching the other 15 URLs.
If the pilot fails or redirects, stop and report metadata only; do not loosen
validation or retry the rest. A rejected body is not saved by this command.

This entry point holds the shared manifest lock and refuses unfinished requests,
raw temporary files, or configured archive work. It never runs general crawl
recovery or archive cleanup. Before publication, caught interruptions clean only
the temporary file created by this write. Published raw files are complete and
installed without replacing any existing destination. A hard process kill can
leave a private temporary file; an interruption between publication and manifest
commit can leave a complete unregistered raw. The next run refuses these states
and requires a separately reviewed recovery, rather than deleting or overwriting
them.

Recovery belongs to the maintainer or a named operator under a separately
reviewed plan. There is currently **no scoped errata recovery command**. Do not
run ordinary `crawl` (including resume/repair) or `refresh` to clear the blockage,
delete a lock, remove raw files, or restore an older manifest over the live one.
Preserve the interrupted state, create a new locked manifest backup, inspect
the closed backup and affected files offline, then request a recovery tool
limited to the reviewed request IDs, URLs, paths and hashes. The detailed
[recovery procedure](docs/schema/refresh-operation.md#勘誤新增入口中斷後的處置)
requires evidence preservation, synthetic rehearsal and explicit maintainer
approval before any recovery writes.

The operator must back up the manifest before and after the run, then seal the
reviewed `jp:errata` scope, back up its closure and verify this batch with
`archive restore-check`; these operations are explicit and are not performed by
`errata-new`. See [source archive](docs/schema/source-archive.md) and
[protected fetching](docs/schema/refresh-operation.md). Official bodies stay
outside git, tests, reports and public snapshots.

```bash
uv --directory carddb sync          # install dependencies
uv --directory carddb run pytest    # test
uv --directory carddb run ruff check
uv --directory carddb run mypy
```

## Contributing

Issues in any language are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for
language, commit message and code comment conventions, and how to set up the tools.
