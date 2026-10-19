import { useMemo } from "react"
import { useTranslation } from "react-i18next"

import { textLanguage } from "../card/text-language"

export function useResultLabels() {
  const { t, i18n } = useTranslation()
  const language = textLanguage(i18n.resolvedLanguage)
  return useMemo(
    () => ({
      t,
      language,
      noTranslation: t("card.noTranslation"),
      translationProofreading: t("card.translationProofreading"),
      jpSource: t("card.jpSource"),
      wordingPending: t("card.wordingPending"),
      effectUnavailable: t("results.effectUnavailable"),
      effectPending: t("results.effectPending"),
      noValue: t("cardPage.noValue"),
      stats: {
        cost: t("cardPage.stats.cost"),
        attack: t("cardPage.stats.attack"),
        defense: t("cardPage.stats.defense"),
      },
    }),
    [t, language],
  )
}

export type ResultLabels = ReturnType<typeof useResultLabels>
