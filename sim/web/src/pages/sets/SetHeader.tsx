import { ChevronLeft } from "lucide-react"
import { useTranslation } from "react-i18next"

import { useSetCounts, useSetReleases } from "../../app/snapshot"
import { Link } from "../../components/ui/Link"
import type { NameSource } from "../../domain/nameDisplay"
import { editionUrl } from "../../domain/query/codec"
import type { Region, TextLang } from "../../domain/search"
import { setCaption, setKindKey } from "../../domain/sets"

export interface SetHeaderSet {
  readonly id: string
  /** `product_family.code`, the `set=` value. */
  readonly code: string
  readonly publicCode?: string
  readonly kind?: string
  readonly name: NameSource
}

// The heading is the archive family's own name (architecture §2.1); the product's release date is
// a supplement from the edition's first dated product and never decides what the page lists.
export function SetHeader({
  set,
  edition,
  textLang,
  locale,
}: {
  readonly set: SetHeaderSet
  readonly edition: Region
  readonly textLang: TextLang
  readonly locale: string
}) {
  const { t } = useTranslation()
  const releases = useSetReleases(edition)
  const release =
    releases.result.phase === "complete" && releases.lastInput?.edition === edition
      ? releases.result.releases?.find((item) => item.id === set.id)
      : undefined
  const counted = useSetCounts(set.code, edition)
  const counts =
    counted.result.phase === "complete" &&
    counted.result.counts &&
    counted.lastInput?.state.sets[0] === set.code &&
    counted.lastInput.edition === edition
      ? counted.result.counts
      : undefined
  const { code, name } = setCaption(set, textLang)
  const separator = t("cardPage.separator")
  return (
    <header className="flex flex-col gap-1">
      <Link
        to={editionUrl("/sets", edition)}
        className="-ml-1 inline-flex h-8 items-center gap-0.5 self-start text-13 font-semibold"
      >
        <ChevronLeft className="size-4" aria-hidden="true" />
        <span>{t("home.sets.all")}</span>
      </Link>
      <p className="text-12 text-text-3">
        <span>{t(`sets.kind.${setKindKey(set.kind)}`)}</span>
        {code !== null && (
          <>
            {separator}
            <span className="font-semibold tabular-nums">{code}</span>
          </>
        )}
        {release !== undefined && (
          <>
            {separator}
            <span className="tabular-nums">
              {t("home.sets.released", { date: release.releasedOn })}
            </span>
          </>
        )}
      </p>
      <h1 lang={name.lang} className="text-20 font-bold">
        {name.text}
      </h1>
      <p role="status" className="text-13 text-text-2 tabular-nums">
        {counts
          ? t("sets.cards", { value: new Intl.NumberFormat(locale).format(counts.cards) })
          : counted.result.phase === "error"
            ? t("sets.countsUnavailable")
            : t("sets.countsPending")}
      </p>
    </header>
  )
}
