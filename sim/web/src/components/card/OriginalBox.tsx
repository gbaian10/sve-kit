import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

/** The framed original-language text under the translation it belongs to (card text, Q&A, errata). */
export function OriginalBox({ children }: { readonly children: ReactNode }) {
  const { t } = useTranslation()
  return (
    <div
      className="rounded-control border border-border px-3 py-2 text-14 leading-effect text-text-2"
      data-original
    >
      <p className="mb-1 text-12 text-text-3">{t("card.originalText")}</p>
      {children}
    </div>
  )
}
