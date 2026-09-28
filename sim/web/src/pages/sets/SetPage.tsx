import { useTranslation } from "react-i18next"
import { useParams } from "react-router"

import { PageTitle } from "../PageTitle"

export function SetPage() {
  const { t } = useTranslation()
  const { code } = useParams()
  return (
    <PageTitle>
      {t("pages.sets")}
      <span className="ml-2 font-mono text-16 text-text-2">{code}</span>
    </PageTitle>
  )
}
