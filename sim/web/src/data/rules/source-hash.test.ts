import { describe, expect, it } from "vitest"

import { localizeNode } from "./localize"
import type { RuleNode, RulesTranslationFile, TranslationEntry } from "./model"
import { sourceHash, sourceProjection, stampTranslation, translationProblems } from "./source-hash"

// Synthetic rules only; no official wording.
const rule: RuleNode = {
  key: "1.2.3",
  context: "main",
  number: "1.2.3",
  kind: "rule",
  depth: 3,
  text: "本文",
  examples: ["例一", "例二"],
}
const heading: RuleNode = {
  key: "1",
  context: "main",
  number: "1",
  kind: "chapter",
  depth: 1,
  title: "章名",
  text: "",
  examples: [],
}
const table: RuleNode = {
  key: "A:1",
  context: "A",
  kind: "block",
  depth: 1,
  title: "表名",
  text: "",
  examples: [],
  table: {
    header: ["名", "值"],
    rows: [
      ["甲", "1"],
      ["乙", "2"],
    ],
  },
}
const appendix: RuleNode = { ...rule, key: "B-1:1.2.3", context: "B-1", text: "附錄本文" }

const file = (entries: Record<string, TranslationEntry>): RulesTranslationFile => ({
  format: 1,
  kind: "comprehensive-rules-translation",
  version: "0.0.0",
  language: "zh-TW",
  entries,
})

describe("sourceHash", () => {
  it("fills defaults so absent and empty pieces hash alike", () => {
    expect(sourceProjection(rule)).toEqual({
      title: "",
      text: "本文",
      examples: ["例一", "例二"],
      table: null,
    })
    expect(sourceHash(rule)).toMatch(/^[0-9a-f]{64}$/u)
    expect(sourceHash({ ...rule, title: "" })).toBe(sourceHash(rule))
    const { examples: _omitted, ...bare } = heading
    expect(sourceProjection(bare)).toMatchObject({ examples: [] })
    expect(sourceHash(bare)).toBe(sourceHash(heading))
  })

  it("is the same for the same projection whatever the key or numbering", () => {
    expect(sourceHash({ ...rule, key: "B-2:9.9", context: "B-2", depth: 1 })).toBe(sourceHash(rule))
  })

  it.each([
    ["the title", { ...heading, title: "改章名" }, heading],
    ["one of equally many examples", { ...rule, examples: ["例一", "改例二"] }, rule],
    [
      "one table cell",
      {
        ...table,
        table: {
          header: ["名", "值"],
          rows: [
            ["甲", "1"],
            ["乙", "3"],
          ],
        },
      },
      table,
    ],
    [
      "a table header",
      { ...table, table: { header: ["名", "數"], rows: table.table?.rows ?? [] } },
      table,
    ],
  ])("changes when only %s changes", (_label, changed, original) => {
    expect(sourceHash(changed)).not.toBe(sourceHash(original))
  })
})

describe("translationProblems", () => {
  const nodes = [heading, rule, table, appendix]

  it("accepts stamped entries, including appendix keys that reuse a body number", () => {
    const { file: stamped } = stampTranslation(
      nodes,
      file({ "1.2.3": { text: "譯" }, "B-1:1.2.3": { text: "附錄譯" } }),
    )
    expect(translationProblems(nodes, stamped)).toEqual([])
  })

  it("reports unknown keys, missing or stale hashes and shape mismatches", () => {
    const problems = translationProblems(
      nodes,
      file({
        "9.9": { text: "x", source_sha256: sourceHash(rule) },
        "1.2.3": { text: "x" },
        "1": { title: "x", source_sha256: "0".repeat(64) },
        "A:1": {
          table: {
            header: ["名"],
            rows: [
              ["甲", "1"],
              ["乙", "2"],
            ],
          },
          source_sha256: sourceHash(table),
        },
      }),
    )
    // Integer-like keys ("1") enumerate first in JavaScript objects.
    expect(problems).toEqual([
      { key: "1", kind: "stale" },
      { key: "9.9", kind: "unknown-key" },
      { key: "1.2.3", kind: "missing-hash" },
      { key: "A:1", kind: "shape", detail: "table header width" },
    ])
  })

  it("rejects an example count that differs from the original", () => {
    const problems = translationProblems(
      nodes,
      file({ "1.2.3": { examples: ["只有一則"], source_sha256: sourceHash(rule) } }),
    )
    expect(problems).toEqual([{ key: "1.2.3", kind: "shape", detail: "examples 1 != 2" }])
  })
})

describe("stampTranslation", () => {
  it("fills only missing hashes and lists, without rewriting, entries whose original changed", () => {
    const old = "f".repeat(64)
    const {
      file: stamped,
      stamped: filled,
      stale,
    } = stampTranslation(
      [heading, rule],
      file({ "1": { title: "譯章" }, "1.2.3": { text: "譯", source_sha256: old } }),
    )
    expect(filled).toEqual(["1"])
    expect(stale).toEqual(["1.2.3"])
    expect(stamped.entries["1"]?.source_sha256).toBe(sourceHash(heading))
    expect(stamped.entries["1.2.3"]?.source_sha256).toBe(old)
  })
})

describe("localizeNode", () => {
  it("places table translations in their own row and column and falls back per cell", () => {
    const view = localizeNode(table, {
      title: "表",
      table: {
        header: ["名稱", null],
        rows: [
          [null, "壹"],
          ["乙譯", null],
        ],
      },
    })
    expect(view.table?.header).toEqual([
      { text: "名稱", translated: true },
      { text: "值", translated: false },
    ])
    expect(view.table?.rows).toEqual([
      [
        { text: "甲", translated: false },
        { text: "壹", translated: true },
      ],
      [
        { text: "乙譯", translated: true },
        { text: "2", translated: false },
      ],
    ])
    expect(view.pending).toBe(true)
  })

  it("falls back per example and treats an empty original body as complete", () => {
    const view = localizeNode(rule, { text: "譯", examples: [null, "例二譯"] })
    expect(view.examples).toEqual([
      { text: "例一", translated: false },
      { text: "例二譯", translated: true },
    ])
    expect(localizeNode(heading, { title: "譯章" })).toMatchObject({
      title: { text: "譯章", translated: true },
      text: { text: "", translated: true },
      pending: false,
    })
  })

  it("shows the whole original, marked pending, when the entry is missing", () => {
    expect(localizeNode(rule, undefined)).toMatchObject({
      text: { text: "本文", translated: false },
      pending: true,
    })
  })
})
