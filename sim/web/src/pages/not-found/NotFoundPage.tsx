import { useTranslation } from "react-i18next"

import { Link } from "../../components/ui/Link"
import { PageTitle } from "../PageTitle"

export function NotFoundPage() {
  const { t } = useTranslation()
  return (
    <>
      <PageTitle>{t("pages.notFound")}</PageTitle>
      <p className="text-15 text-text-2">{t("pages.notFoundHint")}</p>
      <p className="mt-4">
        <Link to="/cards">{t("nav.cards")}</Link>
      </p>
    </>
  )
}
