import type { Point, Quad } from "./tensor"

// The camera finds cards anew every frame. With two cards near the aiming frame the best one can
// change from frame to frame, and the same card's corners shift a little each time, so the drawn
// box jumps between cards and shakes. The follower keeps to the card it showed last and averages
// that card's corners over recent frames.

/** A card found this frame, in the same pixels as every other frame, and how good a pick it is. */
export interface QuadCandidate {
  readonly quad: Quad
  readonly key: number
}

/** A box overlapping the last one this much is the same card. */
const SAME_CARD = 0.5
/** Another card has to beat the followed one by this much to take over. */
const STAY = 0.25
/** Weight of this frame's corners against the average so far. */
const NEWEST = 0.5
/** Frames with no card at all before the followed one is dropped. */
const GRACE = 2

export class QuadFollower {
  private last: Quad | null = null
  private missed = 0

  /** The card to show this frame, or null when none has been seen for a while. */
  follow(candidates: readonly QuadCandidate[]): Quad | null {
    const last = this.last
    let best: { readonly quad: Quad; readonly key: number; readonly same: boolean } | null = null
    for (const candidate of candidates) {
      const same = last !== null && boxOverlap(candidate.quad, last) >= SAME_CARD
      const key = candidate.key + (same ? STAY : 0)
      if (!best || key > best.key) best = { quad: candidate.quad, key, same }
    }
    if (!best) {
      if (last && this.missed < GRACE) {
        this.missed += 1
        return last
      }
      this.reset()
      return null
    }
    this.missed = 0
    this.last = best.same && last ? blend(last, best.quad) : best.quad
    return this.last
  }

  reset(): void {
    this.last = null
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
 * Moves the last corners part way to the new ones. The finder lists corners clockwise but from any
 * corner, so the new corners are first turned to line up with the last ones.
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
