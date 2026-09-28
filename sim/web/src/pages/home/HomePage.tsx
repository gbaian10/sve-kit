import { useTranslation } from "react-i18next"

import { PageTitle } from "../PageTitle"

export function HomePage() {
  const { t } = useTranslation()
  return <PageTitle>{t("pages.home")}</PageTitle>
}
