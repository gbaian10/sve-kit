import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs"
import path from "node:path"

import { canonicalText } from "../../src/data/format-v3/json"
import type {
  RuleNode,
  RulesFile,
  RulesIndex,
  RulesTranslationFile,
  TranslationEntry,
} from "../../src/data/rules/model"
import { sourceProjection, translationProblems } from "../../src/data/rules/source-hash"

interface LocatedNode {
  readonly file: string
  readonly node: RuleNode
}

interface Version {
  readonly index: RulesIndex
  readonly nodes: ReadonlyMap<string, LocatedNode>
  readonly files: ReadonlyMap<string, RulesFile>
}

function readJson(file: string): unknown {
  return JSON.parse(readFileSync(file, "utf8"))
}

function readVersion(directory: string): Version {
  const index = readJson(path.join(directory, "index.json")) as RulesIndex
  if (index.format !== 1 || index.kind !== "comprehensive-rules-index") {
    throw new Error(`${directory}: invalid rules index`)
  }
  const nodes = new Map<string, LocatedNode>()
  const files = new Map<string, RulesFile>()
  for (const { file } of index.contents) {
    if (!/^(ch\d+|appendix-[a-z])\.json$/.test(file) || files.has(file)) {
      throw new Error(`${directory}: invalid or repeated file ${file}`)
    }
    const data = readJson(path.join(directory, file)) as RulesFile
    if (
      data.format !== 1 ||
      data.kind !== "comprehensive-rules" ||
      data.version !== index.version
    ) {
      throw new Error(`${directory}: invalid rules file ${file}`)
    }
    files.set(file, data)
    for (const node of data.nodes) {
      if (nodes.has(node.key)) throw new Error(`${directory}: duplicate rule ${node.key}`)
      nodes.set(node.key, { file, node })
    }
  }
  return { index, nodes, files }
}

const FIELDS = ["title", "text", "examples", "table"] as const

type Change = {
  readonly key: string
  readonly file: string
} & (
  | { readonly status: "added" | "deleted" | "unchanged" }
  | {
      readonly status: "modified"
      readonly fields: readonly (typeof FIELDS)[number][]
      readonly tableRows?: readonly RowChange[]
    }
)

interface RowChange {
  readonly name: string
  readonly status: "added" | "deleted" | "modified"
}

function compareRows(before: RuleNode["table"], after: RuleNode["table"]): RowChange[] {
  const indexRows = (table: RuleNode["table"]) =>
    new Map((table?.rows ?? []).map((row) => [row[0] ?? "", row]))
  const oldRows = indexRows(before)
  const newRows = indexRows(after)
  const changes: RowChange[] = []
  for (const [name, row] of newRows) {
    const previous = oldRows.get(name)
    if (!previous) changes.push({ name, status: "added" })
    else if (canonicalText([...previous]) !== canonicalText([...row]))
      changes.push({ name, status: "modified" })
  }
  for (const name of oldRows.keys()) {
    if (!newRows.has(name)) changes.push({ name, status: "deleted" })
  }
  return changes
}

function compare(before: Version, after: Version): Change[] {
  const changes: Change[] = []
  for (const [key, { node, file }] of after.nodes) {
    const previous = before.nodes.get(key)
    if (!previous) {
      changes.push({ key, file, status: "added" })
      continue
    }
    const oldSource = sourceProjection(previous.node)
    const newSource = sourceProjection(node)
    const fields = FIELDS.filter(
      (field) => canonicalText(oldSource[field]) !== canonicalText(newSource[field]),
    )
    if (fields.length === 0) changes.push({ key, file, status: "unchanged" })
    else
      changes.push({
        key,
        file,
        status: "modified",
        fields,
        ...(fields.includes("table") && {
          tableRows: compareRows(previous.node.table, node.table),
        }),
      })
  }
  for (const [key, { file }] of before.nodes) {
    if (!after.nodes.has(key)) changes.push({ key, file, status: "deleted" })
  }
  return changes
}

