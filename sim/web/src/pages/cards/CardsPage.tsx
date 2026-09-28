import { useTranslation } from "react-i18next"

import { PageTitle } from "../PageTitle"

export function CardsPage() {
  const { t } = useTranslation()
  return <PageTitle>{t("pages.cards")}</PageTitle>
}
