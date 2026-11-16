import "./battle.css"

import { ChevronRight, ScrollText } from "lucide-react"
import { type CSSProperties, type ReactNode, useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"

import type { CardRef, Seat } from "../../battle/contract"
import type { BattleSource } from "../../battle/source"
import { inspect, type ZoneName } from "../../battle/view/inspect"
import { type AttackHints, type SlotArrangement, tableModel } from "../../battle/view/table"
import { cn } from "../../components/ui/cn"
import { prefsStore, usePrefs } from "../../settings"
import { BattleCardsContext, useBattleCards } from "./battle-cards"
import { CARD_FRAMING } from "./hud/mat-crop"
import { PileBrowser } from "./hud/PileBrowser"
import { Plate } from "./hud/Plate"
import { SelectionBar } from "./hud/SelectionBar"
import { SettingsButton } from "./hud/Settings"
import { type Shown, Sidebar } from "./hud/Sidebar"
import { TurnPill } from "./hud/TurnPill"
import { Zoom } from "./hud/Zoom"
import {
  type BrowsablePile,
  type Inspector,
  InspectorContext,
  NO_TARGETS,
  type Picked,
  PIN_KEY,
  samePick,
  type Targets,
  type Zoomed,
} from "./inspector"
import { computeLayout, type Layout, matBox, sidebarFloats, useViewport } from "./layout"
import { useMatUpload } from "./mat-upload"
import { Board } from "./table/Board"
import { DualHand, Hand, OppHand } from "./table/Hand"
import { MatArt, type MatPhoto } from "./table/mats"
import { useBattle } from "./use-battle"

type Leaders = Readonly<Record<Seat, CardRef>>

/** The zones the screen has a name for; a card anywhere else is described by its side alone. */
const ZONE_LABEL: Partial<
  Record<ZoneName, "hand" | "field" | "ex" | "deck" | "evolveDeck" | "cemetery" | "banish">
> = {
  hand: "hand",
  field: "field",
  ex: "ex",
  deck: "deck",
  evolve_deck: "evolveDeck",
  cemetery: "cemetery",
  banish: "banish",
}

/**
 * The mat's picture, as the player set it: under the whole table or the board only. It is drawn
 * once for both mats, and is this player's own; the other player never sees it.
 */
function MatSurface({ layout, leader }: { readonly layout: Layout; readonly leader: CardRef }) {
  const prefs = usePrefs()
  const upload = useMatUpload()
  const box = matBox(layout, prefs.battleMatReach)
  const cardFraming = prefs.battleMatCardFraming ?? CARD_FRAMING
  // A picture stays where it was framed, whichever box looks at it now.
  const photo: MatPhoto | null =
    prefs.battleMatLook === "card"
      ? {
          kind: "card",
          // Until the player picks a card: their own leader's.
          card:
            prefs.battleMatCard === null ? leader : { printing_id: prefs.battleMatCard, face: 0 },
          framing: cardFraming,
          anchor: matBox(layout, cardFraming.reach),
        }
      : prefs.battleMatLook === "upload" && upload !== null
        ? {
            kind: "upload",
            url: upload.url,
            size: upload.size,
            framing: upload.framing,
            anchor: matBox(layout, upload.framing.reach),
          }
        : null
  return (
    <MatArt
      look={prefs.battleMatLook}
      photo={photo}
      blur={prefs.battleMatBlur}
      dim={prefs.battleMatDim}
      box={box}
      u={layout.u}
      // The picture's centre piece sits on the line between the two mats.
      centre={(layout.boardTop + (layout.boardH * layout.u) / 2 - box.top) / box.height}
      framed={prefs.battleMatReach === "board"}
    />
  )
}

/** The other seat's screen in the dual view: its own table and hands, fed by its own source. */
function OppositePane({
  source,
  leaders,
  slots,
  attackers,
  layout,
  light,
}: {
  readonly source: BattleSource
  readonly leaders: Leaders
  readonly slots: SlotArrangement | undefined
  readonly attackers: AttackHints | undefined
  readonly layout: Layout
  readonly light: boolean
}) {
  const { t } = useTranslation()
  const battle = useBattle(source)
  const model = useMemo(
    () => (battle.board === null ? null : tableModel(battle.board, slots, attackers)),
    [battle.board, slots, attackers],
  )
  return (
    <section
      className="bp-dual-pane"
      style={{ left: layout.vw, width: layout.vw, height: layout.vh }}
      aria-label={t("play.dev.opposite")}
    >
      {model !== null && (
        // Only shown: nothing on this half can be picked.
        <InspectorContext value={null}>
          <MatSurface layout={layout} leader={leaders[model.self.seat]} />
          <Board model={model} leaders={leaders} layout={layout} light={light} />
          <DualHand hand={model.self.hand} backs={model.opponent.hand.count} layout={layout} />
          <div className="bp-dual-tag">{t("play.dev.opposite")}</div>
          <div className="bp-dual-side">
            <TurnPill model={model} />
          </div>
        </InspectorContext>
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
  slots,
  attackers,
  corner,
  onDual,
}: {
  readonly source: BattleSource
  /** The other seat's source: both screens side by side, to compare what each player is told. */
  readonly opposite?: BattleSource | undefined
  /** Each seat's leader card, from the match setup. */
  readonly leaders: Leaders
  /** Where the field cards lie on this screen, when something other than their order decides it. */
  readonly slots?: SlotArrangement | undefined
  /** Which field cards may attack now; see `AttackHints`. */
  readonly attackers?: AttackHints | undefined
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
  const [picked, setPicked] = useState<Picked | null>(null)
  const [hovered, setHovered] = useState<Picked | null>(null)
  const [zoomed, setZoomed] = useState<Zoomed | null>(null)
  const [browsing, setBrowsing] = useState<{ seat: Seat; pile: BrowsablePile } | null>(null)
  // Without an info column the selection shows in a bar, which can be put away.
  const [barOpen, setBarOpen] = useState(false)
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
    () => (battle.board === null ? null : tableModel(battle.board, slots, attackers)),
    [battle.board, slots, attackers],
  )
  const pinned = prefs.battleInfoPinned
  // Parchment is the one light picture: the mats print on it in a dark ink.
  const light = prefs.battleMatLook === "parchment"
  // The pin's key works from anywhere on the screen, except while something is being typed.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== PIN_KEY) return
      // Otherwise the browser opens its help page.
      event.preventDefault()
      const focused = document.activeElement
      if (focused instanceof HTMLElement && focused.closest("input, textarea, select")) return
      prefsStore.set({ battleInfoPinned: !prefsStore.get().battleInfoPinned })
    }
    window.addEventListener("keydown", onKey)
    return () => {
      window.removeEventListener("keydown", onKey)
    }
  }, [])
  // Picking a card that may attack lights up what it may attack.
  const targets = useMemo<Targets>(() => {
    if (model === null || picked?.kind !== "object") return NO_TARGETS
    const attack = model.self.field.find((card) => card?.handle === picked.handle)?.attack ?? null
    return attack === null
      ? NO_TARGETS
      : { leader: attack.leader ? model.opponent.seat : null, objects: attack.targets }
  }, [model, picked])
  const inspector = useMemo<Inspector>(
    () => ({
      picked,
      targets,
      select: (next) => {
        setPicked(next)
        // The card shows in the info column, which opens itself; elsewhere in the selection bar.
        setSidebarOpen(true)
        setFloatOpen(true)
        setBarOpen(true)
      },
      preview: setHovered,
      zoom: setZoomed,
      browse: (seat, pile) => {
        setBrowsing({ seat, pile })
      },
    }),
    [picked, targets],
  )
  const rootClass = cn(
    "bp-root",
    layout.plates === "top" && "plates-top",
    layout.hand === "column" && "hand-column",
    layout.compact && "is-compact",
    layout.narrow && "is-narrow",
    layout.stacked && "is-stacked",
    dual && "is-dual",
  )
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
  const board = battle.board
  const describe = (target: Picked, preview: boolean): Shown | null => {
    if (target.kind === "leader") {
      const own = target.seat === model.self.seat
      const life = (own ? model.self : model.opponent).life
      return {
        card: leaders[target.seat],
        where: own ? t("play.info.ownLeader", { life }) : t("play.info.otherLeader", { life }),
        state: null,
        preview,
      }
    }
    // A card that has left every zone this seat can see is no longer something to look at.
    const state = board === null ? null : inspect(board, target.handle)
    if (state === null) return null
    const side = state.seat === model.self.seat ? "self" : "opponent"
    const zone = ZONE_LABEL[state.zone]
    return {
      card: state.card.face,
      where:
        zone === undefined
          ? t(`play.side.${side}`)
          : t(`play.info.where.${side}`, { zone: t(`play.zone.${zone}`) }),
      state,
      preview,
    }
  }
  const selection = picked === null ? null : describe(picked, false)
  const ownLeader: Shown = {
    card: leaders[model.self.seat],
    where: t("play.info.ownLeader", { life: model.self.life }),
    state: null,
    preview: false,
  }
  // Resting on the selected card itself is not a preview of anything else, and a pinned column
  // keeps its card whatever the pointer rests on.
  const previewed =
    pinned || hovered === null || (picked !== null && samePick(hovered, picked))
      ? null
      : describe(hovered, true)
  const shown = previewed ?? selection ?? ownLeader
  // Without an info column the selection shows in a bar over the board.
  const bar = layout.sidebar === "none" && barOpen ? selection : null
  const zoomOf = (target: Shown): Zoomed => ({
    face: target.card,
    under: target.state?.under ?? null,
  })
  const browsed =
    browsing === null ? null : browsing.seat === model.self.seat ? model.self : model.opponent
  // The shape an uploaded picture is cut to: the box the mat's picture fills right now.
  const picture = matBox(layout, prefs.battleMatReach)
  const matAspect = picture.width / picture.height
  return (
    <BattleCardsContext value={cards}>
      <InspectorContext value={inspector}>
        <div className={rootClass} style={rootStyle}>
          <MatSurface layout={layout} leader={leaders[model.self.seat]} />
          <Board model={model} leaders={leaders} layout={layout} light={light} />
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
                <SettingsButton leader={leaders[model.self.seat]} matAspect={matAspect} />
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
                <SettingsButton leader={leaders[model.self.seat]} matAspect={matAspect} />
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
                <SettingsButton leader={leaders[model.self.seat]} matAspect={matAspect} />
              </div>
            </>
          ) : null}
          {layout.plates === "top" ? (
            <header className="bp-topbar">
              {opponentPlate}
              {turn}
              {selfPlate}
              <div className="bp-col-tools">
                <SettingsButton leader={leaders[model.self.seat]} matAspect={matAspect} />
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
              shown={shown}
              width={layout.sidebarW}
              overlay={layout.sidebarOverlay}
              pinned={pinned}
              onPin={() => {
                prefsStore.set({ battleInfoPinned: !pinned })
              }}
              onZoom={() => {
                setZoomed(zoomOf(shown))
              }}
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
          {bar !== null ? (
            <SelectionBar
              shown={bar}
              onZoom={() => {
                setZoomed(zoomOf(bar))
              }}
              onClose={() => {
                setBarOpen(false)
              }}
            />
          ) : null}

          {dual ? (
            <>
              <OppositePane
                source={opposite}
                leaders={leaders}
                slots={slots}
                attackers={attackers}
                layout={layout}
                light={light}
              />
              <div className="bp-dual-divider" style={{ left: layout.vw }} aria-hidden="true" />
            </>
          ) : null}
          {/* Stacked, upright is a layout of its own: nothing to suggest. The selection bar takes the
              same place, so the suggestion waits while a card is being looked at. */}
          {layout.portrait && !layout.stacked && !portraitDismissed && bar === null ? (
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
          {browsing !== null && browsed !== null ? (
            <PileBrowser
              seat={browsed}
              pile={browsing.pile}
              own={browsing.seat === model.self.seat}
              onZoom={setZoomed}
              onClose={() => {
                setBrowsing(null)
              }}
            />
          ) : null}
          {zoomed !== null ? (
            <Zoom
              zoomed={zoomed}
              onClose={() => {
                setZoomed(null)
              }}
            />
          ) : null}
        </div>
      </InspectorContext>
    </BattleCardsContext>
  )
}