function readTranslations(directory: string, before: Version): Map<string, TranslationEntry> {
  const entries = new Map<string, TranslationEntry>()
  for (const name of readdirSync(directory).filter((file) => file.endsWith(".json"))) {
    const source = before.files.get(name)
    const translation = readJson(path.join(directory, name)) as RulesTranslationFile
    if (
      !source ||
      translation.format !== 1 ||
      translation.kind !== "comprehensive-rules-translation" ||
      translation.version !== before.index.version ||
      translation.language !== "zh-TW"
    ) {
      throw new Error(`${name}: translation does not match the old Japanese version`)
    }
    for (const problem of translationProblems(source.nodes, translation)) {
      throw new Error(`${name}: ${problem.kind} ${problem.key}`)
    }
    for (const [key, entry] of Object.entries(translation.entries)) entries.set(key, entry)
  }
  return entries
}

function carry(
  before: Version,
  after: Version,
  changes: readonly Change[],
  oldTranslation: string,
  output: string,
) {
  if (before.index.region !== "jp")
    throw new Error("Traditional Chinese translations use Japanese originals")
  const entries = readTranslations(oldTranslation, before)
  const nextFiles = new Map<string, Map<string, TranslationEntry>>()
  const pending = []
  const previouslyUntranslated = []
  let carried = 0
  for (const change of changes) {
    const { key, file, status } = change
    if (status === "deleted") continue
    if (status !== "unchanged") {
      pending.push(change)
      continue
    }
    const entry = entries.get(key)
    if (!entry) {
      previouslyUntranslated.push({ key, file })
      continue
    }
    let next = nextFiles.get(file)
    if (!next) {
      next = new Map()
      nextFiles.set(file, next)
    }
    next.set(key, entry)
    carried++
  }
  mkdirSync(output)
  for (const [file, next] of nextFiles) {
    const translation: RulesTranslationFile = {
      format: 1,
      kind: "comprehensive-rules-translation",
      version: after.index.version,
      language: "zh-TW",
      entries: Object.fromEntries(next),
    }
    writeFileSync(path.join(output, file), json(translation), "utf8")
  }
  return { carried, pending, previouslyUntranslated }
}

function json(value: unknown): string {
  return `${JSON.stringify(value, null, 2)}\n`
}

function main(args: readonly string[]) {
  const [command, oldDirectory, newDirectory, ...options] = args
  if (
    !oldDirectory ||
    !newDirectory ||
    !(
      (command === "diff" &&
        (options.length === 0 || (options.length === 1 && options[0] === "--json"))) ||
      (command === "carry" && options.length === 2)
    )
  ) {
    throw new Error(
      "usage: revision.ts diff <old rules> <new rules> [--json]\n       revision.ts carry <old rules> <new rules> <old translation> <new translation>",
    )
  }
  const before = readVersion(oldDirectory)
  const after = readVersion(newDirectory)
  if (before.index.region !== after.index.region || before.index.version === after.index.version) {
    throw new Error("Select two distinct versions of the same edition")
  }
  const changes = compare(before, after)
  const version = {
    region: after.index.region,
    from: before.index.version,
    to: after.index.version,
  }
  const [oldTranslation, output] = options
  if (command === "carry" && oldTranslation && output) {
    process.stdout.write(
      json({ ...version, ...carry(before, after, changes, oldTranslation, output) }),
    )
  } else if (options[0] === "--json") {
    process.stdout.write(json({ ...version, changes }))
  } else {
    process.stdout.write(`${version.region} ${version.from} → ${version.to}\n`)
    for (const status of ["added", "deleted", "modified", "unchanged"]) {
      const group = changes.filter((change) => change.status === status)
      process.stdout.write(`\n${status} (${String(group.length)})\n`)
      for (const change of group) {
        const fields = change.status === "modified" ? ` [${change.fields.join(", ")}]` : ""
        process.stdout.write(`  ${change.key} (${change.file})${fields}\n`)
        if (change.status === "modified") {
          for (const row of change.tableRows ?? [])
            process.stdout.write(`    ${row.status} row: ${row.name}\n`)
        }
      }
    }
  }
}

try {
  main(process.argv.slice(2))
} catch (error) {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`)
  process.exitCode = 1
}
