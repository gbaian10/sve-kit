import { canonical } from "../format-v3/json"
import { digest } from "../format-v3/sha256"
import type { RuleNode, RulesTranslationFile, TranslationEntry } from "./model"

/**
 * Everything a translator translates, with fixed defaults, so a change to any of it (a title, one
 * example, one table cell) makes the stored hash stale. This is the only implementation: the data
 * test and `bun run rules:stamp` both call it.
 */
export function sourceProjection(node: RuleNode) {
  return {
    title: node.title ?? "",
    text: node.text,
    examples: [...(node.examples ?? [])],
    table: node.table
      ? { header: [...node.table.header], rows: node.table.rows.map((row) => [...row]) }
      : null,
  }
}

/** 64 lowercase hex digits of SHA-256 over the canonical-json-v1 bytes of the projection. */
export function sourceHash(node: RuleNode): string {
  return digest(canonical(sourceProjection(node))).slice("sha256:".length)
}

export type TranslationProblem =
  | { readonly key: string; readonly kind: "unknown-key" }
  | { readonly key: string; readonly kind: "missing-hash" }
  | { readonly key: string; readonly kind: "stale" }
  | { readonly key: string; readonly kind: "shape"; readonly detail: string }

function shapeProblem(node: RuleNode, entry: TranslationEntry): string | undefined {
  if (entry.title !== undefined && node.title === undefined) return "title has no original"
  const examples = node.examples?.length ?? 0
  if (entry.examples && entry.examples.length !== examples)
    return `examples ${String(entry.examples.length)} != ${String(examples)}`
  if (entry.table) {
    if (!node.table) return "table has no original"
    if (entry.table.header.length !== node.table.header.length) return "table header width"
    if (entry.table.rows.length !== node.table.rows.length) return "table row count"
    const row = entry.table.rows.findIndex(
      (cells, index) => cells.length !== node.table?.rows[index]?.length,
    )
    if (row >= 0) return `table row ${String(row)} width`
  }
  return undefined
}

/** Every way a translation file can disagree with the original nodes, in object key order. */
export function translationProblems(
  nodes: readonly RuleNode[],
  file: RulesTranslationFile,
): TranslationProblem[] {
  const byKey = new Map(nodes.map((node) => [node.key, node]))
  const problems: TranslationProblem[] = []
  for (const [key, entry] of Object.entries(file.entries)) {
    const node = byKey.get(key)
    if (!node) {
      problems.push({ key, kind: "unknown-key" })
      continue
    }
    const detail = shapeProblem(node, entry)
    if (detail) problems.push({ key, kind: "shape", detail })
    if (!entry.source_sha256) problems.push({ key, kind: "missing-hash" })
    else if (entry.source_sha256 !== sourceHash(node)) problems.push({ key, kind: "stale" })
  }
  return problems
}

/**
 * Fills only missing hashes with the current original's. An existing hash is never rewritten:
 * a mismatch means the original changed after translation, and overwriting would hide that.
 */
export function stampTranslation(
  nodes: readonly RuleNode[],
  file: RulesTranslationFile,
): { readonly file: RulesTranslationFile; readonly stamped: string[]; readonly stale: string[] } {
  const byKey = new Map(nodes.map((node) => [node.key, node]))
  const stamped: string[] = []
  const stale: string[] = []
  const entries: Record<string, TranslationEntry> = {}
  for (const [key, entry] of Object.entries(file.entries)) {
    const node = byKey.get(key)
    if (node && !entry.source_sha256) {
      entries[key] = { ...entry, source_sha256: sourceHash(node) }
      stamped.push(key)
      continue
    }
    if (node && entry.source_sha256 !== sourceHash(node)) stale.push(key)
    entries[key] = entry
  }
  return { file: { ...file, entries }, stamped, stale }
}
