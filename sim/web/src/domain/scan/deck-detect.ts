import type { Point, Quad } from "./tensor"

// A deck photo holds many cards at unknown sizes, so the card finder runs on overlapping square
// tiles at several scales. This file turns those per-tile finds into one box per card, in reading
// order. Thresholds come from the scan-lab experiments the maintainer approved in the preview.

/** Tile side as a share of the photo's short side. */
const DECK_TILE_SCALES = [0.22, 0.32, 0.45, 0.65, 1.0] as const
const MERGE_IOU = 0.35
const MIN_EDGE = 12
/** Finds at least this sure set the size of a typical card; weaker ones (print, dice) do not. */
const STRONG = 0.6
/** A whole-photo find counts only for a card too big for the tiles: long side over this share of the short side. */
const WHOLE_MIN = 0.4

export interface Tile {
  readonly x: number
  readonly y: number
  readonly side: number
}

export interface Box {
  readonly score: number
  /** Top left, top right, bottom right, bottom left as seen in the photo. */
  readonly quad: Quad
}

/** Overlapping square tiles (half-tile stride) covering a `width` × `height` photo at each scale. */
export function deckTiles(width: number, height: number): Tile[] {
  const out: Tile[] = []
  for (const scale of DECK_TILE_SCALES) {
    const side = Math.floor(Math.min(width, height) * scale)
    const stride = Math.max(1, side >> 1)
    const xs: number[] = []
    const ys: number[] = []
    for (let x = 0; x <= Math.max(0, width - side); x += stride) xs.push(x)
    for (let y = 0; y <= Math.max(0, height - side); y += stride) ys.push(y)
    if ((xs.at(-1) ?? 0) + side < width) xs.push(width - side)
    if ((ys.at(-1) ?? 0) + side < height) ys.push(height - side)
    for (const y of ys) for (const x of xs) out.push({ x, y, side })
  }
  return out
}

function area(q: readonly Point[]): number {
  let sum = 0
  for (let i = 0; i < q.length; i += 1) {
    const [x0, y0] = q[i] as Point
    const [x1, y1] = q[(i + 1) % q.length] as Point
    sum += x0 * y1 - x1 * y0
  }
  return Math.abs(sum) / 2
}

function edges(q: Quad): number[] {
  return q.map((p, i) => {
    const n = q[(i + 1) % 4] as Point
    return Math.hypot(n[0] - p[0], n[1] - p[1])
  })
}

/** A convex quad with roughly a card's proportions. */
function plausible(q: Quad): boolean {
  let sign = 0
  for (let i = 0; i < 4; i += 1) {
    const a = q[i] as Point
    const b = q[(i + 1) % 4] as Point
    const c = q[(i + 2) % 4] as Point
    const cross = Math.sign((b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]))
    if (!sign) sign = cross
    else if (cross !== sign) return false
  }
  const e = edges(q)
  const a = ((e[0] ?? 0) + (e[2] ?? 0)) / 2
  const b = ((e[1] ?? 0) + (e[3] ?? 0)) / 2
  const ratio = Math.min(a, b) / Math.max(a, b)
  return ratio > 0.45 && ratio < 0.95 && Math.min(...e) > MIN_EDGE
}

/** Sutherland–Hodgman: the part of convex `subject` inside convex `clipper`. */
function clip(subject: readonly Point[], clipper: readonly Point[]): Point[] {
  let out: Point[] = [...subject]
  const turn = Math.sign(area(clipper) === 0 ? 1 : signedArea(clipper))
  for (let i = 0; i < clipper.length && out.length > 0; i += 1) {
    const a = clipper[i] as Point
    const b = clipper[(i + 1) % clipper.length] as Point
    const input = out
    out = []
    const side = (p: Point) =>
      turn * ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]))
    for (let j = 0; j < input.length; j += 1) {
      const p = input[j] as Point
      const q = input[(j + 1) % input.length] as Point
      const sp = side(p)
      const sq = side(q)
      if (sp >= 0) out.push(p)
      if (sp >= 0 !== sq >= 0) {
        const t = sp / (sp - sq)
        out.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])])
      }
    }
  }
  return out
}

function signedArea(q: readonly Point[]): number {
  let sum = 0
  for (let i = 0; i < q.length; i += 1) {
    const [x0, y0] = q[i] as Point
    const [x1, y1] = q[(i + 1) % q.length] as Point
    sum += x0 * y1 - x1 * y0
  }
  return sum / 2
}

function overlap(a: Quad, b: Quad): number {
  const inside = clip(a, b)
  return inside.length > 2 ? area(inside) : 0
}

const centre = (q: Quad): Point => [
  q.reduce((sum, p) => sum + p[0], 0) / 4,
  q.reduce((sum, p) => sum + p[1], 0) / 4,
]

/**
 * The whole photo as one square tile, the photo centred in it: the tiles stop at the short side,
 * so a card filling more of the photo is only seen whole here.
 */
export function wholeTile(width: number, height: number): Tile {
  const side = Math.max(width, height)
  return { x: (width - side) / 2, y: (height - side) / 2, side }
}

/** Finds from `wholeTile`, kept only for cards too big for the tiles. */
export function placeWholeFinds(
  finds: readonly Box[],
  width: number,
  height: number,
  input: number,
): Box[] {
  const least = WHOLE_MIN * Math.min(width, height)
  return placeTileFinds(finds, wholeTile(width, height), input).filter(
    (box) => Math.max(...edges(box.quad)) >= least,
  )
}

