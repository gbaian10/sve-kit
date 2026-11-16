import { useTranslation } from "react-i18next"

import { useCards } from "../battle-cards"
import type { Zoomed } from "../inspector"
import { Overlay } from "./Overlay"
import { CardKind, CardWordingText } from "./Sidebar"

/** One card large, with its text beside it; where no info column fits, this is how a card is read. */
export function Zoom({
  zoomed,
  onClose,
}: {
  readonly zoomed: Zoomed
  readonly onClose: () => void
}) {
  const { t } = useTranslation()
  const cards = useCards()
  const known = cards.find(zoomed.face)
  if (known === undefined) return null
  const source = cards.images?.cardImage(known.summary.printingId, known.summary.faceId)?.src
  const under = zoomed.under === null ? undefined : cards.find(zoomed.under)
  const { cost, attack, defense } = known.summary
  return (
    <Overlay label={known.name.text} front onClose={onClose}>
      <div className="bp-zoom" translate="no">
        {source !== undefined && (
          <img
            src={source}
            alt={t("card.imageAlt", { name: known.name.text, cardNo: known.summary.cardNo })}
          />
        )}
        <div className="bp-zoom-info">
          <div className="bp-zoom-no">{known.summary.cardNo}</div>
          <h2 lang={known.name.lang}>{known.name.text}</h2>
          <p>
            <CardKind known={known} />
            {cost !== null ? t("play.info.zoomCost", { n: cost }) : null}
            {attack !== null && defense !== null
              ? t("play.info.zoomStats", { attack, defense })
              : null}
          </p>
          {known.summary.typeCode !== "leader" ? <CardWordingText known={known} /> : null}
          {under !== undefined ? (
            <p>{t("play.info.under", { name: under.name.text, no: under.summary.cardNo })}</p>
          ) : null}
          <button type="button" onClick={onClose}>
            {t("play.info.close")}
          </button>
        </div>
      </div>
    </Overlay>
  )
}
