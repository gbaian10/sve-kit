import type { JsonObject } from "./format-v3/json"
import { type TextDiff, textDiff } from "./text-diff"

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
  readonly before?: string
  readonly after?: string
  readonly diff?: TextDiff
}

// Notices also spell these JP icons as bare 《…》; card pages encode them as {…}.
export function errataFragment(text: string): string {
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
    before: errataFragment(change["before"] as string),
    after: errataFragment(change["after"] as string),
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
  return {
    state: confirmedPrinting ? "derived_from_errata" : "unknown",
    before: old,
    after: current,
    diff: textDiff(old, current),
  }
}
