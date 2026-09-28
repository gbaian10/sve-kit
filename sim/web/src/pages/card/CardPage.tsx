import { useTranslation } from "react-i18next"
import { useParams } from "react-router"

import { PageTitle } from "../PageTitle"

export function CardPage() {
  const { t } = useTranslation()
  const { cardNo, intId } = useParams()
  return (
    <PageTitle>
      {t("pages.card")}
      <span className="ml-2 font-mono text-16 text-text-2">{cardNo ?? intId}</span>
    </PageTitle>
  )
}
