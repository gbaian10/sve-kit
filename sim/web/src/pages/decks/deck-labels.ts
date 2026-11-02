import { useMemo } from "react"
import { useTranslation } from "react-i18next"

import { useCardSnapshot, useRouteEdition, useSetOptions } from "../../app/snapshot"
import { quickBarClasses } from "../../domain/classes"
import type { DeckBasis } from "../../domain/deck/types"
import { UI_TEXT_LANG } from "../../domain/nameDisplay"
import { NEUTRAL_CLASS } from "../../domain/query/model"
import { currentUiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"
import { titleLabel, type TitleOption, titleOptions } from "./deck-titles"

export interface ClassOption {
  readonly code: string
  readonly label: string
}

/** The names a deck's basis is shown with: the six classes plus neutral, and the titles. */
export function useDeckLabels(): {
  readonly classOptions: readonly ClassOption[] | undefined
  readonly titles: readonly TitleOption[] | undefined
  readonly titleName: (code: string) => string
  readonly basisLabel: (basis: DeckBasis) => string
} {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const textLang = UI_TEXT_LANG[currentUiLanguage(prefs.uiLanguage)]
  const { classes } = useSetOptions()
  const { client, catalog } = useCardSnapshot()
  const edition = useRouteEdition()
  // After an edition change the previous region's catalog stays until the new one has loaded.
  const loadedRegion = client.snapshot()?.region
  const current =
    catalog && (loadedRegion === undefined || loadedRegion === edition) ? catalog : null
  const classOptions = useMemo(
    () =>
      classes &&
      quickBarClasses(classes.map((item) => item.code)).map((code) => {
        const option = classes.find((item) => item.code === code)
        return {
          code,
          label:
            code === NEUTRAL_CLASS
              ? t("search.neutral")
              : (option?.name.translations[textLang] ?? option?.name.original.text ?? code),
        }
      }),
    [classes, textLang, t],
  )
  const titles = useMemo(
    () => (current ? titleOptions(current, edition) : undefined),
    [current, edition],
  )
  const titleName = (code: string) => (catalog ? titleLabel(catalog, code, textLang) : code)
  const basisLabel = (basis: DeckBasis) =>
    basis.kind === "class"
      ? (classOptions?.find((option) => option.code === basis.classCode)?.label ??
        (basis.classCode === NEUTRAL_CLASS ? t("search.neutral") : basis.classCode))
      : t("decks.basis.titleNamed", { title: titleName(basis.titleCode) })
  return { classOptions, titles, titleName, basisLabel }
}
