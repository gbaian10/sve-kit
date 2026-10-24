import { useTranslation } from "react-i18next"

import type { CardRestriction } from "../../data"
import { Badge } from "../ui/Badge"

export function RestrictionBadge({
  restrictions,
  className,
}: {
  readonly restrictions: readonly CardRestriction[] | undefined
  readonly className?: string
}) {
  const { t } = useTranslation()
  const limits = (restrictions ?? []).flatMap((entry) =>
    entry.maxCopies === null ? [] : [entry.maxCopies],
  )
  if (!limits.length) return null
  const count = Math.min(...limits)
  return (
    <span
      className={className}
      title={t("restrictions.badgeHint", { date: restrictions?.[0]?.asOf ?? "" })}
    >
      <Badge tone={count === 0 ? "danger" : "warning"}>
        {count === 0 ? t("restrictions.banned") : t("restrictions.shortLimit", { count })}
      </Badge>
    </span>
  )
}
