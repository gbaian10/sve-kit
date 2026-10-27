import type { Region } from "./search"

export interface MechanicProjection {
  readonly keywordId: string
  readonly scope: string
  readonly relations: readonly string[]
}
export interface MechanicCoverage {
  readonly scope: string
  readonly hasCompleteKeywordIds?: readonly string[]
  readonly completeAll: boolean
  readonly completeMode: string
  readonly completeKeywordIds: readonly string[]
}

/** Self/has membership; partial or unexamined coverage never establishes absence. */
export function mechanicState(
  keywordId: string,
  edition: Region,
  projections: readonly MechanicProjection[],
  coverage: readonly MechanicCoverage[],
  blocked: boolean,
): "present" | "absent" | "unknown" {
  const override =
    edition === "en" &&
    (projections.some((row) => row.scope === "en_override") ||
      coverage.some((row) => row.scope === "en_override"))
  const scope = override ? "en_override" : "shared"
  if (edition === "en" && !override && blocked) return "unknown"
  if (
    projections.some(
      (row) => row.scope === scope && row.keywordId === keywordId && row.relations.includes("has"),
    )
  )
    return "present"
  const complete = coverage.find((row) => row.scope === scope)
  if (!complete) return "unknown"
  if (complete.hasCompleteKeywordIds?.includes(keywordId)) return "absent"
  const listed = complete.completeKeywordIds.includes(keywordId)
  return complete.completeAll || (complete.completeMode === "exclude" ? !listed : listed)
    ? "absent"
    : "unknown"
}
