import { createContext, use, useEffect, useMemo } from "react"

import { currentUiLanguage } from "../../app/language"
import { useImageIndex } from "../../app/snapshot"
import type { CardRef, Region } from "../../battle/contract"
import {
  type CardSummary,
  catalogOf,
  type ImageIndex,
  type SnapshotClient,
  snapshotClient,
  useSnapshotRoot,
  useSnapshotStatus,
} from "../../data"
import { type DisplayedText, displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { usePrefs } from "../../settings"

/** What the snapshot says about one card face a contract message refers to. */
export interface KnownCard {
  readonly summary: CardSummary
  readonly name: DisplayedText
  /** The printed name, when `name` is a translation of it. */
  readonly original: DisplayedText | undefined
  /** The reader asked for translated names and this card has none. */
  readonly untranslated: boolean
  readonly classLabel: string | null
  /** The wording in force for this face, for reading its text; undefined when the snapshot has none. */
  readonly wording: string | undefined
}

export interface BattleCards {
  /** Undefined while the snapshot loads, and for a face the snapshot does not have. */
  readonly find: (card: CardRef) => KnownCard | undefined
  readonly images: ImageIndex | undefined
  /** Where card text is read from, once the snapshot is ready. */
  readonly text: { readonly client: SnapshotClient; readonly edition: Region } | undefined
  /** Cards of the match's edition by name or card number, best match first. */
  readonly search: (text: string, limit: number) => readonly KnownCard[]
}

const NONE: BattleCards = {
  find: () => undefined,
  images: undefined,
  text: undefined,
  search: () => [],
}

export const BattleCardsContext = createContext<BattleCards>(NONE)

export function useCards(): BattleCards {
  return use(BattleCardsContext)
}

/** Names and images of the match's cards, read from the snapshot of the match's region. */
export function useBattleCards(
  region: Region | undefined,
  matchClient?: SnapshotClient,
): BattleCards {
  const root = useSnapshotRoot()
  const client = matchClient ?? snapshotClient(root)
  const status = useSnapshotStatus(client)
  const prefs = usePrefs()
  useEffect(() => {
    if (region === undefined) return
    void (client.selectRegion?.(region) ?? client.load()).catch(() => undefined)
  }, [client, region])
  const snapshot = status.state === "ready" ? client.snapshot() : undefined
  useEffect(() => {
    if (!snapshot || snapshot.printingReady === true) return
    // The basic catalog carries one printing per card; a deck may hold any of the others.
    void client.prefetchPrintings?.().catch(() => undefined)
  }, [client, snapshot])
  const images = useImageIndex(client, snapshot !== undefined)
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  // One short line per card: "both" has no room here, so it shows the translation.
  const nameDisplay = prefs.nameDisplay === "original" ? "original" : "translated"
  return useMemo(() => {
    if (!snapshot) return NONE
    const catalog = catalogOf(snapshot)
    const known = (printingId: string): KnownCard | undefined => {
      const summary = catalog.summary(printingId)
      if (summary === undefined) return undefined
      const shown = displayName(summary.name, uiLanguage, nameDisplay)
      return {
        summary,
        name: shown.primary,
        original:
          shown.primary.lang === summary.name.original.lang ? undefined : summary.name.original,
        untranslated: shown.missingTranslation,
        classLabel:
          summary.classCode === null
            ? null
            : catalog.classLabel(summary.classCode, UI_TEXT_LANG[uiLanguage]),
        wording: revisionId(
          catalog.index.displayRevision(summary.faceId, summary.region, summary.printingId),
        ),
      }
    }
    return {
      images,
      text: region === undefined ? undefined : { client, edition: region },
      // Only front faces resolve until the screen needs the other face of a double-faced card.
      find: (card) => (card.face === 0 ? known(card.printing_id) : undefined),
      search: (text, limit) =>
        region === undefined || text.trim() === ""
          ? []
          : catalog
              .results({ ...DEFAULT_QUERY, text }, region)
              .slice(0, limit)
              .flatMap((item) => known(item.printingId) ?? []),
    }
  }, [snapshot, images, uiLanguage, nameDisplay, client, region])
}

function revisionId(row: Readonly<Record<string, unknown>> | undefined): string | undefined {
  const id = row?.["id"]
  return typeof id === "string" ? id : undefined
}
