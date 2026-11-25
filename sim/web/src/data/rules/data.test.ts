// @vitest-environment node
import { describe, expect, it } from "vitest"

import type { RuleNode, RulesFile, RulesIndex, RulesTranslationFile } from "./model"
import { findRuleReferences, resolveRuleReference } from "./references"
import { translationProblems } from "./source-hash"

// The committed comprehensive rules of every edition (authored/rules/comprehensive/<region>/) and
// every translation of them. Reports keys and numbers only, never rules wording.
const originals = import.meta.glob<RulesFile | RulesIndex>(
  "../../../../../authored/rules/comprehensive/*/*/*.json",
  { import: "default", eager: true },
)
const translations = import.meta.glob<RulesTranslationFile>(
  "../../../../../authored/translations/comprehensive-rules/*/*/*.json",
  { import: "default", eager: true },
)
const raw = import.meta.glob<string>("../../../../../authored/rules/comprehensive/*/*/*.json", {
  query: "?raw",
  import: "default",
  eager: true,
})

interface Version {
  readonly index: RulesIndex
  readonly files: ReadonlyMap<string, RulesFile>
  readonly nodes: readonly RuleNode[]
}

/** Keyed `<region>/<version>`: the Japanese and English editions number their versions apart. */
function versions(): Map<string, Version> {
  const byVersion = new Map<string, { index?: RulesIndex; files: Map<string, RulesFile> }>()
  for (const [path, data] of Object.entries(originals)) {
    const [region = "", version = "", file = ""] = path.split("/").slice(-3)
    const id = `${region}/${version}`
    const entry: { index?: RulesIndex; files: Map<string, RulesFile> } = byVersion.get(id) ?? {
      files: new Map(),
    }
    if (data.kind === "comprehensive-rules-index") entry.index = data
    else entry.files.set(file, data)
    byVersion.set(id, entry)
  }
  const result = new Map<string, Version>()
  for (const [version, { index, files }] of byVersion) {
    if (!index) throw new Error(`${version}: index.json missing`)
    const nodes = index.contents.flatMap((item) => files.get(item.file)?.nodes ?? [])
    result.set(version, { index, files, nodes })
  }
  return result
}

const VERSIONS = versions()
const parts = (number: string) => number.split(".").map(Number)
const compare = (a: readonly number[], b: readonly number[]) => {
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const d = (a[i] ?? -1) - (b[i] ?? -1)
    if (d !== 0) return d
  }
  return 0
}

it("has at least the Japanese 1.29.0 and English 1.26.1 rules", () => {
  expect([...VERSIONS.keys()]).toEqual(expect.arrayContaining(["jp/1.29.0", "en/1.26.1"]))
})

describe.each([...VERSIONS])("comprehensive rules %s", (id, { index, files, nodes }) => {
  const [region, version] = id.split("/")
  it("lists exactly its files, each declaring this edition and version, under 1 MiB", () => {
    expect(new Set(index.contents.map((item) => item.file))).toEqual(new Set(files.keys()))
    expect(index.version).toBe(version)
    expect(index.region).toBe(region)
    for (const [file, data] of files) {
      expect(data, file).toMatchObject({ format: 1, kind: "comprehensive-rules", version })
      const text = raw[`../../../../../authored/rules/comprehensive/${id}/${file}`] ?? ""
      expect(new TextEncoder().encode(text).length, file).toBeLessThan(1024 * 1024)
    }
  })

  it("matches the table of contents to each file's first node", () => {
    for (const item of index.contents) {
      const first = files.get(item.file)?.nodes[0]
      expect(first?.key, item.file).toBe(item.key)
      const title = first?.number ? `${first.number}. ${first.title ?? ""}` : first?.title
      expect(title, item.file).toBe(item.title)
    }
  })

  it("gives every node a unique key", () => {
    const keys = nodes.map((node) => node.key)
    expect(keys.filter((key, i) => keys.indexOf(key) !== i)).toEqual([])
  })

  it("numbers the body without gaps: each clause is a first child or the next sibling", () => {
    const body = nodes.filter((node) => node.context === "main")
    const seen = new Set<string>()
    const broken: string[] = []
    let previous: number[] = []
    for (const node of body) {
      const number = node.number ?? ""
      const p = parts(number)
      const parent = p.slice(0, -1).join(".")
      const last = p.at(-1) ?? 0
      const ok =
        node.key === number &&
        node.depth === p.length &&
        node.kind === (p.length === 1 ? "chapter" : p.length === 2 ? "section" : "rule") &&
        (parent === "" || seen.has(parent)) &&
        (last === 1
          ? compare(p.slice(0, -1), previous.slice(0, p.length - 1)) === 0
          : seen.has([...p.slice(0, -1), last - 1].join("."))) &&
        compare(p, previous) > 0
      if (!ok) broken.push(number)
      seen.add(number)
      previous = p
    }
    expect(broken).toEqual([])
    expect(body.filter((node) => node.depth === 1)).toHaveLength(15)
  })

  it("keeps appendix numbering unique and ascending per context, allowing gaps", () => {
    const broken: string[] = []
    const last = new Map<string, number[]>()
    for (const node of nodes) {
      if (node.context === "main") continue
      expect(node.key.startsWith(node.context), node.key).toBe(true)
      if (!node.number) continue
      expect(node.key).toBe(`${node.context}:${node.number}`)
      const p = parts(node.number)
      const before = last.get(node.context)
      if (before && compare(p, before) <= 0) broken.push(node.key)
      last.set(node.context, p)
    }
    expect(broken).toEqual([])
  })

  it("keeps examples non-empty and every table row as wide as its header", () => {
    for (const node of nodes) {
      for (const example of node.examples ?? []) expect(example, node.key).not.toBe("")
      if (node.table)
        for (const row of node.table.rows)
          expect(row, node.key).toHaveLength(node.table.header.length)
    }
  })

  it("resolves every cited clause number", () => {
    const keys = new Set(nodes.map((node) => node.key))
    const unresolved: string[] = []
    let count = 0
    for (const node of nodes) {
      const texts = [node.title ?? "", node.text, ...(node.examples ?? [])]
      for (const text of texts)
        for (const ref of findRuleReferences(text)) {
          count++
          if (!resolveRuleReference(ref.number, node.context, keys))
            unresolved.push(`${node.key}→${ref.number}`)
        }
    }
    expect(unresolved).toEqual([])
    expect(count).toBeGreaterThan(0)
  })
})

