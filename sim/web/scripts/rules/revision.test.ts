// @vitest-environment node
import { execFileSync } from "node:child_process"
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"
import { fileURLToPath } from "node:url"

import { expect, it } from "vitest"

import type {
  RuleNode,
  RulesFile,
  RulesIndex,
  RulesTranslationFile,
} from "../../src/data/rules/model"
import { sourceHash } from "../../src/data/rules/source-hash"

const SCRIPT = fileURLToPath(new URL("revision.ts", import.meta.url))

function writeJson(file: string, value: unknown) {
  writeFileSync(file, JSON.stringify(value), "utf8")
}

function writeVersion(directory: string, version: string, nodes: readonly RuleNode[]) {
  mkdirSync(directory)
  const file = "ch01.json"
  const index: RulesIndex = {
    format: 1,
    kind: "comprehensive-rules-index",
    version,
    region: "jp",
    last_updated: "2026-01-01",
    pages: 1,
    page_url: "https://example.com/",
    source_url: "https://example.com/sample.pdf",
    raw_sha256: "0".repeat(64),
    contents: [{ file, key: "1", title: "1. Game" }],
  }
  const chapter: RulesFile = { format: 1, kind: "comprehensive-rules", version, nodes }
  writeJson(path.join(directory, "index.json"), index)
  writeJson(path.join(directory, file), chapter)
}

function rule(key: string): RuleNode {
  return { key, context: "main", kind: "rule", depth: 3, text: "Sample" }
}

it("diffs translated content and carries complete entries with their original hashes", () => {
  const directory = mkdtempSync(path.join(tmpdir(), "rules-revision-"))
  try {
    const before = path.join(directory, "before")
    const after = path.join(directory, "after")
    const oldTranslation = path.join(directory, "old-translation")
    const output = path.join(directory, "new-translation")
    const carried = rule("1.1.1")
    const edited = rule("1.1.2")
    const removed = rule("1.1.3")
    const added = rule("1.1.4")
    const missing = rule("1.1.5")
    writeVersion(before, "1.0.0", [carried, edited, removed, missing])
    writeVersion(after, "1.1.0", [
      { ...carried, changed: true },
      {
        ...edited,
        text: "Revision",
        title: "Heading",
        examples: ["Example"],
        table: { header: ["Name"], rows: [["New"]] },
      },
      added,
      missing,
    ])
    mkdirSync(oldTranslation)
    const entry = { text: "譯文", source_sha256: sourceHash(carried) }
    const old: RulesTranslationFile = {
      format: 1,
      kind: "comprehensive-rules-translation",
      version: "1.0.0",
      language: "zh-TW",
      entries: {
        [carried.key]: entry,
        [edited.key]: { text: "舊譯文", source_sha256: sourceHash(edited) },
        [removed.key]: { text: "已刪除", source_sha256: sourceHash(removed) },
      },
    }
    writeJson(path.join(oldTranslation, "ch01.json"), old)
    const report = JSON.parse(
      execFileSync("bun", [SCRIPT, "diff", before, after, "--json"], { encoding: "utf8" }),
    ) as unknown
    const modified = {
      key: edited.key,
      file: "ch01.json",
      status: "modified",
      fields: ["title", "text", "examples", "table"],
      tableRows: [{ name: "New", status: "added" }],
    }
    const addition = { key: added.key, file: "ch01.json", status: "added" }
    expect(report).toEqual({
      region: "jp",
      from: "1.0.0",
      to: "1.1.0",
      changes: [
        { key: carried.key, file: "ch01.json", status: "unchanged" },
        modified,
        addition,
        { key: missing.key, file: "ch01.json", status: "unchanged" },
        { key: removed.key, file: "ch01.json", status: "deleted" },
      ],
    })
    const result = JSON.parse(
      execFileSync("bun", [SCRIPT, "carry", before, after, oldTranslation, output], {
        encoding: "utf8",
      }),
    ) as unknown
    expect(result).toEqual({
      region: "jp",
      from: "1.0.0",
      to: "1.1.0",
      carried: 1,
      pending: [modified, addition],
      previouslyUntranslated: [{ key: missing.key, file: "ch01.json" }],
    })
    expect(JSON.parse(readFileSync(path.join(output, "ch01.json"), "utf8"))).toEqual({
      ...old,
      version: "1.1.0",
      entries: { [carried.key]: entry },
    })
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})
