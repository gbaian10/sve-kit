import { useTranslation } from "react-i18next"

import type { CardRestriction } from "../../data"
import { copyLimit } from "../../domain/copyLimit"
import { Badge } from "../ui/Badge"

export function RestrictionBadge({
  restrictions,
  className,
}: {
  readonly restrictions: readonly CardRestriction[] | undefined
  readonly className?: string
}) {
  const { t } = useTranslation()
  const count = copyLimit(restrictions)
  if (count === undefined) return null
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
