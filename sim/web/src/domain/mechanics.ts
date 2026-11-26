import type { Region } from "./search"

export const MECHANIC_RELATIONS = ["has", "grants", "refers", "counts"] as const
/** How a card relates to a keyword: holds it, gives it to others, mentions it, or counts it. */
export type MechanicRelation = (typeof MECHANIC_RELATIONS)[number]

export function perRelation<T>(value: T): Record<MechanicRelation, T> {
  return { has: value, grants: value, refers: value, counts: value }
}

/** Keywords the search panel offers, in panel order; the derived relations cover exactly these. */
export const PANEL_KEYWORDS = [
  "kw:assail",
  "kw:intimidate",
  "kw:aura",
  "kw:ward",
  "kw:bane",
  "kw:drain",
  "kw:rush",
  "kw:storm",
] as const

export interface MechanicCondition {
  readonly relation: MechanicRelation
  readonly keywordId: string
}

/** Query and index key of one condition; `has` stays the bare keyword id so old links keep working. */
export function mechanicKey(relation: MechanicRelation, keywordId: string): string {
  return relation === "has" ? keywordId : `${keywordId}.${relation}`
}

/** Undefined for a relation suffix this build does not know. */
export function parseMechanicKey(key: string): MechanicCondition | undefined {
  const dot = key.lastIndexOf(".")
  if (dot < 0) return { relation: "has", keywordId: key }
  const relation = MECHANIC_RELATIONS.find(
    (candidate) => candidate !== "has" && candidate === key.slice(dot + 1),
  )
  return relation && { relation, keywordId: key.slice(0, dot) }
}

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

/** One relation's membership; partial or unexamined coverage never establishes absence. */
export function mechanicState(
  keywordId: string,
  relation: MechanicRelation,
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
      (row) =>
        row.scope === scope && row.keywordId === keywordId && row.relations.includes(relation),
    )
  )
    return "present"
  const complete = coverage.find((row) => row.scope === scope)
  if (!complete) return "unknown"
  // The wording check behind this set only inspects what a card itself holds.
  if (relation === "has" && complete.hasCompleteKeywordIds?.includes(keywordId)) return "absent"
  const listed = complete.completeKeywordIds.includes(keywordId)
  return complete.completeAll || (complete.completeMode === "exclude" ? !listed : listed)
    ? "absent"
    : "unknown"
}
