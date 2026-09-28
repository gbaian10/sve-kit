# sve-kit

CI acceptance cancel line.
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

```bash
uv --directory carddb sync          # install dependencies
uv --directory carddb run pytest    # test
uv --directory carddb run ruff check
uv --directory carddb run mypy
```

## Contributing

Issues in any language are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for
language, commit message and code comment conventions, and how to set up the tools.
