import { useTranslation } from "react-i18next"

import { QuickSearchForm } from "../../components/search/QuickSearchForm"
import { PageTitle } from "../PageTitle"

export function NotFoundPage({ hint }: { readonly hint?: string } = {}) {
  const { t } = useTranslation()
  return (
    <>
      <PageTitle>{t("pages.notFound")}</PageTitle>
      <p className="text-15 text-text-2">{hint ?? t("pages.notFoundHint")}</p>
      <QuickSearchForm className="mt-4 max-w-120" />
    </>
  )
}
