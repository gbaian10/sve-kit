// Board geometry in board units. One player's mat follows the official playmat arrangement:
// right column leader / deck / cemetery, left column evolve deck / banish, five field slots in the
// upper middle and five EX-area slots in the lower middle, a divider between them. The slot rows
// are centred on the mat, each slot mirroring another across its middle. The opponent's
// mat is the same mat turned 180°, so both leaders sit next to the centre line.
//
// The mat height is fixed; its width stretches with the window (piles stay at the edges, the
// slot rows spread out), so a wide screen gets bigger cards instead of empty side bands.
//
// A phone held upright is too narrow for that: there the piles leave the side columns for a strip
// under the two rows, smaller than the cards in play, and the slots take the whole width.

export const CARD_W = 100
export const CARD_H = 140
export const GAP = 16

export interface Rect {
  readonly x: number
  readonly y: number
  readonly w: number
  readonly h: number
}

export type PileName = "leader" | "deck" | "cemetery" | "evolve" | "banish"
type Row = "field" | "ex"

/** What the layout needs to size a board: the width range of a mat and the board's height. */
export interface BoardSize {
  readonly matWMin: number
  readonly matWMax: number
  readonly boardH: number
}

const MAT_H = 456
/** The PP column at the far right of each mat: two columns of 0–10 circles (max / remaining). */
const PP_W = 72
const STACKED_MAT_H = 466
/** Piles in the strip of a stacked mat: six tenths of a card. */
const SMALL_W = 60
const SMALL_H = 84

export const boardSize = (stacked: boolean): BoardSize =>
  stacked
    ? { matWMin: 620, matWMax: 760, boardH: STACKED_MAT_H * 2 + GAP }
    : // At its narrowest a slot is still wider than an engaged card.
      { matWMin: 1160, matWMax: 1600, boardH: MAT_H * 2 + GAP }

export interface MatGeometry {
  readonly matW: number
  readonly matH: number
  /** The piles sit in a strip under the rows instead of beside them. */
  readonly stacked: boolean
  /** Where the line between the field and the EX area runs, from the owner's top edge. */
  readonly dividerY: number
  readonly pile: (pile: PileName, opp: boolean) => Rect
  /** The slot frame: wide enough for an engaged card, centred in its pitch. Slot 0 = owner's leftmost. */
  readonly slotFrame: (row: Row, slot: number, opp: boolean) => Rect
  readonly slotCard: (row: Row, slot: number, opp: boolean) => Rect
  readonly rowFrame: (row: Row, opp: boolean) => Rect
  /** EP and SEP share one spot beside the evolve deck (tournament layout). */
  readonly points: (opp: boolean) => Rect
  readonly pp: (opp: boolean) => Rect
  /** Mat-local → board units. The opponent's mat is on top. */
  readonly onBoard: (r: Rect, opp: boolean) => Rect
}

/** The rectangles of one mat that is `matW` units wide. */
export function matGeometry(matW: number, stacked: boolean): MatGeometry {
  const matH = stacked ? STACKED_MAT_H : MAT_H
  /** Turned 180° inside the mat for the opponent. */
  const turn = (r: Rect, opp: boolean): Rect =>
    opp ? { x: matW - r.x - r.w, y: matH - r.y - r.h, w: r.w, h: r.h } : r
  const card = (x: number, y: number): Rect => ({ x, y, w: CARD_W, h: CARD_H })
  const small = (x: number): Rect => ({ x, y: 364, w: SMALL_W, h: SMALL_H })
  const right = matW - 12 - PP_W - 10 - CARD_W
  // The slot rows are centred on the mat, so that the slots mirror each other across its middle:
  // the wider side column (leader, deck, cemetery and the PP column) sets the margin on both
  // sides. The narrower column on the left stands in the middle of its margin.
  const margin = stacked ? 12 : matW - right + 20
  const left = (margin - 20 - CARD_W) / 2
  const piles: Record<PileName, Rect> = stacked
    ? {
        leader: small(matW - 66),
        deck: small(matW - 152),
        cemetery: small(matW - 238),
        evolve: small(114),
        banish: small(200),
      }
    : {
        leader: card(right, 6),
        deck: card(right, 158),
        cemetery: card(right, 310),
        evolve: card(left, 158),
        banish: card(left, 310),
      }
  const rowY: Record<Row, number> = stacked ? { field: 28, ex: 208 } : { field: 76, ex: 310 }
  const rowX0 = margin
  const pitch = (matW - 2 * margin) / 5
  const slotFrame = (row: Row, slot: number, opp: boolean): Rect => {
    const w = Math.min(pitch - 4, CARD_H + 14)
    return turn({ x: rowX0 + slot * pitch + (pitch - w) / 2, y: rowY[row], w, h: CARD_H }, opp)
  }
  return {
    matW,
    matH,
    stacked,
    dividerY: stacked ? 180 : 266,
    pile: (pile, opp) => turn(piles[pile], opp),
    slotFrame,
    slotCard: (row, slot, opp) => {
      const frame = slotFrame(row, slot, opp)
      return { x: frame.x + (frame.w - CARD_W) / 2, y: frame.y, w: CARD_W, h: CARD_H }
    },
    rowFrame: (row, opp) =>
      turn({ x: rowX0 - 6, y: rowY[row] - 22, w: pitch * 5 + 12, h: CARD_H + 28 }, opp),
    points: (opp) =>
      turn(
        stacked ? { x: 6, y: 364, w: 100, h: SMALL_H } : { x: left, y: 50, w: CARD_W, h: 88 },
        opp,
      ),
    pp: (opp) =>
      turn(
        stacked
          ? { x: 270, y: 364, w: matW - 518, h: SMALL_H }
          : { x: matW - 8 - PP_W, y: 6, w: PP_W, h: MAT_H - 12 },
        opp,
      ),
    onBoard: (r, opp) => ({ ...r, y: r.y + (opp ? 0 : matH + GAP) }),
  }
}

/** A board rectangle as the pixel box of an absolutely placed element. */
export const px = (
  r: Rect,
  u: number,
): { left: number; top: number; width: number; height: number } => ({
  left: r.x * u,
  top: r.y * u,
  width: r.w * u,
  height: r.h * u,
})
