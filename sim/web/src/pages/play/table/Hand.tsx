import { ChevronsDown, ChevronsUp } from "lucide-react"
import { type CSSProperties, useState } from "react"
import { useTranslation } from "react-i18next"

import type { Pile, TableCard } from "../../../battle/view/table"
import { useCards } from "../battle-cards"
import { CostBadge, CounterBadge } from "../dice/Dice"
import { usePick } from "../inspector"
import type { Layout } from "../layout"
import { CardBack } from "./CardBack"
import { CardFace } from "./Cards"
import { CARD_H, CARD_W } from "./geometry"

/**
 * A card of the viewer's own hand: its face, the cost the authority currently quotes, and the
 * attack and defense printed on it. A cost that is not quoted (or not settled yet) is shown as
 * unknown: the printed cost is not what the card costs now.
 */
function HandCard({ card, style }: { readonly card: TableCard; readonly style: CSSProperties }) {
  const { t } = useTranslation()
  const printed = useCards().find(card.face)?.summary
  const pick = usePick({ kind: "object", handle: card.handle })
  return (
    <div
      className={pick.selected ? "bp-hand-card is-selected" : "bp-hand-card"}
      style={style}
      {...pick.props}
    >
      <CardFace card={card.face} tag />
      <CostBadge value={card.cost} />
      {printed?.attack != null && printed.defense != null ? (
        <>
          <CounterBadge
            kind="atk"
            value={printed.attack}
            className="bp-hstat bp-atk"
            label={t("play.stat.attack")}
          />
          <CounterBadge
            kind="hp"
            value={printed.defense}
            className="bp-hstat bp-def"
            label={t("play.stat.defense")}
          />
        </>
      ) : null}
    </div>
  )
}

/** The hand: one straight row (or a two-column grid on landscape phones), every card fully shown. */
export function Hand({ hand, layout }: { readonly hand: Pile; readonly layout: Layout }) {
  const { t } = useTranslation()
  const [collapsed, setCollapsed] = useState(false)
  const a = layout.handArea
  const cards = hand.cards
  if (layout.hand === "column") {
    const w = (a.right - a.left - 18) / 2
    return (
      <section
        className="bp-hand-col"
        data-hit-area=""
        style={{ left: a.left, top: a.top, width: a.right - a.left, bottom: layout.vh - a.bottom }}
        aria-label={t("play.hand.self")}
      >
        <div className="bp-hand-col-title">{t("play.hand.count", { n: hand.count })}</div>
        <div className="bp-hand-grid" style={{ "--w": `${String(w)}px` } as CSSProperties}>
          {cards.map((card) => (
            <HandCard
              key={card.handle}
              card={card}
              style={{ width: w, height: (w * CARD_H) / CARD_W }}
            />
          ))}
        </div>
      </section>
    )
  }
  const h = layout.handH
  const w = (h * CARD_W) / CARD_H
  const avail = a.right - a.left - 24
  const n = cards.length
  const natural = n * w + (n - 1) * 10
  // Overlap only so far that the next card never covers the cost corner; beyond that, scroll.
  const overlap = natural > avail
  const step = overlap ? Math.max(w * 0.64, (avail - w) / Math.max(1, n - 1)) : w + 10
  const total = w + step * Math.max(0, n - 1)
  return (
    <section
      className={collapsed ? "bp-hand-row is-collapsed" : "bp-hand-row"}
      data-hit-area=""
      style={{ left: a.left, width: a.right - a.left, top: a.top, height: a.bottom - a.top }}
      aria-label={t("play.hand.self")}
    >
      <div className="bp-hand-scroll">
        <div
          className={overlap ? "bp-hand-track is-overlap" : "bp-hand-track"}
          style={{ width: total, height: h }}
        >
          {cards.map((card, i) => (
            <HandCard
              key={card.handle}
              card={card}
              style={{ width: w, height: h, left: i * step, zIndex: 10 + i }}
            />
          ))}
        </div>
      </div>
      {/* Stacked, the board reaches down to the hand: there is no free strip for the handle. */}
      {layout.portrait && !layout.stacked ? (
        <button
          type="button"
          className="bp-hand-toggle"
          onClick={() => {
            setCollapsed(!collapsed)
          }}
          aria-label={collapsed ? t("play.hand.expand") : t("play.hand.collapse")}
        >
          {collapsed ? <ChevronsUp aria-hidden="true" /> : <ChevronsDown aria-hidden="true" />}
        </button>
      ) : null}
    </section>
  )
}

