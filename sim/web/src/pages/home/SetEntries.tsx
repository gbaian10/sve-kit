import { ChevronRight } from "lucide-react"
import { useId } from "react"
import { useTranslation } from "react-i18next"
import { Link as RouterLink } from "react-router"

import { useRouteEdition } from "../../app/snapshot"
import { Button } from "../../components/ui/Button"
import { Link } from "../../components/ui/Link"
import { editionUrl } from "../../domain/query/codec"
import { setCardsPath } from "../../domain/sets"
import { RECENT_SET_LIMIT, type SetEntry } from "./home-model"

// Only known release dates decide which booster and collaboration sets are newest.
export function SetEntries({
  sets,
  failed = false,
  onRetry,
}: {
  readonly sets: readonly SetEntry[] | null
  readonly failed?: boolean
  readonly onRetry?: () => void
}) {
  const { t } = useTranslation()
  const edition = useRouteEdition()
  const headingId = useId()
  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-2">
      <div className="flex min-h-8 items-center justify-between gap-3">
        <h2 id={headingId} className="text-15 font-semibold">
          {t("home.sets.title")}
        </h2>
        <Link
          to={editionUrl("/sets", edition)}
          className="flex h-8 shrink-0 items-center gap-0.5 text-13 font-semibold"
        >
          <span>{t("home.sets.all")}</span>
          <ChevronRight className="size-4" aria-hidden="true" />
        </Link>
      </div>
      {failed ? (
        <div role="alert" className="text-13 leading-relaxed text-text-2">
          <p>{t("home.sets.failed")}</p>
          {onRetry && (
            <Button className="mt-2 h-9" onClick={onRetry}>
              {t("status.retry")}
            </Button>
          )}
        </div>
      ) : sets === null ? (
        <p role="status" className="text-13 text-text-3">
          {t("home.sets.loading")}
        </p>
      ) : sets.length === 0 ? (
        <p className="text-13 leading-relaxed text-text-3">{t("home.sets.none")}</p>
      ) : (
        <>
          <ul className="flex flex-col gap-2">
            {sets.map((set) => (
              <li key={set.publicCode}>
                <RouterLink
                  to={setCardsPath(set.code, edition)}
                  className="flex min-h-14 items-center gap-3 rounded-button border border-border bg-surface-1 px-3 py-1.5 text-text-1 hover:bg-surface-2"
                >
                  <span className="w-14 shrink-0 font-sans text-14 font-bold tabular-nums">
                    {set.publicCode}
                  </span>
                  <span className="block min-w-0 flex-1 leading-tight">
                    <span lang={set.name.lang} className="block truncate text-14">
                      {set.name.text}
                    </span>
                    <span className="block text-12 text-text-3 tabular-nums">
                      {t("home.sets.released", { date: set.releasedOn })}
                    </span>
                  </span>
                </RouterLink>
              </li>
            ))}
          </ul>
          {sets.length < RECENT_SET_LIMIT && (
            <p className="text-12 leading-relaxed text-text-3">{t("home.sets.undated")}</p>
          )}
        </>
      )}
    </section>
  )
}
