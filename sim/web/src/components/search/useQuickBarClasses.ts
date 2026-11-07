import { useMemo } from "react"
import { useTranslation } from "react-i18next"

import { useSetOptions } from "../../app/snapshot"
import { quickBarClasses } from "../../domain/classes"
import { NEUTRAL_CLASS } from "../../domain/query/model"
import type { TextLang } from "../../domain/search"
import type { QuickBarClass } from "./ClassQuickBar"

/** The snapshot's classes in quick-bar order, named in `textLang`; undefined while they load. */
export function useQuickBarClasses(textLang: TextLang): readonly QuickBarClass[] | undefined {
  const { t } = useTranslation()
  const { classes } = useSetOptions()
  return useMemo(
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
}
