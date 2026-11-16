// Where everything on the battle screen goes for a given window. The board keeps its proportions
// and takes what is left after the panels around it, so the layout is worked out here in one place
// and the parts are placed by it.
import { useEffect, useState } from "react"

import { boardSize, CARD_H, CARD_W } from "./table/geometry"

type HandMode = "row" | "column"
type PlateMode = "right" | "side" | "top"

export interface Layout {
  readonly vw: number
  readonly vh: number
  /** One board unit in pixels. */
  readonly u: number
  readonly matW: number
  readonly boardLeft: number
  readonly boardTop: number
  readonly portrait: boolean
  readonly compact: boolean
  readonly narrow: boolean
  /** A phone held upright: too narrow for the mat's side columns, so the piles move out of them. */
  readonly stacked: boolean
  readonly boardH: number
  readonly sidebar: "open" | "closed" | "none"
  readonly sidebarOverlay: boolean
  readonly sidebarW: number
  readonly log: "column" | "none"
  readonly logW: number
  readonly hand: HandMode
  readonly handH: number
  readonly handReserve: number
  readonly handArea: {
    readonly left: number
    readonly right: number
    readonly top: number
    readonly bottom: number
  }
  readonly plates: PlateMode
  readonly sideColW: number
  readonly topH: number
  readonly oppH: number
  /** Both seats side by side; this layout is one half. */
  readonly dual: boolean
}

export function useViewport(): { readonly vw: number; readonly vh: number } {
  const [size, setSize] = useState(() => ({ vw: window.innerWidth, vh: window.innerHeight }))
  useEffect(() => {
    const on = () => {
      setSize({ vw: window.innerWidth, vh: window.innerHeight })
    }
    window.addEventListener("resize", on)
    return () => {
      window.removeEventListener("resize", on)
    }
  }, [])
  return size
}

/** Tablets on their side: the card-info sidebar floats over the board instead of taking width. */
export const sidebarFloats = (vw: number, vh: number): boolean =>
  vh <= vw && vw >= 1000 && vw < 1300

export function computeLayout(
  vw: number,
  vh: number,
  options: { readonly sidebar: boolean; readonly dual: boolean },
): Layout {
  const { dual } = options
  // In the dual view `vw` is half the screen; orientation still follows the whole screen.
  const portrait = vh > (dual ? vw * 2 : vw)
  const compact = vh < 520 || vw < 520
  const phoneLandscape = !portrait && vh < 520
  const eligible = !portrait && vw >= 1000
  // Below 1300 px the sidebar floats over the board instead of taking width from it.
  const sidebarOverlay = eligible && vw < 1300
  const sidebar = eligible ? (options.sidebar ? "open" : "closed") : "none"
  // From 1300 px both side columns keep a fixed slot whether open or closed: card info on the
  // left (wider, for the image and effect text), the log on the right between the board and the
  // player info. Toggling a panel never moves or resizes the board.
  const wide = eligible && vw >= 1300
  const sidebarW = wide
    ? Math.round(Math.max(320, Math.min(420, vw * 0.2)))
    : Math.round(Math.max(280, Math.min(380, vw * 0.18)))
  const sidebarReserve = wide
    ? sidebarW
    : sidebarOverlay
      ? 0
      : sidebar === "open"
        ? sidebarW
        : sidebar === "closed"
          ? 30
          : 0
  const logW = wide ? Math.round(Math.max(200, Math.min(250, vw * 0.125))) : 0
  // Player info is fixed on the right (opponent top, you bottom), or a strip on top when upright.
  const plates: PlateMode = portrait ? "top" : phoneLandscape ? "side" : "right"
  const narrow = plates === "top" && vw < 1000
  const stacked = portrait && vw < 520
  const topH = plates === "top" ? (narrow ? 150 : compact ? 44 : 58) : 0
  const hand: HandMode = phoneLandscape ? "column" : "row"
  const sideColW = dual
    ? 0
    : phoneLandscape
      ? 150
      : plates === "right"
        ? vw < 1600
          ? 168
          : 196
        : 0
  const handColW = phoneLandscape ? 176 : 0
  // Row hand: the whole card stays visible, so it gets its own strip under the board.
  const handH =
    hand === "row"
      ? Math.round(Math.max(92, Math.min(portrait ? 150 : 160, vh * (portrait ? 0.13 : 0.15))))
      : Math.round((handColW - 18) / 2 / (CARD_W / CARD_H))
  const handReserve = hand === "row" ? handH + 22 : 0
  // The opponent's hand gets its own strip above the board (about 60% of a field card).
  // Stacked, the board is what runs out of height: the strip takes the least it can.
  const oppH = stacked ? 46 : Math.round(Math.max(46, Math.min(84, vh * 0.08)))
  const oppReserve = hand === "row" ? oppH + 10 : 0
  const x0 = sidebarReserve + handColW + 8
  const x1 = vw - (plates === "right" || plates === "side" ? sideColW : 0) - logW - 8
  const y0 = topH + oppReserve + 4
  const y1 = vh - handReserve - 4
  const availW = x1 - x0
  const availH = y1 - y0
  const size = boardSize(stacked)
  const u = Math.min(availW / size.matWMin, availH / size.boardH)
  const matW = Math.max(size.matWMin, Math.min(size.matWMax, availW / u))
  return {
    vw,
    vh,
    u,
    matW,
    boardLeft: x0 + (availW - matW * u) / 2,
    boardTop: y0 + (availH - size.boardH * u) / 2,
    portrait,
    compact,
    narrow,
    stacked,
    boardH: size.boardH,
    sidebar,
    sidebarOverlay,
    sidebarW,
    log: wide ? "column" : "none",
    logW,
    hand,
    handH,
    handReserve,
    handArea:
      hand === "row"
        ? {
            left: sidebarReserve + 8,
            right: wide ? x1 : vw - 8,
            top: vh - handReserve,
            bottom: vh - 4,
          }
        : { left: 4, right: handColW, top: 4, bottom: vh - 4 },
    plates,
    sideColW,
    topH,
    oppH,
    dual,
  }
}