// One test over all files: translation files arrive chapter by chapter, and none may exist yet.
it("matches every translation file to its original", () => {
  const problems: string[] = []
  for (const [path, file] of Object.entries(translations)) {
    const [language = "", version = "", name = ""] = path.split("/").slice(-3)
    const where = `${language}/${version}/${name}`
    // Translations translate the Japanese edition.
    const original = VERSIONS.get(`jp/${version}`)?.files.get(name)
    if (!original) {
      problems.push(`${where}: no original`)
      continue
    }
    // The JSON type is only our claim about the file; check the envelope it actually has.
    const envelope: Readonly<Record<string, unknown>> = { ...file }
    if (envelope["format"] !== 1 || envelope["kind"] !== "comprehensive-rules-translation")
      problems.push(`${where}: envelope`)
    if (file.version !== version || file.language !== language) problems.push(`${where}: path`)
    for (const problem of translationProblems(original.nodes, file))
      problems.push(`${where}: ${problem.kind} ${problem.key}`)
  }
  expect(problems).toEqual([])
})

describe("findRuleReferences", () => {
  it("finds cited numbers in parentheses, in sentences and at both ends of ranges", () => {
    const text =
      "ア（10.5.1）イ（3.2.2、5.14.1）ウ（5.16.1.2を適用した後）詳しくは10.5.2を参照。7.2から7.4、10.9.1.2-10.9.1.4。"
    expect(
      findRuleReferences(text).map((ref) => [ref.number, text.slice(ref.start, ref.end)]),
    ).toEqual(
      ["10.5.1", "3.2.2", "5.14.1", "5.16.1.2", "10.5.2", "7.2", "7.4", "10.9.1.2", "10.9.1.4"].map(
        (number) => [number, number],
      ),
    )
  })

  it("skips numbers without a dot, decimals inside longer numbers and version labels", () => {
    expect(findRuleReferences("（11章）（計6枚）2026年9月28日 ver 1.28.0、ver.1.27.0")).toEqual([])
  })

  it("resolves inside appendix B to the restated clause first, then the body", () => {
    const keys = new Set(["6.1.1.1", "8.3.2", "B-1:6.1.1.1"])
    expect(resolveRuleReference("6.1.1.1", "B-1", keys)).toBe("B-1:6.1.1.1")
    expect(resolveRuleReference("8.3.2", "B-1", keys)).toBe("8.3.2")
    expect(resolveRuleReference("6.1.1.1", "main", keys)).toBe("6.1.1.1")
    expect(resolveRuleReference("6.1.1.1", "B-2", keys)).toBe("6.1.1.1")
    expect(resolveRuleReference("9.9.9", "main", keys)).toBeUndefined()
  })
})
