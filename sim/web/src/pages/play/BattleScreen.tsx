import "./battle.css"

import { ChevronRight, ScrollText } from "lucide-react"
import { type CSSProperties, type ReactNode, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"

import type { CardRef, Seat } from "../../battle/contract"
import type { BattleSource } from "../../battle/source"
import { type ProvisionalPoints, type SlotArrangement, tableModel } from "../../battle/view/table"
import { type BattleMat, usePrefs } from "../../settings"
import { BattleCardsContext, useBattleCards } from "./battle-cards"
import { Plate } from "./hud/Plate"
import { SettingsButton } from "./hud/Settings"
import { Sidebar } from "./hud/Sidebar"
import { TurnPill } from "./hud/TurnPill"
import { computeLayout, type Layout, sidebarFloats, useViewport } from "./layout"
import { Board } from "./table/Board"
import { ArtCrop } from "./table/Cards"
import { DualHand, Hand, OppHand } from "./table/Hand"
import { useBattle } from "./use-battle"

type Leaders = Readonly<Record<Seat, CardRef>>
/** Until the contract carries them: each seat's evolve-point maximums and super-evolution wait. */
type SeatPoints = Readonly<Record<Seat, ProvisionalPoints>>

/** The other seat's screen in the dual view: its own table and hands, fed by its own source. */
function OppositePane({
  source,
  leaders,
  points,
  slots,
  layout,
  mat,
}: {
  readonly source: BattleSource
  readonly leaders: Leaders
  readonly points: SeatPoints
  readonly slots: SlotArrangement | undefined
  readonly layout: Layout
  readonly mat: BattleMat
}) {
  const { t } = useTranslation()
  const battle = useBattle(source)
  const model = useMemo(
    () => (battle.board === null ? null : tableModel(battle.board, points, slots)),
    [battle.board, points, slots],
  )
  return (
    <section
      className="bp-dual-pane"
      style={{ left: layout.vw, width: layout.vw, height: layout.vh }}
      aria-label={t("play.dev.opposite")}
    >
      {model !== null && (
        <>
          <Board model={model} leaders={leaders} layout={layout} mat={mat} />
          <DualHand hand={model.self.hand} backs={model.opponent.hand.count} layout={layout} />
          <div className="bp-dual-tag">{t("play.dev.opposite")}</div>
          <div className="bp-dual-side">
            <TurnPill model={model} />
          </div>
        </>
      )}
    </section>
  )
}

/**
 * The battle screen of one seat, filling the window. Everything it shows comes from that seat's
 * source; with `opposite` the other seat's screen stands beside it on a source of its own, and
 * the two share nothing but the card snapshot.
 */
export function BattleScreen({
  source,
  opposite,
  leaders,
  points,
  slots,
  corner,
  onDual,
}: {
  readonly source: BattleSource
  /** The other seat's source: both screens side by side, to compare what each player is told. */
  readonly opposite?: BattleSource | undefined
  /** Each seat's leader card, from the match setup. */
  readonly leaders: Leaders
  readonly points: SeatPoints
  /** Where the field cards lie on this screen, when something other than their order decides it. */
  readonly slots?: SlotArrangement | undefined
  /** Page controls for the chip on the board's top-left corner. */
  readonly corner?: ReactNode
  /** Switches the dual view on or off, where the page offers it. */
  readonly onDual?: ((next: boolean) => void) | undefined
}) {
  const { t } = useTranslation()
  const { vw, vh } = useViewport()
  const battle = useBattle(source)
  const cards = useBattleCards(battle.ready?.identity.region)
  const prefs = usePrefs()
  const dual = opposite !== undefined
  const [sidebarOpen, setSidebarOpen] = useState(true)
  // Where the sidebar floats over the board it starts closed.
  const [floatOpen, setFloatOpen] = useState(false)
  const [portraitDismissed, setPortraitDismissed] = useState(false)
  const floats = sidebarFloats(vw, vh)
  const layout = useMemo(
    () =>
      computeLayout(dual ? vw / 2 : vw, vh, {
        sidebar: floats ? sidebarOpen && floatOpen : sidebarOpen,
        dual,
      }),
    [vw, vh, dual, floats, sidebarOpen, floatOpen],
  )
  const model = useMemo(
    () => (battle.board === null ? null : tableModel(battle.board, points, slots)),
    [battle.board, points, slots],
  )
  const rootClass = [
    "bp-root",
    `plates-${layout.plates}`,
    `hand-${layout.hand}`,
    layout.compact && "is-compact",
    layout.narrow && "is-narrow",
    layout.stacked && "is-stacked",
    dual && "is-dual",
  ]
    .filter(Boolean)
    .join(" ")
  const rootStyle = {
    "--hand-h": `${String(layout.handH)}px`,
    "--hand-reserve": `${String(layout.handReserve)}px`,
    "--top-h": `${String(layout.topH)}px`,
  } as CSSProperties

  if (battle.stopped !== null || model === null)
    return (
      <div className={rootClass} style={rootStyle}>
        {battle.stopped !== null ? (
          <p role="alert" className="bp-notice">
            {t("play.stopped")}
          </p>
        ) : (
          <p role="status" className="bp-notice">
            {t("play.loading")}
          </p>
        )}
      </div>
    )

  const opponentPlate = (
    <Plate
      seat={model.opponent}
      leader={leaders[model.opponent.seat]}
      self={false}
      compact={layout.compact}
    />
  )
  const selfPlate = (
    <Plate seat={model.self} leader={leaders[model.self.seat]} self compact={layout.compact} />
  )
  const turn = <TurnPill model={model} />
  const colBottom = layout.handReserve + 8
  return (
    <BattleCardsContext value={cards}>
      <div className={rootClass} style={rootStyle}>
        {/* The page's background is always the two leaders' art; the mat setting changes the board only. */}
        <div className="bp-leader-art" aria-hidden="true" data-hit-area="">
          <div className="bp-leader-art-opp" data-hit-area="">
            <ArtCrop card={leaders[model.opponent.seat]} focus={30} />
          </div>
          <div className="bp-leader-art-self" data-hit-area="">
            <ArtCrop card={leaders[model.self.seat]} focus={30} />
          </div>
        </div>

        <Board model={model} leaders={leaders} layout={layout} mat={prefs.battleMat} />
        <OppHand count={model.opponent.hand.count} layout={layout} />

        {corner !== undefined && (
          <div
            className="bp-scene-chip"
            style={{ left: layout.boardLeft, top: Math.max(4, layout.boardTop - 30) }}
          >
            {corner}
          </div>
        )}

        {/* Dual view: a small tool bar and the turn on this half; the other seat's half on the right. */}
        {dual ? (
          <>
            <div className="bp-dual-hud" style={{ right: vw - layout.vw + 8 }}>
              <SettingsButton />
              {onDual !== undefined && (
                <button
                  type="button"
                  className="bp-dual-exit"
                  onClick={() => {
                    onDual(false)
                  }}
                >
                  {t("play.dev.exitDual")}
                </button>
              )}
            </div>
            <div className="bp-dual-turn" style={{ right: vw - layout.vw + 8 }}>
              {turn}
            </div>
          </>
        ) : null}
        {layout.plates === "right" && !dual ? (
          <div
            className="bp-col bp-col-right"
            style={{ right: 6, width: layout.sideColW - 8, top: 8, bottom: colBottom }}
          >
            <div className="bp-col-tools">
              <SettingsButton />
              {onDual !== undefined && (
                <button
                  type="button"
                  className="bp-dual-exit"
                  onClick={() => {
                    onDual(true)
                  }}
                  title={t("play.dev.dualHint")}
                >
                  {t("play.dev.dual")}
                </button>
              )}
            </div>
            {opponentPlate}
            <div className="bp-side-spacer" />
            {turn}
            <div className="bp-side-spacer" />
            {selfPlate}
          </div>
        ) : null}
        {layout.plates === "side" ? (
          <>
            <div className="bp-side bp-side-right is-phone" style={{ width: layout.sideColW }}>
              {opponentPlate}
              <div className="bp-side-spacer" />
              {turn}
              <div className="bp-side-spacer" />
              {selfPlate}
            </div>
            {/* The tools sit in the hand column's header, so the info column keeps its full height. */}
            <div
              className="bp-gear-wrap is-hand-col"
              style={{ left: 4, width: layout.handArea.right - 4 }}
            >
              <SettingsButton />
            </div>
          </>
        ) : null}
        {layout.plates === "top" ? (
          <header className="bp-topbar">
            {opponentPlate}
            {turn}
            {selfPlate}
            <div className="bp-col-tools">
              <SettingsButton />
            </div>
          </header>
        ) : null}

        {layout.log === "column" ? (
          <div
            className="bp-log-col"
            style={{
              left: layout.vw - layout.sideColW - layout.logW,
              width: layout.logW - 8,
              top: 8,
              bottom: 8,
            }}
          >
            <section className="bp-log" translate="no" aria-label={t("play.log.title")}>
              <header>
                <ScrollText aria-hidden="true" />
                <span>{t("play.log.title")}</span>
              </header>
              <div className="bp-log-scroll">
                <p className="bp-log-empty">{t("play.log.empty")}</p>
              </div>
            </section>
          </div>
        ) : null}

        {layout.sidebar === "open" ? (
          <Sidebar
            card={leaders[model.self.seat]}
            where={t("play.info.ownLeader", { life: model.self.life })}
            width={layout.sidebarW}
            overlay={layout.sidebarOverlay}
            onClose={() => {
              if (floats) setFloatOpen(false)
              else setSidebarOpen(false)
            }}
          />
        ) : null}
        {layout.sidebar === "closed" ? (
          <button
            type="button"
            className="bp-sidebar-tab"
            onClick={() => {
              setSidebarOpen(true)
              setFloatOpen(true)
            }}
            aria-label={t("play.info.expand")}
          >
            <ChevronRight aria-hidden="true" />
            <span>{t("play.info.title")}</span>
          </button>
        ) : null}

        <Hand hand={model.self.hand} layout={layout} />

        {dual ? (
          <>
            <OppositePane
              source={opposite}
              leaders={leaders}
              points={points}
              slots={slots}
              layout={layout}
              mat={prefs.battleMat}
            />
            <div className="bp-dual-divider" style={{ left: layout.vw }} aria-hidden="true" />
          </>
        ) : null}
        {/* Stacked, upright is a layout of its own: nothing to suggest. */}
        {layout.portrait && !layout.stacked && !portraitDismissed ? (
          <div className="bp-portrait">
            {t("play.portrait.hint")}
            <button
              type="button"
              onClick={() => {
                setPortraitDismissed(true)
              }}
            >
              {t("play.portrait.dismiss")}
            </button>
          </div>
        ) : null}
      </div>
    </BattleCardsContext>
  )
}
