import type { TFunction } from "i18next"

import { parseMechanicKey } from "../../domain/mechanics"

/** Chip and summary wording of one keyword condition; a held keyword keeps the plain "keyword" wording. */
export function mechanicConditionLabel(
  t: TFunction,
  key: string,
  keywordLabel: (id: string) => string,
): string {
  const condition = parseMechanicKey(key)
  if (!condition) return key
  const label = keywordLabel(condition.keywordId)
  return condition.relation === "has"
    ? t("filters.keywordHas", { label })
    : t("filters.keywordRelationHas", {
        relation: t(`filters.keywordRelations.${condition.relation}`),
        label,
      })
}
