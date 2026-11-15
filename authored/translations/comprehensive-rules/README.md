# Comprehensive rules translations

Traditional Chinese translations of the official Japanese comprehensive rules shown on the
website's rules page. Files live at `<language>/<version>/<file>.json`, one per original file in
`authored/rules/comprehensive/jp/<version>/`, with the same file name. A file may cover only part
of its chapter; untranslated nodes show the Japanese original marked as in preparation.

```json
{
  "format": 1,
  "kind": "comprehensive-rules-translation",
  "version": "1.28.0",
  "language": "zh-TW",
  "entries": {
    "1.3.2.2": { "text": "…", "examples": ["…"], "source_sha256": "<64 hex>" },
    "B-1:6.1.1.1": { "text": "…", "source_sha256": "<64 hex>" },
    "A:1": {
      "title": "…",
      "table": { "header": ["…", null], "rows": [["…", null]] },
      "source_sha256": "<64 hex>"
    }
  }
}
```

- Entry keys are the original node `key`: the body number itself, or `<context>:<number>` in
  appendices, where numbers repeat body numbers.
- `title`, `text`, `examples` and `table` mirror the original node. `examples` and `table` keep
  the original's exact shape; `null` marks one untranslated example or cell, which falls back to
  the original on its own.
- Keep rule references such as `（10.5.1）` exactly as in the original so they stay linked.
- `source_sha256` pins the original this entry was translated from. Do not write it by hand: run
  `bun run rules:stamp <file>…` from `sim/web`. It fills missing hashes only and lists entries whose
  original has changed since; retranslate those, delete their hash, and stamp again.

The data test in `sim/web/src/data/rules/` rejects unknown keys, missing or stale hashes and shape
mismatches. Our translation contributions are CC0-1.0; the official rules text is excluded, see
[LICENSING.md](../../../LICENSING.md).
