import { createContext, useContext } from "react"

import type { CardRestriction, CardSummary } from "../../data"
import { SAME_NAME_LIMIT } from "../../domain/deck/construction"
import {
  countOf,
  editDeck,
  type Placement,
  placementOf,
  withCount,
  withPrinting,
} from "../../domain/deck/model"
import type { DeckProblem } from "../../domain/deck/rules"
import type { Deck, DeckSection } from "../../domain/deck/types"
import type { NameDisplay } from "../../domain/nameDisplay"
import type { TextLang } from "../../domain/search"
import type { UiLanguage } from "../../i18n"
import type { DeckFacts } from "./deck-info"
import type { ConstructionState } from "./useDeckData"

interface CardStatus {
  readonly placement: Placement
  /** Copies in the card's own deck (main or evolve). */
  readonly count: number
  readonly sideCount: number
  /** The same-name limit its deck shows (3, or what the card text allows); never a cap on adding. */
  readonly limit: number
  readonly isLeader: boolean
}

export interface DeckEditor {
  readonly deck: Deck
  readonly facts: DeckFacts | null
  /** All printings of the edition are known (see `useDeckData`). */
  readonly printingsReady: boolean
  /** The cards or their printings could not be loaded; `retryData` asks again. */
  readonly dataFailed: boolean
  readonly retryData: () => void
  readonly restrictions: ReadonlyMap<string, readonly CardRestriction[]>
  readonly construction: ConstructionState
  readonly problems: readonly DeckProblem[]
  readonly uiLanguage: UiLanguage
  readonly textLang: TextLang
  readonly nameDisplay: NameDisplay
  readonly statusOf: (cardId: string) => CardStatus
  /** One more copy where the card belongs; a leader becomes the deck's leader. */
  readonly add: (cardId: string, printingId?: string) => void
  /** One copy fewer from the card's own deck. */
  readonly remove: (cardId: string) => void
  readonly setCount: (section: DeckSection, cardId: string, count: number) => void
  readonly setLeader: (cardId: string | null, printingId?: string) => void
  readonly setPrinting: (
    section: DeckSection | "leader",
    cardId: string,
    printingId: string | undefined,
  ) => void
  /** Moves a card from the side list back into the deck it belongs to. */
  readonly moveIn: (cardId: string) => void
  readonly rename: (name: string) => void
  /** Opens the card over the pool. */
  readonly openCard: (summary: CardSummary) => void
  /** The card the deck list scrolls to and outlines (a problem was clicked). */
  readonly highlight: string | null
  readonly focusCard: (cardId: string) => void
  /** Shows leaders in the pool so one can be picked. */
  readonly pickLeader: () => void
}

export const DeckEditorContext = createContext<DeckEditor | null>(null)

export function useDeckEditor(): DeckEditor {
  const editor = useContext(DeckEditorContext)
  if (!editor) throw new Error("useDeckEditor needs a DeckEditorContext")
  return editor
}

type EditorView = Pick<
  DeckEditor,
  | "deck"
  | "facts"
  | "printingsReady"
  | "dataFailed"
  | "retryData"
  | "restrictions"
  | "construction"
  | "problems"
  | "uiLanguage"
  | "textLang"
  | "nameDisplay"
  | "openCard"
  | "highlight"
  | "focusCard"
  | "pickLeader"
>

/** The editing actions over one deck; `save` stores each changed deck. */
export function createDeckEditor(view: EditorView, save: (deck: Deck) => void): DeckEditor {
  const { deck, facts } = view
  const change = (patch: Partial<Omit<Deck, "id" | "createdAt">>) => {
    save(editDeck(deck, patch, new Date()))
  }
  const statusOf = (cardId: string): CardStatus => {
    const isLeader = deck.leader?.cardId === cardId
    const sideCount = countOf(deck.side, cardId)
    const info = facts?.factsOf(cardId)
    const placement = info ? placementOf(info) : "none"
    if (!info || placement === "leader" || placement === "none")
      return { placement, count: 0, sideCount, limit: 0, isLeader }
    return {
      placement,
      count: countOf(deck[placement], cardId),
      sideCount,
      limit:
        (view.construction.state === "ready"
          ? view.construction.index.card(cardId)?.copies?.[placement]
          : undefined) ?? SAME_NAME_LIMIT,
      isLeader,
    }
  }
  const setLeader = (cardId: string | null, printingId?: string) => {
    change({
      leader:
        cardId === null ? null : { cardId, ...(printingId === undefined ? {} : { printingId }) },
    })
  }
  return {
    ...view,
    statusOf,
    add: (cardId, printingId) => {
      const status = statusOf(cardId)
      if (status.placement === "leader") setLeader(cardId, printingId)
      else if (status.placement !== "none")
        change({
          [status.placement]: withCount(
            deck[status.placement],
            cardId,
            status.count + 1,
            printingId,
          ),
        })
    },
    remove: (cardId) => {
      const status = statusOf(cardId)
      if ((status.placement === "main" || status.placement === "evolve") && status.count > 0)
        change({ [status.placement]: withCount(deck[status.placement], cardId, status.count - 1) })
    },
    setCount: (section, cardId, count) => {
      change({ [section]: withCount(deck[section], cardId, count) })
    },
    setLeader,
    setPrinting: (section, cardId, printingId) => {
      if (section !== "leader")
        change({ [section]: withPrinting(deck[section], cardId, printingId) })
      else if (deck.leader?.cardId === cardId) setLeader(cardId, printingId)
    },
    moveIn: (cardId) => {
      const copies = countOf(deck.side, cardId)
      const info = facts?.factsOf(cardId)
      const placement = info ? placementOf(info) : "none"
      if (copies === 0 || (placement !== "main" && placement !== "evolve")) return
      change({
        side: withCount(deck.side, cardId, 0),
        [placement]: withCount(deck[placement], cardId, countOf(deck[placement], cardId) + copies),
      })
    },
    rename: (name) => {
      change({ name })
    },
  }
}
