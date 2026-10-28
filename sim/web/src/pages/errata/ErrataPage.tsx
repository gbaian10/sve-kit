import { useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { Link } from "react-router"

import {
  useActiveSnapshot,
  useListView,
  useRouteEdition,
  useSearchReadiness,
} from "../../app/snapshot"
import { arrayValue, objectValue, stringValue } from "../../data"
import { printingPath } from "../../domain/cardRoute"
import { displayName } from "../../domain/nameDisplay"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { currentUiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"

interface NoticeInfo {
  readonly url: string
  readonly date: string
  readonly title: string
}

export function ErrataPage() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const edition = useRouteEdition()
  const { client } = useActiveSnapshot()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const readiness = useSearchReadiness(edition)
  const ready = readiness.phase === "ready" && readiness.edition === edition
  const query = {
    edition,
    state: { ...DEFAULT_QUERY, errataOnly: true },
  }
  const {
    view: listView,
    imageRoot,
    images,
    loading: viewLoading,
  } = useListView(query, ready ? 1 : 0)
  const snapshot = client.snapshot()
  const [notices, setNotices] = useState<ReadonlyMap<string, NoticeInfo>>(new Map())
  useEffect(() => {
    if (!snapshot) return
    let active = true
    const keys = [...snapshot.files]
      .filter(([, file]) =>
        arrayValue(file["row_counts"]).some((count) => arrayValue(count)[0] === "errata"),
      )
      .map(([key]) => key)
    void Promise.all(keys.map((key) => client.fragments(key)))
      .then((groups) => {
        const byPrinting = new Map<string, NoticeInfo>()
        for (const notice of groups
          .flat()
          .filter((fragment) => fragment.table === "errata")
          .flatMap((fragment) => fragment.rows)) {
          if (notice["region"] !== edition) continue
          const info = {
            url: stringValue(notice["official_url"]),
            date: stringValue(
              arrayValue(notice["versions"]).map((v) => objectValue(v))[0]?.["announced_on"],
            ),
            title: stringValue(notice["title"]),
          }
          for (const version of arrayValue(notice["versions"]).map((v) => objectValue(v))) {
            for (const printing of arrayValue(version["printings"]).map((v) => objectValue(v))) {
              const id = stringValue(printing["printing_id"])
              if (id) byPrinting.set(id, info)
            }
          }
        }
        if (active) setNotices(byPrinting)
      })
      .catch(() => {
        if (active) setNotices(new Map())
      })
    return () => {
      active = false
    }
  }, [client, edition, snapshot])
  const rows = useMemo(
    () => listView?.page.items.flatMap((item) => (item.summary ? [item.summary] : [])) ?? [],
    [listView],
  )
  return (
    <section className="mx-auto max-w-6xl py-8 lg:py-12" aria-labelledby="errata-heading">
      <header className="mb-7 border-b border-border pb-6">
        <p className="mb-2 text-12 font-semibold tracking-[0.16em] text-accent-text uppercase">
          {t("nav.errata")}
        </p>
        <h1 id="errata-heading" className="text-24 font-bold tracking-tight text-text-1">
          {t("errata.listTitle")}
        </h1>
        <p className="mt-2 max-w-2xl text-14 text-text-2">{t("errata.listDescription")}</p>
      </header>
      {rows.length > 0 ? (
        <div ref={imageRoot}>
          <ul className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {rows.map((card) => {
              const notice = notices.get(card.printingId)
              return (
                <li key={card.printingId}>
                  <article className="flex h-full gap-4 rounded-card border border-border bg-surface-1 p-4 shadow-sm transition hover:border-accent/50 hover:bg-surface-2">
                    {images?.cardImage(card.printingId, card.faceId) ? (
                      <img
                        src={images.cardImage(card.printingId, card.faceId)?.src}
                        alt=""
                        loading="lazy"
                        className="h-24 w-16 shrink-0 rounded-card border border-border object-cover"
                      />
                    ) : (
                      <div
                        aria-hidden="true"
                        className="flex h-24 w-16 shrink-0 items-center justify-center rounded-card border border-border bg-gradient-to-br from-accent-soft via-surface-2 to-surface-1 text-12 font-bold text-accent-text"
                      >
                        SVE
                      </div>
                    )}
                    <div className="min-w-0 flex-1">
                      <Link
                        to={printingPath(card)}
                        className="group block rounded-button focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
                      >
                        <span className="text-12 font-semibold tracking-wide text-text-3">
                          {card.cardNo}
                        </span>
                        <span className="mt-1 block text-16 font-semibold text-text-1 group-hover:text-accent-text">
                          {displayName(card.name, uiLanguage, prefs.nameDisplay).primary.text}
                        </span>
                      </Link>
                      <p className="mt-2 line-clamp-2 min-h-5 text-13 text-text-2">
                        {notice?.date || notice?.title || t("errata.noticeDetailsUnavailable")}
                      </p>
                      {notice?.url && (
                        <a
                          href={notice.url}
                          target="_blank"
                          rel="noreferrer"
                          className="mt-2 inline-flex text-13 text-accent-text underline-offset-2 hover:underline"
                        >
                          {t("errata.official")}
                        </a>
                      )}
                    </div>
                  </article>
                </li>
              )
            })}
          </ul>
        </div>
      ) : (
        <p className="rounded-card border border-border bg-surface-1 p-6 text-14 text-text-2">
          {ready && !viewLoading ? t("errata.listEmpty") : t("errata.listLoading")}
        </p>
      )}
    </section>
  )
}
