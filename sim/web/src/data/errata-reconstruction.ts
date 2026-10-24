import type { JsonObject } from "./format-v3/json"

type ReconstructionReason =
  | "source_missing"
  | "fragment_missing"
  | "ambiguous"
  | "roundtrip_failed"
  | "unsupported"
  | "multiple_notices"

export interface PrintedWording {
  readonly state: "unknown" | "derived_from_errata"
  readonly reason?: ReconstructionReason
  readonly before?: readonly { readonly text: string; readonly changed: boolean }[]
  readonly after?: string
}

// Notices also spell these JP icons as bare 《…》; card pages encode them as {…}.
function fragment(text: string): string {
  return text
    .replace(/\{《([^{}]+)》\}/gu, "{$1}")
    .replace(/《(ファンファーレ|ラストワード|アクト|起動|攻撃力|体力)》/gu, "{$1}")
}

function occurrences(text: string, part: string): number[] {
  const positions: number[] = []
  if (!part) return positions
  for (let start = text.indexOf(part); start !== -1; start = text.indexOf(part, start + 1))
    positions.push(start)
  return positions
}

interface Replacement {
  readonly start: number
  readonly end: number
  readonly text: string
}

function changedPassage(before: string, after: string): { text: string; changed: boolean }[] {
  // Keep icon markers and Unicode scalars intact when finding unchanged context.
  const tokens = (text: string) => text.match(/\{[^{}]+\}|[^]/gu) ?? []
  const old = tokens(before)
  const current = tokens(after)
  let start = 0
  while (start < old.length && start < current.length && old[start] === current[start]) start++
  let end = 0
  while (
    end < old.length - start &&
    end < current.length - start &&
    old[old.length - end - 1] === current[current.length - end - 1]
  )
    end++
  if (start + end === old.length) return [{ text: before, changed: true }]
  return [
    { text: old.slice(0, start).join(""), changed: false },
    { text: old.slice(start, old.length - end).join(""), changed: true },
    { text: end ? old.slice(-end).join("") : "", changed: false },
  ].filter(({ text }) => text)
}

function apply(text: string, replacements: readonly Replacement[]): string {
  let cursor = 0
  let result = ""
  for (const replacement of replacements) {
    result += text.slice(cursor, replacement.start) + replacement.text
    cursor = replacement.end
  }
  return result + text.slice(cursor)
}

/** Reconstruct an announcement's old effect; a listed printing still has unknown printed text. */
export function reconstructPrintedWording(
  current: string | undefined,
  changes: readonly JsonObject[],
  confirmedPrinting: boolean,
): PrintedWording {
  if (
    !changes.length ||
    changes.some(
      (change) =>
        change["field"] !== "effect" ||
        typeof change["before"] !== "string" ||
        typeof change["after"] !== "string" ||
        !change["before"] ||
        !change["after"],
    )
  )
    return { state: "unknown", reason: "unsupported" }
  if (current === undefined) return { state: "unknown", reason: "source_missing" }
  const pairs = changes.map((change) => ({
    before: fragment(change["before"] as string),
    after: fragment(change["after"] as string),
  }))
  const reverse: Replacement[] = []
  for (const pair of pairs) {
    const positions = occurrences(current, pair.after)
    const [start] = positions
    if (start === undefined) return { state: "unknown", reason: "fragment_missing" }
    if (positions.length !== 1) return { state: "unknown", reason: "ambiguous" }
    reverse.push({
      start,
      end: start + pair.after.length,
      text: pair.before,
    })
  }
  reverse.sort((a, b) => a.start - b.start)
  if (reverse.some((item, index) => item.start < (reverse[index - 1]?.end ?? 0)))
    return { state: "unknown", reason: "ambiguous" }
  const old = apply(current, reverse)
  const forward: Replacement[] = []
  for (const pair of pairs) {
    const positions = occurrences(old, pair.before)
    const [start] = positions
    if (positions.length !== 1 || start === undefined)
      return { state: "unknown", reason: "roundtrip_failed" }
    forward.push({
      start,
      end: start + pair.before.length,
      text: pair.after,
    })
  }
  forward.sort((a, b) => a.start - b.start)
  if (
    forward.some((item, index) => item.start < (forward[index - 1]?.end ?? 0)) ||
    apply(old, forward) !== current
  )
    return { state: "unknown", reason: "roundtrip_failed" }
  const before: { text: string; changed: boolean }[] = []
  let cursor = 0
  for (const item of reverse) {
    if (cursor < item.start)
      before.push({ text: current.slice(cursor, item.start), changed: false })
    before.push(...changedPassage(item.text, current.slice(item.start, item.end)))
    cursor = item.end
  }
  if (cursor < current.length) before.push({ text: current.slice(cursor), changed: false })
  return { state: confirmedPrinting ? "derived_from_errata" : "unknown", before, after: current }
}
