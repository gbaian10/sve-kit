import type { CSSProperties } from "react"
import { useTranslation } from "react-i18next"

import type { CardRef } from "../../../battle/contract"
import { CardPlaceholder } from "../../../components/card/CardPlaceholder"
import { useCards } from "../battle-cards"

/** The card's image, with its name over it when asked: card art is read in the UI language. */
export function CardFace({
  card,
  tag = false,
}: {
  readonly card: CardRef
  readonly tag?: boolean
}) {
  const { t } = useTranslation()
  const cards = useCards()
  const known = cards.find(card)
  const source =
    known && cards.images?.cardImage(known.summary.printingId, known.summary.faceId)?.src
  return (
    <>
      {known !== undefined && source !== undefined ? (
        <img
          className="bp-face"
          data-image-printing={known.summary.printingId}
          data-image-face={known.summary.faceId}
          src={source}
          alt={t("card.imageAlt", { name: known.name.text, cardNo: known.summary.cardNo })}
          draggable={false}
          loading="eager"
        />
      ) : (
        // Until the snapshot and its images are in: the same box, with the app's placeholder.
        // The image index fetches the faces of the elements that name one, so the box names its own.
        <span
          className="bp-face is-missing"
          data-image-printing={known?.summary.printingId}
          data-image-face={known?.summary.faceId}
          role="img"
          aria-label={t("play.card.unknown")}
        >
          <CardPlaceholder pulse />
        </span>
      )}
      {tag && known !== undefined ? (
        <span
          className={known.untranslated ? "bp-name-tag is-untranslated" : "bp-name-tag"}
          lang={known.name.lang}
        >
          {known.name.text}
        </span>
      ) : null}
    </>
  )
}

/**
 * The art of a card, cut out of the large card image (no separate art download). The box is the
 * portrait art crop of the export, [36,89,420,377) on a 459x641 card; it covers its container
 * like `background-size: cover` (times `zoom`), with `focus` as the vertical position in percent.
 */
export function ArtCrop({
  card,
  zoom = 1,
  focus = 50,
}: {
  readonly card: CardRef
  readonly zoom?: number
  readonly focus?: number
}) {
  const cards = useCards()
  const known = cards.find(card)
  const source =
    known && cards.images?.cardImage(known.summary.printingId, known.summary.faceId)?.src
  return (
    // The art shows through a window smaller than the image; the overflow audit skips boxes
    // marked `data-hit-area`.
    <span
      className="bp-art"
      style={{ "--zoom": zoom, "--focus": `${String(focus)}%` } as CSSProperties}
      aria-hidden="true"
      data-hit-area=""
    >
      <span className="bp-art-box" data-hit-area="">
        {source !== undefined && <img src={source} alt="" draggable={false} />}
      </span>
    </span>
  )
}
