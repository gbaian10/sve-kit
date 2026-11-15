import { useTranslation } from "react-i18next"

export function ComprehensiveRulesPage() {
  const { t } = useTranslation()
  return (
    <p className="rounded-card border border-border bg-surface-1 p-6 text-14 text-text-2">
      {t("rules.comprehensivePending")}
    </p>
  )
}
