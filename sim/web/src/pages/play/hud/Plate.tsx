import { Ban, Hand as HandIcon, Heart, Layers, Skull } from "lucide-react"
import { useTranslation } from "react-i18next"

import sepIcon from "../../../assets/card-labels/dice/sep.svg"
import epIcon from "../../../assets/card-labels/ep.svg"
import type { CardRef } from "../../../battle/contract"
import type { SeatTable } from "../../../battle/view/table"
import { CLASS_ICON } from "../../../components/card/classIcons"
import { cn } from "../../../components/ui/cn"
import { classStyle } from "../../../domain/classes"
import { useCards } from "../battle-cards"
import { ArtCrop } from "../table/Cards"

/** One player's numbers beside the table; nothing here is hidden information. */
export function Plate({
  seat,
  leader,
  self,
  compact,
}: {
  readonly seat: SeatTable
  readonly leader: CardRef
  readonly self: boolean
  readonly compact: boolean
}) {
  const { t } = useTranslation()
  const known = useCards().find(leader)
  const { ep, sep } = seat
  const locked = sep.opensIn > 0
  return (
    <section
      className={cn(
        "bp-plate",
        self ? "is-self" : "is-opp",
        seat.active && "is-active",
        compact && "is-compact",
      )}
      aria-label={self ? t("play.side.self") : t("play.side.opponent")}
    >
      {/* The art is cut to a circle; the overflow audit skips boxes marked `data-hit-area`. */}
      <div className="bp-avatar" data-hit-area="">
        <ArtCrop card={leader} zoom={1.35} focus={25} />
      </div>
      <div className="bp-plate-body">
        <div className="bp-plate-name">
          <span className="bp-plate-who">
            {self ? t("play.side.self") : t("play.side.opponent")}
            {known?.classLabel != null && (
              <span className="bp-plate-cls">
                <img src={CLASS_ICON[classStyle(known.summary.classCode)]} alt="" />
                {known.classLabel}
              </span>
            )}
            {seat.first !== null && (
              <em className={seat.first ? "bp-first is-first" : "bp-first"}>
                {seat.first ? t("play.order.first") : t("play.order.second")}
              </em>
            )}
          </span>
          {known !== undefined && (
            <span className="bp-plate-class" lang={known.name.lang}>
              {known.name.text}
            </span>
          )}
        </div>
        <div className="bp-plate-stats">
          <span className="bp-hud-life" title={t("play.stat.life")}>
            <Heart aria-hidden="true" />
            <span className="sr-only">{t("play.stat.life")}</span>
            {seat.life}
          </span>
          <span title={t("play.stat.ppOf", { left: seat.pp, max: seat.maxPp })}>
            <i className="bp-pp-gem" />
            <span className="sr-only">{t("play.stat.pp")}</span>
            {seat.pp}
            <small>/{seat.maxPp}</small>
          </span>
          {/* Same cells in the same order for both players; a seat without EP shows a grey 0/0. */}
          <span
            className={ep.max > 0 ? "bp-hud-ep" : "bp-hud-ep is-none"}
            title={
              ep.max === 0 && seat.first === true ? t("play.stat.epNoneFirst") : t("play.stat.ep")
            }
          >
            <img
              className={ep.current > 0 ? "bp-mini-icon" : "bp-mini-icon is-used"}
              src={epIcon}
              alt={t("play.stat.ep")}
            />
            {ep.current}
            <small>/{ep.max}</small>
          </span>
          <span
            className={locked ? "bp-hud-sep is-locked" : "bp-hud-sep"}
            title={locked ? t("play.stat.sepLocked", { n: sep.opensIn }) : t("play.stat.sepOpen")}
          >
            <img
              className={sep.current > 0 ? "bp-mini-icon is-sep" : "bp-mini-icon is-sep is-used"}
              src={sepIcon}
              alt={t("play.stat.sep")}
            />
            {sep.current}
            <small>/{sep.max}</small>
            {locked ? <b className="bp-sep-count is-hud">{sep.opensIn}</b> : null}
          </span>
          <span className="bp-hud-hand" title={t("play.zone.hand")}>
            <HandIcon aria-hidden="true" />
            {seat.hand.count}
            <em>{t("play.zone.hand")}</em>
          </span>
          <span className="bp-hud-deck" title={t("play.zone.deck")}>
            <Layers aria-hidden="true" />
            {seat.deck.count}
            <em>{t("play.zone.deck")}</em>
          </span>
          <span className="bp-hud-pile" title={t("play.zone.cemetery")}>
            <Skull aria-hidden="true" />
            {seat.cemetery.count}
            <em>{t("play.zone.cemetery")}</em>
          </span>
          <span className="bp-hud-pile" title={t("play.zone.banish")}>
            <Ban aria-hidden="true" />
            {seat.banish.count}
            <em>{t("play.zone.banish")}</em>
          </span>
        </div>
      </div>
    </section>
  )
}
