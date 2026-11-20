import { useContext } from "react"
import { useTranslation } from "react-i18next"

import type { SelectedText } from "../../data"
import { CardReportContext } from "./card-report-context"

/** Origin · authority of the shown translation, plus the proofreading notice when it is low confidence. */
export function TranslationSourceLine({
  value,
  proofreading = true,
}: {
  readonly value: SelectedText
  /** Whether a low-confidence translation also gets the proofreading notice. */
  readonly proofreading?: boolean
}) {
  const { t } = useTranslation()
  const reportTranslation = useContext(CardReportContext)
  if (!value.translated) return null
  const machine =
    value.translation?.["origin"] !== "official" && value.translation?.["origin"] !== "project"
  const origin =
    value.translation?.["origin"] === "official"
      ? t("card.translationOrigin.official")
      : value.translation?.["origin"] === "project"
        ? t("card.translationOrigin.project")
        : t("card.translationOrigin.machine")
  const authority =
    value.translation?.["authority"] === "sve_official"
      ? t("card.translationAuthority.sve_official")
      : value.translation?.["authority"] === "digital_official"
        ? t("card.translationAuthority.digital_official")
        : t("card.translationAuthority.unofficial")
  return (
    <>
      <p className="text-12 text-text-3">
        {origin} · {authority}
        {machine && reportTranslation && (
          <button
            type="button"
            className="ms-2 min-h-8 font-medium text-accent-text underline underline-offset-2"
            onClick={reportTranslation}
          >
            {t("cardReport.translationAction")}
          </button>
        )}
      </p>
      {proofreading && value.translation?.["low_confidence"] === true && (
        <p className="text-13">{t("card.translationProofreading")}</p>
      )}
    </>
  )
}
