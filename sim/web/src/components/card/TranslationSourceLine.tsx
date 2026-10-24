import { useTranslation } from "react-i18next"

import type { SelectedText } from "../../data"

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
  if (!value.translated) return null
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
      </p>
      {proofreading && value.translation?.["low_confidence"] === true && (
        <p className="text-13">{t("card.translationProofreading")}</p>
      )}
    </>
  )
}
