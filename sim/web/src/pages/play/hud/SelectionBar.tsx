import { Maximize2, X } from "lucide-react"
import { useTranslation } from "react-i18next"

import { useCards } from "../battle-cards"
import { costLine, statsLine } from "./reading"
import { CardKind, type Shown } from "./Sidebar"

/** The selected card in one line, where the screen has no room for the info column. */
export function SelectionBar({
  shown,
  onZoom,
  onClose,
}: {
  readonly shown: Shown
  readonly onZoom: () => void
  readonly onClose: () => void
}) {
  const { t } = useTranslation()
  const cards = useCards()
  const known = cards.find(shown.card)
  if (known === undefined) return null
  const source = cards.images?.cardImage(known.summary.printingId, known.summary.faceId)?.src
  const { cost, attack, defense } = known.summary
  const leader = known.summary.typeCode === "leader"
  const costing = leader ? null : costLine(shown.state, cost)
  const stats = leader ? null : statsLine(shown.state, attack, defense)
  return (
    <section className="bp-selbar" aria-label={t("play.info.selected")} translate="no">
      {source !== undefined && <img src={source} alt="" />}
      <div className="bp-selbar-body">
        <b lang={known.name.lang}>{known.name.text}</b>
        <span>
          <CardKind known={known} state={shown.state} />
          {costing === null
            ? null
            : costing.kind === "quoted"
              ? t("play.info.zoomCost", { n: costing.now })
              : costing.kind === "unknown"
                ? t("play.info.zoomCostUnknown")
                : t("play.info.zoomCostPrinted", { n: costing.printed })}
          {costing !== null &&
          costing.kind !== "printed" &&
          costing.printed !== null &&
          (costing.kind === "unknown" || costing.printed !== costing.now)
            ? t("play.info.zoomPrintedNote", { n: costing.printed })
            : null}
          {stats === null
            ? null
            : stats.kind === "current"
              ? t("play.info.zoomStats", { attack: stats.attack, defense: stats.defense })
              : t("play.info.zoomStatsPrinted", { attack: stats.attack, defense: stats.defense })}
        </span>
        <small>
          {known.summary.cardNo} · {shown.where}
        </small>
      </div>
      <button type="button" onClick={onZoom} aria-label={t("play.info.zoom")}>
        <Maximize2 aria-hidden="true" />
      </button>
      <button type="button" onClick={onClose} aria-label={t("play.info.close")}>
        <X aria-hidden="true" />
      </button>
    </section>
  )
}
