import { useTranslation } from "react-i18next"

import { Badge } from "../ui/Badge"

/** Marks a printing that exists on paper but that the official card database does not list. */
export function UnlistedBadge({ className }: { readonly className?: string }) {
  const { t } = useTranslation()
  return (
    <span title={t("cardPage.unlistedHint")} className={className}>
      <Badge tone="info">{t("cardPage.unlistedBadge")}</Badge>
    </span>
  )
}
