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

## Updating a translation

After [parsing and comparing the new Japanese rules][rules], use the old source and translation
directories to create the new translation directory. Run these commands from the repository root
and use absolute paths for the rules and translation directories.

```bash
bun run --cwd sim/web rules:revision carry \
  "$old_rules" "$new_rules" "$old_translation" "$new_translation" > pending.json
```

Read the full new rules and refer to the previous translation to keep terminology consistent.
For modified nodes, use the old entry as a starting point where helpful. Retrieve it from the
previous translation directory or Git history.
Translate the `pending` nodes listed in the report, which includes added nodes and modified
nodes. Existing entries for unchanged nodes are copied with their original `source_sha256`.
`previouslyUntranslated` lists unchanged nodes that had no entry in the old version. If a
particular unchanged node warrants a translation correction, explain that correction individually
in the PR.

Put the new rules and translation files under their version directories before stamping, so
`rules:stamp` can locate the source by version and file name.

```bash
bun run --cwd sim/web rules:stamp "$new_translation"/*.json
bun run --cwd sim/web test:all src/data/rules scripts/rules/revision.test.ts
```

[rules]: ../../rules/comprehensive/README.md#updating-an-edition
