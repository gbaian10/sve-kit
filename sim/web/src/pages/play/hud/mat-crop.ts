// Framing the player's own mat picture. The whole picture is kept, with where the player put the
// frame on it; the table then looks at the picture through whichever box the mat's picture fills.
// Switching between the board and the whole table therefore never cuts the picture again: the
// picture stays where it is on screen and the larger box simply shows more of it.
import type { BattleMatFraming, BattleMatReach } from "../../../settings"

export interface Size {
  readonly w: number
  readonly h: number
}

/** Where the frame lies in the picture's free room, each from 0 (left, top) to 1. */
export interface Pan {
  readonly x: number
  readonly y: number
}

export interface CropRect extends Size {
  readonly x: number
  readonly y: number
}

/** A box on screen, in pixels. */
export interface Box {
  readonly left: number
  readonly top: number
  readonly width: number
  readonly height: number
}

export type Framing = BattleMatFraming

/** The art on a card, as a picture of its own: the art crop of the large card image, 4:3. */
export const CARD_ART: Size = { w: 384, h: 288 }

/**
 * A card's art as it lies on the mat until the player frames it: all of its width, a little above
 * its middle (where faces mostly are), on the board.
 */
export const CARD_FRAMING: Framing = { cx: 0.5, cy: 0.42, span: 1, aspect: 4 / 3, reach: "board" }

export const MAX_ZOOM = 4
/** A picture is kept no longer than this on its long side: more than any screen shows. */
const MAX_SIDE = 4096

const clamp = (value: number, low: number, high: number): number =>
  Math.min(high, Math.max(low, value))

/**
 * The frame on a picture: `aspect` (width over height) wide; at zoom 1 the largest such frame the
 * picture holds, at zoom z one z times smaller, placed by `pan`.
 */
export function cropRect(natural: Size, aspect: number, zoom: number, pan: Pan): CropRect {
  const z = clamp(zoom, 1, MAX_ZOOM)
  const fitW = Math.min(natural.w, natural.h * aspect)
  const w = fitW / z
  const h = w / aspect
  return {
    x: (natural.w - w) * clamp(pan.x, 0, 1),
    y: (natural.h - h) * clamp(pan.y, 0, 1),
    w,
    h,
  }
}

/**
 * The pan after the frame was dragged by (dx, dy) screen pixels over a picture shown `shownW`
 * pixels wide: the frame follows the pointer and stops at the picture's edges.
 */
export function moveFrame(
  natural: Size,
  rect: CropRect,
  pan: Pan,
  shownW: number,
  dx: number,
  dy: number,
): Pan {
  // One screen pixel is this many picture pixels.
  const scale = natural.w / shownW
  const roomX = natural.w - rect.w
  const roomY = natural.h - rect.h
  return {
    x: roomX <= 0 ? pan.x : clamp(pan.x + (dx * scale) / roomX, 0, 1),
    y: roomY <= 0 ? pan.y : clamp(pan.y + (dy * scale) / roomY, 0, 1),
  }
}

/** A frame as it is kept: by shares of the picture, so that it outlives a resized copy. */
export function framingOf(
  natural: Size,
  rect: CropRect,
  aspect: number,
  reach: BattleMatReach,
): Framing {
  return {
    cx: (rect.x + rect.w / 2) / natural.w,
    cy: (rect.y + rect.h / 2) / natural.h,
    span: rect.w / natural.w,
    aspect,
    reach,
  }
}

/** The zoom and pan that put a frame of `aspect` back around a kept framing's centre. */
export function frameFrom(
  natural: Size,
  framing: Framing,
  aspect: number,
): { readonly zoom: number; readonly pan: Pan } {
  const fitW = Math.min(natural.w, natural.h * aspect)
  const zoom = clamp(fitW / (framing.span * natural.w), 1, MAX_ZOOM)
  const rect = cropRect(natural, aspect, zoom, { x: 0, y: 0 })
  const roomX = natural.w - rect.w
  const roomY = natural.h - rect.h
  return {
    zoom,
    pan: {
      x: roomX <= 0 ? 0.5 : clamp((framing.cx * natural.w - rect.w / 2) / roomX, 0, 1),
      y: roomY <= 0 ? 0.5 : clamp((framing.cy * natural.h - rect.h / 2) / roomY, 0, 1),
    },
  }
}

/**
 * Where the whole picture lies, relative to the box the mat's picture fills now. `anchor` is the
 * box of the reach the picture was framed for, as the screen is laid out now: the frame covers
 * it, centre on centre. Another reach looks at the same picture in the same place, so a larger
 * box shows more of it. Only where the picture ends before the box does is it moved, and then
 * enlarged, just enough to cover the box.
 */
export function placePicture(natural: Size, framing: Framing, anchor: Box, box: Box): Box {
  const frameW = framing.span * natural.w
  const frameH = frameW / framing.aspect
  const scale = Math.max(
    anchor.width / frameW,
    anchor.height / frameH,
    box.width / natural.w,
    box.height / natural.h,
  )
  const width = natural.w * scale
  const height = natural.h * scale
  const left = anchor.left + anchor.width / 2 - framing.cx * width
  const top = anchor.top + anchor.height / 2 - framing.cy * height
  return {
    left: clamp(left, box.left + box.width - width, box.left) - box.left,
    top: clamp(top, box.top + box.height - height, box.top) - box.top,
    width,
    height,
  }
}

/** The size a picture is kept at: its own, unless its long side is longer than any screen needs. */
export function keptSize(natural: Size): Size {
  const long = Math.max(natural.w, natural.h)
  if (long <= MAX_SIDE) return natural
  const scale = MAX_SIDE / long
  return {
    w: Math.max(1, Math.round(natural.w * scale)),
    h: Math.max(1, Math.round(natural.h * scale)),
  }
}
