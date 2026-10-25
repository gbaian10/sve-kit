import { ExternalLink } from "lucide-react"
import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"

import { Badge } from "../../components/ui/Badge"
import { Button } from "../../components/ui/Button"
import {
  type Catalog,
  type DigitalLink,
  digitalLinkUrl,
  loadCardDigitalLinks,
  type SnapshotClient,
} from "../../data"
import { currentUiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"

interface LinksState {
  readonly key: string
  readonly links?: readonly DigitalLink[]
  readonly failed?: boolean
}

function useDigitalLinks(
  client: SnapshotClient,
  catalog: Catalog,
  cardId: string,
  attempt: number,
): LinksState | undefined {
  const snapshot = client.snapshot()
  const homeSetId = catalog.index.card(cardId)?.["home_set_id"]
  const key = JSON.stringify([client.base, snapshot?.manifestHash ?? null, cardId, attempt])
  const [state, setState] = useState<LinksState>()
  // Keyed by the manifest hash rather than the snapshot object, so a client that builds its
  // snapshot view per call does not restart the read on every render.
  useEffect(() => {
    const current = client.snapshot()
    if (!current || typeof homeSetId !== "string") return
    let active = true
    void loadCardDigitalLinks(client, current, cardId, homeSetId).then(
      (links) => {
        if (active) setState({ key, links })
      },
      () => {
        if (active) setState({ key, failed: true })
      },
    )
    return () => {
      active = false
    }
  }, [client, homeSetId, cardId, key])
  // Without a snapshot or a known card there is nothing to read, and nothing was checked either.
  if (!snapshot || typeof homeSetId !== "string") return { key, links: [] }
  return state?.key === key ? state : undefined
}

function LinkRows({
  client,
  links,
}: {
  readonly client: SnapshotClient
  readonly links: readonly DigitalLink[]
}) {
  const { t } = useTranslation()
  const uiLanguage = currentUiLanguage(usePrefs().uiLanguage)
  const config = client.snapshot()?.config
  const sameGame = (link: DigitalLink) => links.filter(({ game }) => game === link.game)
  return (
    <ul className="space-y-1.5">
      {links.map((link) => {
        const siblings = sameGame(link)
        const name = t(`digital.game.${link.game}`)
        const label =
          siblings.length > 1
            ? t("digital.numbered", { name, n: siblings.indexOf(link) + 1 })
            : name
        const url = config ? digitalLinkUrl(config, link, uiLanguage) : undefined
        return (
          <li
            key={`${link.game}:${link.officialId}`}
            className="flex flex-wrap items-center gap-x-2 gap-y-1 text-14"
          >
            <span className="text-13 text-text-2 md:w-44 md:shrink-0">
              {t(`digital.generation.${link.game}`)}
            </span>
            <Badge tone={link.relation === "same_card" ? "neutral" : "info"}>
              {t(`digital.relation.${link.relation}`)}
            </Badge>
            {url ? (
              <a
                href={url}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 text-accent-text underline-offset-2 hover:underline"
              >
                {label}
                <ExternalLink className="size-3.5" aria-hidden="true" />
                <span className="sr-only">{t("digital.newTab")}</span>
              </a>
            ) : (
              <span>{label}</span>
            )}
          </li>
        )
      })}
    </ul>
  )
}

function DigitalLinks({
  client,
  catalog,
  cardId,
}: {
  readonly client: SnapshotClient
  readonly catalog: Catalog
  readonly cardId: string
}) {
  const { t } = useTranslation()
  const [attempt, setAttempt] = useState(0)
  const state = useDigitalLinks(client, catalog, cardId, attempt)
  if (!state) return <p className="text-13 text-text-3">{t("digital.loading")}</p>
  if (state.failed)
    return (
      <div className="flex flex-wrap items-center gap-2 text-13 text-text-3">
        <span>{t("digital.failed")}</span>
        <Button
          onClick={() => {
            setAttempt(attempt + 1)
          }}
        >
          {t("status.retry")}
        </Button>
      </div>
    )
  // An empty coverage list means nobody has checked this card yet, not that it has no counterpart.
  return state.links?.length ? (
    <LinkRows client={client} links={state.links} />
  ) : (
    <p className="text-13 text-text-3">{t("digital.unchecked")}</p>
  )
}

export function CardExternalLinks({
  client,
  catalog,
  cardId,
}: {
  readonly client: SnapshotClient
  readonly catalog: Catalog
  readonly cardId: string
}) {
  const { t } = useTranslation()
  const { showDigitalLinks } = usePrefs()
  if (!showDigitalLinks) return null
  return (
    <section className="mt-6" aria-label={t("digital.sectionTitle")}>
      <h3 className="text-13 font-semibold text-text-3">{t("digital.sectionTitle")}</h3>
      <div className="mt-1.5 rounded-control border border-border p-2.5">
        <h4 className="mb-1.5 text-12 text-text-3">{t("digital.heading")}</h4>
        <DigitalLinks client={client} catalog={catalog} cardId={cardId} />
      </div>
    </section>
  )
}
