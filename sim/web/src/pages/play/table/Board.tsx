import { Ban, ChevronsUp, Heart, Layers, type LucideIcon, Skull } from "lucide-react"
import type { CSSProperties, ReactNode } from "react"
import { useTranslation } from "react-i18next"

import epIcon from "../../../assets/card-labels/ep.svg"
import type { CardRef, Seat } from "../../../battle/contract"
import type { Pile, SeatTable, TableCard, TableModel } from "../../../battle/view/table"
import type { BattleMat } from "../../../settings"
import { useCards } from "../battle-cards"
import { CounterBadge, KeywordDie } from "../dice/Dice"
import { isDieKeyword, isNamedKeyword } from "../dice/keywords"
import sepIcon from "../dice/sep.svg"
import type { Layout } from "../layout"
import { CardBack } from "./CardBack"
import { CardFace } from "./Cards"
import { GAP, type MatGeometry, matGeometry, type PileName, px } from "./geometry"
import { MatPicture, SharedMatPicture, ZoneFrames } from "./mats"

const classes = (...names: (string | false)[]): string => names.filter(Boolean).join(" ")

const PILES: readonly PileName[] = ["leader", "deck", "cemetery", "evolve", "banish"]

interface MatProps {
  readonly seat: SeatTable
  readonly leader: CardRef
  readonly opp: boolean
  readonly u: number
  readonly mat: MatGeometry
}

function FieldCardView({
  card,
  slot,
  opp,
  u,
  mat,
}: {
  readonly card: TableCard
  readonly slot: number
  readonly opp: boolean
  readonly u: number
  readonly mat: MatGeometry
}) {
  const { t } = useTranslation()
  const printed = useCards().find(card.face)?.summary
  const evolved = card.evolution !== "base"
  const superEvolved = card.evolution === "super_evolved"
  // The follower that evolved stays on the table under its evolve card.
  const under = evolved && card.card.printing_id !== card.face.printing_id
  return (
    <div
      className={classes(
        "bp-card bp-onfield",
        opp && "is-opp-side",
        card.acted && "is-engaged",
        evolved && "is-evolved",
        superEvolved && "is-super",
      )}
      style={px(mat.onBoard(mat.slotCard("field", slot, opp), opp), u)}
      data-hit-area=""
    >
      <div className="bp-card-inner">
        {under ? (
          <div className="bp-under">
            <CardFace card={card.card} />
          </div>
        ) : null}
        <CardFace card={card.face} tag />
        {superEvolved ? (
          <span className="bp-evo-tag is-super">{t("play.card.superEvolved")}</span>
        ) : evolved ? (
          <span className="bp-evo-tag">{t("play.card.evolved")}</span>
        ) : null}
      </div>
      {card.acted && <span className="sr-only">{t("play.card.acted")}</span>}
      <div className="bp-card-badges" data-hit-area="">
        {card.stats !== null ? (
          <>
            <CounterBadge
              kind="atk"
              value={card.stats.attack}
              className="bp-fstat bp-atk"
              label={t("play.stat.attack")}
              printed={printed?.attack}
            />
            <CounterBadge
              kind="hp"
              value={card.stats.life}
              className="bp-fstat bp-def"
              label={t("play.stat.defense")}
              printed={printed?.defense}
            />
          </>
        ) : null}
        <span className="bp-status-row">
          {card.keywords.map((keyword) =>
            isDieKeyword(keyword) ? (
              // Only an engaged ward restricts attacks, so that is when its die lights up.
              <KeywordDie key={keyword} keyword={keyword} on={keyword === "guard" && card.acted} />
            ) : (
              <span key={keyword} className="bp-status">
                {isNamedKeyword(keyword) ? t(`play.keyword.${keyword}`) : keyword}
              </span>
            ),
          )}
          {card.counters.map((counter) => (
            // Counter names arrive as the rules term of the card text.
            <span key={counter.name} className="bp-status" lang="ja">
              {t("play.card.counter", { name: counter.name, n: counter.amount })}
            </span>
          ))}
        </span>
      </div>
    </div>
  )
}

