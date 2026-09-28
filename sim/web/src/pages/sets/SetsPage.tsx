import { useTranslation } from "react-i18next"

import { PageTitle } from "../PageTitle"

export function SetsPage() {
  const { t } = useTranslation()
  return <PageTitle>{t("pages.sets")}</PageTitle>
}
