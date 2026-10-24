import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"

import { RestrictionBadge } from "../../components/card/RestrictionBadge"
import { Button } from "../../components/ui/Button"
import { Link } from "../../components/ui/Link"
import {
  type CardRestrictionDetails,
  loadCardRestrictionDetails,
  type SnapshotClient,
} from "../../data"
import type { Region } from "../../domain/search"

export function CardRestrictions({
  client,
  cardId,
  region,
}: {
  readonly client: SnapshotClient
  readonly cardId: string
  readonly region: Region
}) {
  const { t } = useTranslation()
  const snapshot = client.snapshot()
  const [attempt, retry] = useState(0)
  const key = JSON.stringify([snapshot?.manifestHash, cardId, region, attempt])
  const [state, setState] = useState<{
    key: string
    value?: CardRestrictionDetails
    failed?: boolean
  }>()
  useEffect(() => {
    if (!snapshot) return
    let active = true
    void loadCardRestrictionDetails(client, snapshot, cardId, region).then(
      (value) => {
        if (active) setState({ key, value })
      },
      () => {
        if (active) setState({ key, failed: true })
      },
    )
    return () => {
      active = false
    }
  }, [client, snapshot, cardId, region, key])
  if (state?.key !== key) return null
  if (state.failed)
    return (
      <Button
        onClick={() => {
          retry(attempt + 1)
        }}
      >
        {t("restrictions.retry")}
      </Button>
    )
  const value = state.value
  if (!value) return null
  return (
    <section
      aria-label={t("restrictions.heading")}
      className="mb-4 space-y-2 rounded-block border border-border bg-surface-2 px-3 py-3"
    >
      <h3 className="text-14 font-semibold">{t("restrictions.heading")}</h3>
      <p className="text-12 text-text-3">
        {value.status.state === "unknown"
          ? t("restrictions.unknown")
          : t(
              value.status.synthetic
                ? "filters.restrictionsSyntheticDate"
                : "filters.restrictionsDate",
              { date: value.status.onDate },
            )}{" "}
        {value.status.synthetic ? t("filters.restrictionsSynthetic") : ""}
      </p>
      {value.status.state === "partial" && (
        <p className="text-12 text-text-3">{t("filters.restrictionsPartial")}</p>
      )}
      {!value.restrictions.length && value.status.state === "complete" && (
        <p className="text-13">{t("restrictions.notListed")}</p>
      )}
      {value.restrictions.map((entry) => (
        <div key={entry.id} className="flex flex-wrap items-center gap-2 text-13">
          <RestrictionBadge restrictions={[entry]} />
          <span>
            {entry.maxCopies === null
              ? t("restrictions.combination")
              : entry.maxCopies === 0
                ? t("restrictions.banned")
                : t("restrictions.fullLimit", { count: entry.maxCopies })}
          </span>
          <time dateTime={entry.effectiveFrom}>
            {t("restrictions.effective", { date: entry.effectiveFrom })}
          </time>
          {entry.sourceUrl ? (
            <Link to={entry.sourceUrl} target="_blank" rel="noreferrer">
              {value.status.synthetic
                ? t("restrictions.syntheticSource")
                : t("restrictions.source")}
            </Link>
          ) : (
            <span>{t("restrictions.sourceMissing")}</span>
          )}
        </div>
      ))}
    </section>
  )
}