function ExCard({
  card,
  slot,
  opp,
  u,
  mat,
}: {
  readonly card: TableCard
  readonly slot: number
  readonly opp: boolean
  readonly u: number
  readonly mat: MatGeometry
}) {
  return (
    <div
      className={classes("bp-card bp-onfield", opp && "is-opp-side")}
      style={px(mat.onBoard(mat.slotCard("ex", slot, opp), opp), u)}
      data-hit-area=""
    >
      <div className="bp-card-inner">
        <CardFace card={card.face} tag />
      </div>
    </div>
  )
}

/** The face-up card on top of an open pile; an unidentifiable top shows nothing but the count. */
function TopCard({ pile, banished }: { readonly pile: Pile; readonly banished: boolean }) {
  const top = pile.cards.at(-1)
  if (pile.count === 0 || top === undefined) return null
  return (
    <div className={banished ? "bp-banished" : "bp-pile-top"}>
      <CardFace card={top.face} />
    </div>
  )
}

function PileView({ pile, seat, leader, opp, u, mat }: MatProps & { readonly pile: PileName }) {
  const { t } = useTranslation()
  let body: ReactNode
  let label: string
  let count: number | undefined
  let extra: string | undefined
  let Icon: LucideIcon | undefined
  if (pile === "leader") {
    label = t("play.zone.leader")
    body = (
      <>
        <CardFace card={leader} />
        <span className="bp-life">
          <Heart aria-hidden="true" />
          <span className="sr-only">{t("play.stat.life")}</span>
          <b>{seat.life}</b>
        </span>
      </>
    )
  } else if (pile === "deck") {
    label = t("play.zone.deck")
    Icon = Layers
    count = seat.deck.count
    // A layer for every five cards: the stack thins as the deck runs down.
    body = (
      <div className="bp-stack">
        {Array.from({ length: Math.min(6, Math.ceil(count / 5)) }, (_, i) => (
          <div key={i} className="bp-stack-layer" style={{ "--i": i } as CSSProperties}>
            <CardBack />
          </div>
        ))}
      </div>
    )
  } else if (pile === "evolve") {
    label = t("play.zone.evolveDeck")
    Icon = ChevronsUp
    const used = seat.evolveDeck.cards.filter((card) => !card.faceDown).length
    count = seat.evolveDeck.count - used
    extra = used > 0 ? t("play.evolveUsed", { n: used }) : undefined
    body = (
      <div className="bp-stack">
        {Array.from({ length: Math.min(4, count) }, (_, i) => (
          <div key={i} className="bp-stack-layer" style={{ "--i": i } as CSSProperties}>
            <CardBack />
          </div>
        ))}
      </div>
    )
  } else if (pile === "cemetery") {
    label = t("play.zone.cemetery")
    Icon = Skull
    count = seat.cemetery.count
    body = <TopCard pile={seat.cemetery} banished={false} />
  } else {
    label = t("play.zone.banish")
    Icon = Ban
    count = seat.banish.count
    body = <TopCard pile={seat.banish} banished />
  }
  return (
    <div
      className={classes("bp-pile", `bp-pile-${pile}`, opp && "is-opp-side")}
      style={px(mat.onBoard(mat.pile(pile, opp), opp), u)}
      data-hit-area=""
      role="group"
      aria-label={label}
    >
      <div className="bp-card-inner">{body}</div>
      {count !== undefined ? (
        <span className="bp-pile-tag">
          {/* A small pile has no room for its name: an icon stands in, as on the player plates. */}
          {mat.stacked && Icon !== undefined ? (
            <Icon className="bp-pile-icon" aria-hidden="true" />
          ) : (
            label
          )}
          <b>{count}</b>
          {extra !== undefined ? <i>{extra}</i> : null}
        </span>
      ) : null}
    </div>
  )
}

