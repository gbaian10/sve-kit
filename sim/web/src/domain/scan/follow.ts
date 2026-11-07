import type { Point, Quad } from "./tensor"

// Each frame finds cards anew; without following one, a box near two cards jumps between them.

/** A card found this frame, in the same pixels as every other frame, and how good a pick it is. */
export interface QuadCandidate {
  readonly quad: Quad
  readonly key: number
}

export interface Followed {
  /** Where to draw the box: the followed card's corners averaged over recent frames. */
  readonly shown: Quad | null
  /** This frame's own corners of the followed card, for reading it; null when none was found. */
  readonly found: Quad | null
}

/** A box overlapping the last one this much can be the same card. */
const SAME_CARD = 0.5
/** Another card has to beat the followed one by this much to take over. */
const STAY = 0.25
/** Weight of this frame's corners against the average so far. */
const NEWEST = 0.5
/** Frames with no card at all while the last box stays drawn. */
const GRACE = 2

export class QuadFollower {
  /** The followed card as last found, and as last drawn. */
  private found: Quad | null = null
  private shown: Quad | null = null
  private missed = 0

  follow(candidates: readonly QuadCandidate[]): Followed {
    const last = this.found
    if (candidates.length === 0) {
      this.missed += 1
      if (this.missed > GRACE) this.reset()
      return { shown: this.shown, found: null }
    }
    // Only the one candidate that overlaps the followed card most is that card; stacked cards
    // that also overlap it are others.
    let same = -1
    let most = SAME_CARD
    if (last)
      candidates.forEach((candidate, i) => {
        const overlap = boxOverlap(candidate.quad, last)
        if (overlap >= most) [same, most] = [i, overlap]
      })
    let pick = 0
    let best = -Infinity
    candidates.forEach((candidate, i) => {
      const key = candidate.key + (i === same ? STAY : 0)
      if (key > best) [pick, best] = [i, key]
    })
    const quad = (candidates[pick] as QuadCandidate).quad
    this.shown = pick === same && this.shown ? blend(this.shown, quad) : quad
    this.found = quad
    this.missed = 0
    return { shown: this.shown, found: quad }
  }

  reset(): void {
    this.found = null
    this.shown = null
    this.missed = 0
  }
}

/** Intersection over union of the two quads' bounding boxes. */
export function boxOverlap(a: Quad, b: Quad): number {
  const [ax0, ay0, ax1, ay1] = bounds(a)
  const [bx0, by0, bx1, by1] = bounds(b)
  const w = Math.min(ax1, bx1) - Math.max(ax0, bx0)
  const h = Math.min(ay1, by1) - Math.max(ay0, by0)
  if (w <= 0 || h <= 0) return 0
  const both = w * h
  return both / ((ax1 - ax0) * (ay1 - ay0) + (bx1 - bx0) * (by1 - by0) - both)
}

function bounds(quad: Quad): readonly [number, number, number, number] {
  const xs = quad.map((p) => p[0])
  const ys = quad.map((p) => p[1])
  return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)]
}

/**
 * Moves the drawn corners part way to the new ones. The finder lists corners clockwise but from any
 * corner, so the new corners are first turned to line up with the drawn ones.
 */
function blend(last: Quad, next: Quad): Quad {
  let shift = 0
  let nearest = Infinity
  for (let r = 0; r < 4; r += 1) {
    let distance = 0
    for (let i = 0; i < 4; i += 1) {
      const [x, y] = last[i] as Point
      const [nx, ny] = next[(i + r) % 4] as Point
      distance += Math.hypot(nx - x, ny - y)
    }
    if (distance < nearest) [nearest, shift] = [distance, r]
  }
  return last.map(([x, y], i): Point => {
    const [nx, ny] = next[(i + shift) % 4] as Point
    return [x + (nx - x) * NEWEST, y + (ny - y) * NEWEST]
  }) as unknown as Quad
}