/** Room kept beside a stacked board's opponent hand: the corner chip, and the count after the backs. */
const CHIP_ROOM = 156
const COUNT_ROOM = 84

/** The opponent's hand: backs only (a number of cards, no identities), in a strip above their mat. */
export function OppHand({ count, layout }: { readonly count: number; readonly layout: Layout }) {
  const { t } = useTranslation()
  if (layout.hand === "column") return null
  const h = layout.oppH
  const w = (h * CARD_W) / CARD_H
  const boardW = layout.matW * layout.u
  // Stacked, the strip is shared with the corner chip on the left and the count on the right:
  // the backs close up to fit between the two.
  const spread = layout.stacked
    ? (boardW - CHIP_ROOM - COUNT_ROOM - w) / Math.max(1, count - 1)
    : (boardW * 0.5) / Math.max(1, count)
  const step = Math.min(w * 0.7, spread)
  const width = w + step * Math.max(0, count - 1)
  return (
    <section
      className="bp-opp-hand"
      data-hit-area=""
      style={{
        top: layout.boardTop - h - 6,
        // Stacked, the corner chip takes the left: the backs and their count keep to the right.
        left: layout.stacked
          ? layout.boardLeft + boardW - width - COUNT_ROOM
          : layout.boardLeft + boardW / 2 - width / 2,
        width,
        height: h,
      }}
      aria-label={t("play.hand.opponent")}
    >
      {Array.from({ length: count }, (_, i) => (
        <div key={i} className="bp-opp-card" style={{ width: w, height: h, left: i * step }}>
          <CardBack />
        </div>
      ))}
      <span className="bp-opp-count">
        {t("play.zone.hand")} <b>{count}</b>
      </span>
    </section>
  )
}

/** The other seat's two hands in the dual view: the backs it is shown and its own cards. */
export function DualHand({
  hand,
  backs,
  layout,
}: {
  readonly hand: Pile
  readonly backs: number
  readonly layout: Layout
}) {
  const { t } = useTranslation()
  // The turn panel sits at the bottom-right of this half: the hand keeps clear of it.
  const a = { ...layout.handArea, right: layout.handArea.right - 220 }
  const h = layout.handH
  const w = (h * CARD_W) / CARD_H
  const cards = hand.cards
  const step = Math.min(w + 10, (a.right - a.left - 24 - w) / Math.max(1, cards.length - 1))
  const oh = layout.oppH
  const ow = (oh * CARD_W) / CARD_H
  const ostep = Math.min(ow * 0.7, (layout.matW * layout.u * 0.5) / Math.max(1, backs))
  const owidth = ow + ostep * Math.max(0, backs - 1)
  const width = w + step * Math.max(0, cards.length - 1)
  return (
    <>
      <section
        className="bp-dual-backs"
        data-hit-area=""
        style={{
          top: layout.boardTop - oh - 6,
          left: layout.boardLeft + (layout.matW * layout.u) / 2 - owidth / 2,
          width: owidth,
          height: oh,
        }}
        aria-label={t("play.hand.opponent")}
      >
        {Array.from({ length: backs }, (_, i) => (
          <div key={i} className="bp-dual-back" style={{ width: ow, height: oh, left: i * ostep }}>
            <CardBack />
          </div>
        ))}
        <span className="bp-opp-count">
          {t("play.zone.hand")} <b>{backs}</b>
        </span>
      </section>
      <section
        className="bp-dual-own"
        style={{
          top: a.top + (a.bottom - a.top - h) / 2,
          left: (a.left + a.right) / 2 - width / 2,
          width,
          height: h,
        }}
        aria-label={t("play.hand.self")}
      >
        {cards.map((card, i) => (
          <div
            key={card.handle}
            className="bp-dual-card"
            style={{ width: w, height: h, left: i * step, zIndex: 10 + i }}
          >
            <CardFace card={card.face} tag />
          </div>
        ))}
      </section>
    </>
  )
}