/** EP and SEP as the markers a player keeps beside the evolve deck; a spent one stays, faded. */
function PointTokens({ seat, opp, u, mat }: Omit<MatProps, "leader">) {
  const { t } = useTranslation()
  const { ep, sep } = seat
  const locked = sep.opensIn > 0
  return (
    <div
      className={opp ? "bp-ep bp-pt-spot bp-ep-opp" : "bp-ep bp-pt-spot"}
      style={px(mat.onBoard(mat.points(opp), opp), u)}
      data-hit-area=""
    >
      <span
        className="bp-ep-group"
        data-hit-area=""
        role="img"
        aria-label={t("play.stat.pointsOf", {
          label: t("play.stat.ep"),
          n: ep.current,
          max: ep.max,
        })}
      >
        <i className="bp-pt-label">{t("play.stat.ep")}</i>
        {Array.from({ length: ep.max }, (_, i) => (
          <img
            key={i}
            className={i < ep.current ? "bp-ep-icon" : "bp-ep-icon is-used"}
            src={epIcon}
            alt=""
          />
        ))}
        {ep.max === 0 ? (
          <em className="bp-pt-none">
            {seat.first === true ? t("play.stat.noneFirst") : t("play.stat.none")}
          </em>
        ) : null}
      </span>
      <span
        className={locked ? "bp-ep-group is-locked" : "bp-ep-group"}
        data-hit-area=""
        role="img"
        aria-label={
          locked
            ? t("play.stat.sepLocked", { n: sep.opensIn })
            : t("play.stat.pointsOf", { label: t("play.stat.sep"), n: sep.current, max: sep.max })
        }
        title={locked ? t("play.stat.sepLocked", { n: sep.opensIn }) : t("play.stat.sepOpen")}
      >
        <i className="bp-pt-label is-sep">{t("play.stat.sep")}</i>
        {Array.from({ length: sep.max }, (_, i) => (
          <img
            key={i}
            className={i < sep.current ? "bp-sep-icon" : "bp-sep-icon is-used"}
            src={sepIcon}
            alt=""
          />
        ))}
        {sep.max === 0 ? <em className="bp-pt-none">{t("play.stat.none")}</em> : null}
        {locked ? <b className="bp-sep-count">{sep.opensIn}</b> : null}
      </span>
    </div>
  )
}

/**
 * The PP column printed on the mat: blue = this turn's maximum, green = what is left. Each column
 * is lit from 0 up to its value, so a seat with nothing left still has its 0 lit.
 */
function PpColumn({ seat, opp, u, mat }: Omit<MatProps, "leader">) {
  const { t } = useTranslation()
  const r = mat.onBoard(mat.pp(opp), opp)
  // The viewer's own column counts up towards the middle of the table, like the mat across it.
  const rows = Array.from({ length: 11 }, (_, i) => (opp ? i : 10 - i))
  const head = 40
  const rowH = (r.h - head - 4) / 11
  return (
    <div
      className={opp ? "bp-pp bp-pp-opp" : "bp-pp"}
      style={px(r, u)}
      data-hit-area=""
      role="img"
      aria-label={t("play.stat.ppOf", { left: seat.pp, max: seat.maxPp })}
    >
      <div className="bp-pp-head" style={{ height: head * u, order: opp ? 2 : 0 }}>
        <b>{t("play.stat.pp")}</b>
        <span>
          <i className="is-max">{t("play.stat.ppMax")}</i>
          <i className="is-left">{t("play.stat.ppLeft")}</i>
        </span>
      </div>
      {rows.map((n) => (
        <div key={n} className="bp-pp-row" style={{ height: rowH * u }}>
          <span
            className={classes(
              "bp-pp-dot is-max",
              n <= seat.maxPp && "is-on",
              n === seat.maxPp && "is-mark",
            )}
          >
            {n}
          </span>
          <span
            className={classes(
              "bp-pp-dot is-left",
              n <= seat.pp && "is-on",
              n === seat.pp && "is-mark",
            )}
          >
            {n}
          </span>
        </div>
      ))}
    </div>
  )
}

