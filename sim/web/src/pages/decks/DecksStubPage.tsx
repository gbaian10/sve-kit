import { useTranslation } from "react-i18next"

import { PageTitle } from "../PageTitle"

// The bottom bar keeps its third cell so the layout is final; the deck builder itself is later.
export function DecksStubPage() {
  const { t } = useTranslation()
  return (
    <>
      <PageTitle>{t("pages.decks")}</PageTitle>
      <p className="text-15 text-text-2">{t("pages.decksStub")}</p>
    </>
  )
}
