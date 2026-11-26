import type { TFunction } from "i18next"

import { parseMechanicKey } from "../../domain/mechanics"
import type { MechanicState } from "../../domain/query/model"

/** Chip and summary wording of one keyword condition; a held keyword keeps the plain "keyword" wording. */
export function mechanicConditionLabel(
  t: TFunction,
  key: string,
  state: MechanicState,
  keywordLabel: (id: string) => string,
): string {
  const condition = parseMechanicKey(key)
  if (!condition) return key
  const label = keywordLabel(condition.keywordId)
  if (condition.relation === "has")
    return t(state === "has" ? "filters.keywordHas" : "filters.keywordNot", { label })
  return t(state === "has" ? "filters.keywordRelationHas" : "filters.keywordRelationNot", {
    relation: t(`filters.keywordRelations.${condition.relation}`),
    label,
  })
}
