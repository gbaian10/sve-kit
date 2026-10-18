import { type ReactNode, useEffect, useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { Link as RouterLink, type To } from "react-router"

import { loadCardRelated } from "../../app/card-related"
import { useAnnotatedTexts, useImageIndex } from "../../app/snapshot"
import { CardImage } from "../../components/card/CardImage"
import { useTextLanguage } from "../../components/card/text-language"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import { rovingRadioKeyDown } from "../../components/ui/useRovingRadio"
import {
  arrayValue,
  type CardSummary,
  type Catalog,
  type Row,
  type SnapshotClient,
  stringValue,
} from "../../data"
import { displayName } from "../../domain/nameDisplay"
import type { Region } from "../../domain/search"
import { currentUiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"
import { isEvolveKind, mentionedCards, relatedGroups } from "./related-cards"

export interface CardLink {
  readonly to: To
  readonly state: unknown
}

/** A printing of this card as the "other printings" tab shows it. */
export interface PrintingEntry {
  readonly summary: CardSummary
  readonly rarity: string | undefined
  readonly link: CardLink
}

type TabId = "evolve" | "reference" | "printings"
const TAB_ORDER: readonly TabId[] = ["evolve", "reference", "printings"]

interface RelatedState {
  readonly key: string
  readonly rows?: readonly Row[]
  readonly error?: unknown
}

/** The official related rows of one card, reloaded when the card or the snapshot changes. */
function useCardRelated(
  client: SnapshotClient,
  catalog: Catalog,
  cardId: string,
  attempt: number,
): RelatedState | undefined {
  const snapshot = client.snapshot()
  const key = JSON.stringify([client.base, snapshot?.manifestHash ?? null, cardId, attempt])
  const [state, setState] = useState<RelatedState>()
  const homeSetId = catalog.index.card(cardId)?.["home_set_id"]
  useEffect(() => {
    if (!snapshot || typeof homeSetId !== "string") return
    let active = true
    void loadCardRelated(client, snapshot, cardId, homeSetId).then(
      (rows) => {
        if (active) setState({ key, rows })
      },
      (error: unknown) => {
        if (active) setState({ key, error })
      },
    )
    return () => {
      active = false
    }
  }, [client, snapshot, cardId, homeSetId, key])
  // A missing snapshot (synthetic pages) or an unknown card has no rows and nothing to wait for.
  if (!snapshot || typeof homeSetId !== "string") return { key, rows: [] }
  return state?.key === key ? state : undefined
}

function Thumbnail({
  card,
  link,
  images,
  current,
  children,
}: {
  readonly card: CardSummary
  readonly link: CardLink
  readonly images: ReturnType<typeof useImageIndex>
  /** Set for this card's own printings: the current one is marked and a switch replaces history. */
  readonly current?: boolean
  readonly children?: ReactNode
}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const name = displayName(card.name, currentUiLanguage(prefs.uiLanguage), prefs.nameDisplay)
  // The number is the visible label (names truncate too easily at this size); the name is the
  // accessible name and the hover tooltip.
  return (
    <li className="w-18">
      <RouterLink
        to={link.to}
        state={link.state}
        replace={current !== undefined}
        preventScrollReset={current !== undefined}
        aria-current={current ? "page" : undefined}
        aria-label={t("card.imageAlt", { name: name.primary.text, cardNo: card.cardNo })}
        title={name.primary.text}
        className="group flex flex-col items-center gap-1 rounded-control text-text-2 hover:text-text-1"
      >
        <CardImage
          summary={card}
          name={name.primary}
          images={images}
          alt="decorative"
          sizes="72px"
          className="w-full group-aria-[current=page]:ring-2 group-aria-[current=page]:ring-accent"
        />
        <span className="w-full text-center font-mono text-11 leading-tight wrap-anywhere tabular-nums">
          {card.cardNo}
        </span>
        {children}
      </RouterLink>
    </li>
  )
}

function Group({
  label,
  cards,
  images,
  linkTo,
}: {
  readonly label: string | undefined
  readonly cards: readonly CardSummary[]
  readonly images: ReturnType<typeof useImageIndex>
  readonly linkTo: (card: CardSummary) => CardLink
}) {
  if (cards.length === 0) return null
  return (
    <div>
      {label !== undefined && <h4 className="mb-1.5 text-12 text-text-3">{label}</h4>}
      <ul aria-label={label} className="flex flex-wrap gap-2">
        {cards.map((card) => (
          <Thumbnail key={card.printingId} card={card} link={linkTo(card)} images={images} />
        ))}
      </ul>
    </div>
  )
}

/**
 * The related cards under the text (#521): evolve relations, references and other printings as
 * tabs; a tab without content is left out and the first one with content opens.
 */
export function RelatedCards({
  client,
  catalog,
  edition,
  cardId,
  printingId,
  revisionIds,
  printings,
  imageClient,
  linkTo,
}: {
  readonly client: SnapshotClient
  readonly catalog: Catalog
  readonly edition: Region
  readonly cardId: string
  readonly printingId: string
  /** The displayed revision of each face, whose text names the mentioned cards. */
  readonly revisionIds: readonly string[]
  readonly printings: readonly PrintingEntry[]
  /** Resolves the thumbnails of this card's other printings; the page's client until it is ready. */
  readonly imageClient?: SnapshotClient | undefined
  readonly linkTo: (card: CardSummary) => CardLink
}) {
  const { t } = useTranslation()
  const idPrefix = useId()
  const language = useTextLanguage()
  const images = useImageIndex(imageClient ?? client, true)
  const [attempt, setAttempt] = useState(0)
  const related = useCardRelated(client, catalog, cardId, attempt)
  const effects = useAnnotatedTexts(
    client,
    revisionIds.map((id) => ({
      owner: { kind: "face_revision", id },
      field: "effect" as const,
      ordinal: null,
    })),
    language,
    edition,
  )
  const sections = useAnnotatedTexts(
    client,
    effects.flatMap((state, index) =>
      (state.value?.originalSections ?? []).map((section) => ({
        owner: { kind: "face_revision", id: revisionIds[index] ?? "" },
        field: "section" as const,
        ordinal: section.ordinal,
      })),
    ),
    language,
    edition,
  )
  const [selected, setSelected] = useState<TabId | null>(null)
  // The section waits for the official rows so its tabs do not shuffle after the first paint.
  if (related === undefined) return null
  const mentions = mentionedCards(
    [...effects, ...sections].map((state) => state.value),
    edition,
  )
  const groups = relatedGroups({
    cardId,
    related: related.rows ?? [],
    mentions,
    representative: (id) => {
      const card = catalog.index.card(id)
      const view = arrayValue(card?.["regions"] ?? [])
        .map((raw) => raw as Row)
        .find((entry) => entry["region"] === edition)
      const defaultId = view?.["default_printing_id"]
      return typeof defaultId === "string" ? catalog.summary(defaultId) : undefined
    },
    evolveKind: (id) =>
      catalog.index
        .facesOf(id)
        .some((face) =>
          isEvolveKind(
            arrayValue(
              catalog.index.displayRevision(stringValue(face["id"]), edition)?.["special_kinds"] ??
                [],
            ),
          ),
        ),
  })
  const tabs: { readonly id: TabId; readonly label: string; readonly content: ReactNode }[] = []
  if (groups.evolve.length > 0)
    tabs.push({
      id: "evolve",
      label: t("cardPage.related.evolve"),
      content: <Group label={undefined} cards={groups.evolve} images={images} linkTo={linkTo} />,
    })
  if (groups.mentions.length > 0 || groups.otherOfficial.length > 0)
    tabs.push({
      id: "reference",
      label: t("cardPage.related.reference"),
      content: (
        <div className="space-y-3">
          <Group
            label={t("cardPage.related.mentions")}
            cards={groups.mentions}
            images={images}
            linkTo={linkTo}
          />
          <Group
            label={t("cardPage.related.otherOfficial")}
            cards={groups.otherOfficial}
            images={images}
            linkTo={linkTo}
          />
        </div>
      ),
    })
  if (printings.length > 1)
    tabs.push({
      id: "printings",
      label: t("cardPage.related.printings"),
      content: (
        <ul aria-label={t("cardPage.related.printings")} className="flex flex-wrap gap-2">
          {printings.map(({ summary, rarity, link }) => (
            <Thumbnail
              key={summary.printingId}
              card={summary}
              link={link}
              images={images}
              current={summary.printingId === printingId}
            >
              {rarity !== undefined && rarity !== "" && (
                <span className="text-11 leading-tight text-text-3">{rarity}</span>
              )}
            </Thumbnail>
          ))}
        </ul>
      ),
    })
  const failure = related.error !== undefined && (
    <div role="alert" className="mt-2 flex flex-wrap items-center gap-2 text-13 text-text-2">
      {t("cardPage.related.loadFailed")}
      <Button
        className="h-8 px-3 text-13"
        onClick={() => {
          setAttempt((value) => value + 1)
        }}
      >
        {t("status.retry")}
      </Button>
    </div>
  )
  if (tabs.length === 0) return failure || null
  const active = tabs.find((tab) => tab.id === selected) ?? tabs[0]
  if (!active) return null
  const ids = tabs.map((tab) => tab.id)
  const ordered = TAB_ORDER.filter((id) => ids.includes(id))
  return (
    <section className="mt-6" aria-label={t("cardPage.related.title")}>
      <h3 className="text-13 font-semibold text-text-3">{t("cardPage.related.title")}</h3>
      <div
        role="tablist"
        aria-label={t("cardPage.related.title")}
        className="mt-1 flex gap-1 border-b border-border"
      >
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            id={`${idPrefix}-${tab.id}`}
            aria-selected={tab.id === active.id}
            aria-controls={`${idPrefix}-panel`}
            tabIndex={tab.id === active.id ? 0 : -1}
            onClick={() => {
              setSelected(tab.id)
            }}
            onKeyDown={(event) => {
              rovingRadioKeyDown(event, ordered, active.id, setSelected, idPrefix)
            }}
            className={cn(
              "-mb-px h-10 border-b-2 px-3 text-14 font-semibold",
              tab.id === active.id
                ? "border-text-1 text-text-1"
                : "border-transparent text-text-2 hover:text-text-1",
            )}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        id={`${idPrefix}-panel`}
        aria-labelledby={`${idPrefix}-${active.id}`}
        className="mt-3"
      >
        {active.content}
      </div>
      {failure}
    </section>
  )
}
