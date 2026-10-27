import { useTranslation } from "react-i18next"

import { objectValue, type Row } from "../../data"
import { Badge } from "../ui/Badge"

export function PrintingMarks({ printing }: { readonly printing: Row | undefined }) {
  const { t } = useTranslation()
  const rawStamp = printing?.["stamp"]
  const stamp = rawStamp == null ? null : objectValue(rawStamp)
  const serial = printing?.["serial"] === true
  if (!stamp && !serial) return null
  const kind = typeof stamp?.["kind"] === "string" ? stamp["kind"] : null
  const year = typeof stamp?.["year"] === "string" ? stamp["year"] : null
  return (
    <div data-printing-marks="" className="flex min-w-0 flex-wrap gap-1.5 text-13 text-text-2">
      {stamp && (
        <span className="min-w-0 break-words">
          {kind === "anniversary"
            ? t("cardPage.anniversaryStamp")
            : (kind ?? t("cardPage.unknownStamp"))}
          {year ? ` · ${year}` : ""}
        </span>
      )}
      {serial && <Badge>{t("filters.serialOnly")}</Badge>}
    </div>
  )
}