function contains(q: Quad, p: Point): boolean {
  let sign = 0
  for (let i = 0; i < 4; i += 1) {
    const a = q[i] as Point
    const b = q[(i + 1) % 4] as Point
    const cross = Math.sign((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]))
    if (!cross) continue
    if (!sign) sign = cross
    else if (cross !== sign) return false
  }
  return true
}

/**
 * Maps one tile's finds (in the finder's `input` pixels) into photo pixels, dropping quads cut by
 * the tile edge or without a card's shape.
 */
export function placeTileFinds(finds: readonly Box[], tile: Tile, input: number): Box[] {
  const k = tile.side / input
  const margin = 3
  return finds.flatMap((find) => {
    if (
      find.quad.some(
        ([x, y]) => x < margin || y < margin || x > input - margin || y > input - margin,
      )
    )
      return []
    // Scaling and shifting keep the finder's corner order (already top left first).
    const quad = find.quad.map(([x, y]): Point => [
      tile.x + x * k,
      tile.y + y * k,
    ]) as unknown as Quad
    return plausible(quad) ? [{ score: find.score, quad }] : []
  })
}

/** One box per card, strongest find first, then in reading order: rows top to bottom, left to right. */
export function mergeDeckBoxes(boxes: readonly Box[]): Box[] {
  const sorted = [...boxes].sort((a, b) => b.score - a.score)
  let keep: Box[] = []
  for (const box of sorted) {
    const iouOk = keep.every((kept) => {
      const inter = overlap(box.quad, kept.quad)
      return inter / (area(box.quad) + area(kept.quad) - inter + 1e-6) < MERGE_IOU
    })
    if (iouOk) keep.push(box)
  }
  if (keep.length >= 5) {
    // A box mostly inside another (art inside a card, a card inside a bigger false box): keep the
    // surer one.
    const areas = keep.map((box) => area(box.quad))
    const drop = new Set<number>()
    for (let i = 0; i < keep.length; i += 1)
      for (let j = 0; j < keep.length; j += 1) {
        const ai = areas[i] ?? 0
        const aj = areas[j] ?? 0
        if (i === j || ai >= aj) continue
        if (overlap((keep[i] as Box).quad, (keep[j] as Box).quad) <= 0.5 * ai) continue
        drop.add((keep[i] as Box).score >= (keep[j] as Box).score ? j : i)
      }
    keep = keep.filter((_, i) => !drop.has(i))
    // A box straddling two neighbours is not a card.
    const sizes = keep.map((box) => area(box.quad))
    keep = keep.filter((box, i) => {
      const shares = keep
        .map((other, j) => (j === i ? 0 : overlap(box.quad, other.quad) / (sizes[i] ?? 1)))
        .filter((share) => share > 0.2)
      return !(shares.length >= 2 && shares.reduce((sum, share) => sum + share, 0) > 0.6)
    })
  }
  if (keep.length >= 5) {
    // Sizes are judged against the nearest sure finds, so printed marks and dice around a card
    // cannot make it look too big. Far smaller than them: not a card. Much bigger and holding two
    // other finds: two neighbours merged into one box. A big card on its own stays.
    const sizes = keep.map((box) => area(box.quad))
    const centres = keep.map((box) => centre(box.quad))
    const sure = keep.flatMap((box, j) => (box.score >= STRONG ? [j] : []))
    const reference = sure.length > 0 ? sure : keep.map((_, j) => j)
    keep = keep.filter((box, i) => {
      const [cx, cy] = centres[i] as Point
      const near = reference
        .filter((j) => j !== i)
        .map((j) => {
          const [x, y] = centres[j] as Point
          return [Math.hypot(x - cx, y - cy), j] as const
        })
        .sort((a, b) => a[0] - b[0])
        .slice(0, 6)
        .map(([, j]) => sizes[j] ?? 0)
        .sort((a, b) => a - b)
      const median = near[near.length >> 1] ?? 0
      const size = sizes[i] ?? 0
      if (size <= 0.35 * median) return false
      if (size < 2.2 * median) return true
      return centres.filter((c, j) => j !== i && contains(box.quad, c)).length < 2
    })
  }
  const heights = keep
    .map((box) => {
      const e = edges(box.quad)
      return ((e[1] ?? 0) + (e[3] ?? 0)) / 2
    })
    .sort((a, b) => a - b)
  const rowHeight = heights[heights.length >> 1] ?? 1
  const rows: { box: Box; c: Point }[][] = []
  for (const item of keep
    .map((box) => ({ box, c: centre(box.quad) }))
    .sort((a, b) => a.c[1] - b.c[1])) {
    const row = rows.at(-1)
    if (row && item.c[1] - (row[0]?.c[1] ?? 0) < 0.5 * rowHeight) row.push(item)
    else rows.push([item])
  }
  return rows.flatMap((row) => row.sort((a, b) => a.c[0] - b.c[0]).map((item) => item.box))
}

/** Whether a card in the photo lies on its side (wider than tall), so a 90° reading is worth trying. */
export function lyingSideways(quad: Quad): boolean {
  const e = edges(quad)
  return (e[0] ?? 0) + (e[2] ?? 0) > ((e[1] ?? 0) + (e[3] ?? 0)) * 0.92
}

/** The same box turned so that the photo's left edge becomes the card's top. */
export function turnedQuarter(quad: Quad): Quad {
  return [quad[3], quad[0], quad[1], quad[2]]
}
