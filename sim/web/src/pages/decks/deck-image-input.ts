import type { TFunction } from "i18next"

import type { ImageIndex } from "../../data"
import { totalCount } from "../../domain/deck/model"
import type { Deck, DeckEntry, DeckLeader } from "../../domain/deck/types"
import type { DeckImageCard, DeckImageInput, DeckImageSize } from "./deck-image"
import type { DeckFacts } from "./deck-info"

/** What a deck picture shows; a saved deck or an import preview that is not saved yet. */
type DeckPictureDeck = Pick<Deck, "name" | "edition" | "basis" | "leader" | "main" | "evolve">

/** Theme colours for the canvas, read from the page's own tokens. */
function canvasColors(): DeckImageInput["colors"] {
  const styles = getComputedStyle(document.documentElement)
  const token = (name: string) => styles.getPropertyValue(name).trim()
  return {
    page: token("--bg"),
    text: token("--text-1"),
    muted: token("--text-3"),
    badge: token("--accent"),
    badgeText: token("--accent-ink"),
  }
}

/**
 * The deck as the canvas draws it. The exported picture draws `card_l`; the on-page import preview
 * draws `card_s` (128 px wide, enough for a card slot in a page-wide picture) so it appears at once.
 */
export function deckImageInput({
  deck,
  facts,
  images,
  size,
  file,
  missing,
  t,
  basisLabel,
}: {
  readonly deck: DeckPictureDeck
  readonly facts: DeckFacts
  readonly images: ImageIndex | undefined
  readonly size: DeckImageSize
  readonly file: "card_l" | "card_s"
  readonly missing: NonNullable<DeckImageInput["missing"]>
  readonly t: TFunction
  readonly basisLabel: (basis: Deck["basis"]) => string
}): DeckImageInput {
  const srcOf = (entry: DeckEntry | DeckLeader) => {
    const summary = facts.summaryOf(entry.cardId, entry.printingId)
    const source = summary ? images?.cardImage(summary.printingId, summary.faceId) : undefined
    return file === "card_s" ? (source?.small ?? source?.src) : source?.src
  }
  const nameOf = (cardId: string) => facts.factsOf(cardId)?.name ?? cardId
  const card = (entry: DeckEntry): DeckImageCard => ({
    src: srcOf(entry),
    label: nameOf(entry.cardId),
    count: entry.count,
  })
  const { leader } = deck
  const leaderName = leader ? nameOf(leader.cardId) : ""
  return {
    size,
    title: deck.name,
    subtitle: t("decks.imageSubtitle", {
      edition: t(`options.edition.${deck.edition}`),
      basis: basisLabel(deck.basis),
      main: totalCount(deck.main),
      evolve: totalCount(deck.evolve),
    }),
    leader: leader ? { src: srcOf(leader), label: leaderName } : null,
    leaderLine: t("decks.importLeader", { name: leaderName }),
    sections: [
      { label: t("decks.section.main"), cards: deck.main.map(card) },
      { label: t("decks.section.evolve"), cards: deck.evolve.map(card) },
    ],
    // The viewer's own calendar date, as YYYY-MM-DD.
    footer: t("decks.imageFooter", { date: new Date().toLocaleDateString("sv-SE") }),
    missing,
    colors: canvasColors(),
  }
}

/**
 * Whether some card's image row is still on its way (a reprint's rows come from the search
 * worker); drawing then would show a placeholder that a moment later becomes the card.
 */
export function deckImagesPending(
  deck: DeckPictureDeck,
  facts: DeckFacts,
  images: ImageIndex | undefined,
): boolean {
  if (images === undefined) return true
  return [...(deck.leader ? [deck.leader] : []), ...deck.main, ...deck.evolve].some((entry) => {
    const summary = facts.summaryOf(entry.cardId, entry.printingId)
    if (summary === undefined) return false
    const { printingId, faceId } = summary
    // A failed request will not bring the row; the card is then drawn as a placeholder.
    if (images.failedFace?.(printingId, faceId) ?? images.failed) return false
    return images.known?.(printingId, faceId) === false
  })
}