/** PP on a stacked mat: ten pips in one small box, green = left, blue = spent this turn. */
function PpBar({ seat, opp, u, mat }: Omit<MatProps, "leader">) {
  const { t } = useTranslation()
  return (
    <div
      className="bp-pp-bar"
      style={px(mat.onBoard(mat.pp(opp), opp), u)}
      role="img"
      aria-label={t("play.stat.ppOf", { left: seat.pp, max: seat.maxPp })}
    >
      <b>
        {t("play.stat.pp")} {seat.pp}/{seat.maxPp}
      </b>
      <span className="bp-pp-pips">
        {Array.from({ length: 10 }, (_, i) => (
          <i
            key={i}
            className={classes("bp-pp-pip", i < seat.pp ? "is-left" : i < seat.maxPp && "is-max")}
          />
        ))}
      </span>
    </div>
  )
}

function Mat({ seat, leader, opp, u, mat, shared }: MatProps & { readonly shared: boolean }) {
  const { t } = useTranslation()
  return (
    <section aria-label={opp ? t("play.mat.opponent") : t("play.mat.self")}>
      <div
        className={shared ? "bp-mat is-frames" : "bp-mat"}
        style={{
          left: 0,
          top: (opp ? 0 : mat.matH + GAP) * u,
          width: mat.matW * u,
          height: mat.matH * u,
        }}
      >
        {!shared ? <MatPicture opp={opp} mat={mat} /> : null}
        <div className="bp-frames-wrap" style={{ opacity: 0.7 }}>
          <ZoneFrames opp={opp} mat={mat} />
        </div>
      </div>
      {seat.ex.map((card, slot) =>
        card === null ? null : (
          <ExCard key={card.handle} card={card} slot={slot} opp={opp} u={u} mat={mat} />
        ),
      )}
      {seat.field.map((card, slot) =>
        card === null ? null : (
          <FieldCardView key={card.handle} card={card} slot={slot} opp={opp} u={u} mat={mat} />
        ),
      )}
      {PILES.map((pile) => (
        <PileView key={pile} pile={pile} seat={seat} leader={leader} opp={opp} u={u} mat={mat} />
      ))}
      <PointTokens seat={seat} opp={opp} u={u} mat={mat} />
      {mat.stacked ? (
        <PpBar seat={seat} opp={opp} u={u} mat={mat} />
      ) : (
        <PpColumn seat={seat} opp={opp} u={u} mat={mat} />
      )}
    </section>
  )
}

/**
 * The flat table for one seat: both mats on the board the layout gives it. It draws the model it
 * is given and knows nothing about questions, so another renderer can take its place.
 */
export function Board({
  model,
  leaders,
  layout,
  mat,
}: {
  readonly model: TableModel
  readonly leaders: Readonly<Record<Seat, CardRef>>
  readonly layout: Layout
  readonly mat: BattleMat
}) {
  const { t } = useTranslation()
  const { u } = layout
  const geometry = matGeometry(layout.matW, layout.stacked)
  const shared = mat === "shared"
  return (
    // Dice hang off the corners of their cards and a turned card is wider than its slot, all on
    // purpose: the overflow audit leaves boxes marked `data-hit-area` alone.
    <section
      className="bp-stage"
      data-hit-area=""
      aria-label={t("play.table")}
      style={
        {
          left: layout.boardLeft,
          top: layout.boardTop,
          width: layout.matW * u,
          height: layout.boardH * u,
          "--u": `${String(u)}px`,
        } as CSSProperties
      }
    >
      <div className="bp-board" data-hit-area="">
        {shared ? (
          <div
            className="bp-mat"
            style={{ left: 0, top: 0, width: layout.matW * u, height: layout.boardH * u }}
          >
            <SharedMatPicture mat={geometry} boardH={layout.boardH} />
          </div>
        ) : null}
        <Mat
          seat={model.opponent}
          leader={leaders[model.opponent.seat]}
          opp
          u={u}
          mat={geometry}
          shared={shared}
        />
        <Mat
          seat={model.self}
          leader={leaders[model.self.seat]}
          opp={false}
          u={u}
          mat={geometry}
          shared={shared}
        />
        <div className="bp-centerline" style={{ top: geometry.matH * u, height: GAP * u }} />
      </div>
    </section>
  )
}
