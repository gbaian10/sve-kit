import type { Region } from "./search"

// Mechanic tri-state (snapshot-format §2/§3.1, architecture §4.6): a keyword is "present" when
// the projection lists it, "absent" only when the card's coverage row says the check was complete
// for that keyword, and "unknown" otherwise. Another keyword being present never implies absence.
export type TriState = "present" | "absent" | "unknown"
export type MechanicScope = "shared" | "en_override"

interface MechanicCoverage {
  readonly completeAll: boolean
  readonly completeMode: "include" | "exclude"
  readonly completeIds: readonly string[]
  readonly partialMode: "include" | "exclude"
  readonly partialIds: readonly string[]
}

export interface MechanicFacts {
  /** Keyword ids the projection lists for this card and scope. */
  readonly present: ReadonlySet<string>
  readonly coverage: MechanicCoverage | undefined
  /** The card has an EN region block in `card_engine_support.region_blocks`. */
  readonly enBlocked: boolean
}

function resolve(
  mode: "include" | "exclude",
  ids: readonly string[],
  universe: ReadonlySet<string>,
): Set<string> {
  if (mode === "include") return new Set(ids.filter((id) => universe.has(id)))
  const listed = new Set(ids)
  return new Set([...universe].filter((id) => !listed.has(id)))
}

export function triState(
  facts: MechanicFacts,
  keywordId: string,
  universe: ReadonlySet<string>,
  region: Region,
  scope: MechanicScope = "shared",
): TriState {
  // An EN block means the shared annotation must not be read as the English card's.
  if (region === "en" && scope === "shared" && facts.enBlocked) return "unknown"
  if (facts.present.has(keywordId)) return "present"
  const coverage = facts.coverage
  if (!coverage) return "unknown"
  const complete = coverage.completeAll
    ? universe
    : resolve(coverage.completeMode, coverage.completeIds, universe)
  if (complete.has(keywordId)) return "absent"
  return "unknown"
}
