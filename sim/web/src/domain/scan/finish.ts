import type { ScanCard } from "./search"

// The recogniser cannot tell same-art printings apart; this tells SL from plain by frame hue against
// each printing's official image. Premium (P) images equal the plain ones, so those stay plain.

/** Frame regions as fractions of the card (x0, y0, x1, y1): cost and gem corners, then the name bar. */
const REGIONS: readonly (readonly [number, number, number, number])[] = [
  [0.02, 0.82, 0.16, 0.93],
  [0.84, 0.82, 0.98, 0.93],
  [0.2, 0.82, 0.35, 0.88],
  [0.35, 0.82, 0.5, 0.88],
  [0.5, 0.82, 0.65, 0.88],
  [0.65, 0.82, 0.8, 0.88],
]
/** Degrees: a region counts only where two printings' hues differ more than this. */
const FINISH_SPLIT = 25
/** Degrees: another printing replaces the plain one only when it matches this much better. */
const FINISH_MARGIN = 10
/** Weight of the newest frame in the running score of the live camera. */
const FINISH_EMA = 0.3

/** The (a, b) chroma of each frame region, in CIE Lab. */
export type Finish = readonly (readonly [number, number])[]

function linear(v: number): number {
  const c = Math.min(255, v) / 255
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
}

function labF(t: number): number {
  return t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116
}

/**
 * The frame colours of a card image, RGBA `width` × `height`. `turned` reads the card upside
 * down (its frame is then at the top of the pixels).
 */
export function finishOf(
  rgba: Uint8ClampedArray,
  width: number,
  height: number,
  turned = false,
): Finish {
  const n = width * height
  let r = 0
  let g = 0
  let b = 0
  for (let i = 0; i < n; i += 1) {
    r += rgba[4 * i] ?? 0
    g += rgba[4 * i + 1] ?? 0
    b += rgba[4 * i + 2] ?? 0
  }
  // Gray world: scale each channel so the card's average is neutral.
  const all = (r + g + b) / 3
  const gain = [all / (r + n), all / (g + n), all / (b + n)] as const
  return REGIONS.map(([rx0, ry0, rx1, ry1]) => {
    const [x0, y0, x1, y1] = turned ? [1 - rx1, 1 - ry1, 1 - rx0, 1 - ry0] : [rx0, ry0, rx1, ry1]
    let a = 0
    let bb = 0
    let k = 0
    for (let y = Math.floor(y0 * height); y < Math.floor(y1 * height); y += 1)
      for (let x = Math.floor(x0 * width); x < Math.floor(x1 * width); x += 1) {
        const i = 4 * (y * width + x)
        const lr = linear((rgba[i] ?? 0) * gain[0])
        const lg = linear((rgba[i + 1] ?? 0) * gain[1])
        const lb = linear((rgba[i + 2] ?? 0) * gain[2])
        const fx = labF((0.4124 * lr + 0.3576 * lg + 0.1805 * lb) / 0.9505)
        const fy = labF(0.2126 * lr + 0.7152 * lg + 0.0722 * lb)
        const fz = labF((0.0193 * lr + 0.1192 * lg + 0.9505 * lb) / 1.089)
        a += 500 * (fx - fy)
        bb += 200 * (fy - fz)
        k += 1
      }
    return [a / Math.max(k, 1), bb / Math.max(k, 1)] as const
  })
}

/** Angle between two chroma vectors in degrees: the hue difference. */
function hueGap(p: readonly [number, number], q: readonly [number, number]): number {
  const cos = (p[0] * q[0] + p[1] * q[1]) / Math.max(Math.hypot(...p) * Math.hypot(...q), 1e-6)
  return (Math.acos(Math.max(-1, Math.min(1, cos))) * 180) / Math.PI
}

/** Printings of the same card, art and face: the ones the recogniser cannot tell apart. */
export function finishKey(card: ScanCard): string {
  return `${card.cardId}|${card.artGroup}|${String(card.face)}`
}

/** Groups with more than one printing, by `finishKey`. */
export function finishGroups(cards: readonly ScanCard[]): Map<string, ScanCard[]> {
  const groups = new Map<string, ScanCard[]>()
  for (const card of cards) {
    const key = finishKey(card)
    const group = groups.get(key)
    if (group) group.push(card)
    else groups.set(key, [card])
  }
  for (const [key, group] of groups) if (group.length < 2) groups.delete(key)
  return groups
}

/**
 * How far the photographed card is from each printing (mean hue gap over the regions where the
 * printings differ; the better of upright and turned). Null when all printings share one frame.
 */
export function finishScores(
  references: readonly Finish[],
  photo: readonly [Finish, Finish],
): number[] | null {
  const first = references[0]
  if (!first) return null
  const used = REGIONS.map((_, r) =>
    references.some((ref) => {
      const p = ref[r]
      const q = first[r]
      return p !== undefined && q !== undefined && hueGap(p, q) > FINISH_SPLIT
    }),
  )
  if (!used.some(Boolean)) return null
  return references.map((ref) => {
    const gap = (finish: Finish) => {
      let sum = 0
      let count = 0
      used.forEach((on, r) => {
        const p = finish[r]
        const q = ref[r]
        if (on && p !== undefined && q !== undefined) {
          sum += hueGap(p, q)
          count += 1
        }
      })
      return sum / Math.max(count, 1)
    }
    return Math.min(gap(photo[0]), gap(photo[1]))
  })
}

/**
 * The printing to show: the best-matching one when it beats `plain` by `FINISH_MARGIN`,
 * otherwise `plain`. `scores` follow `group`.
 */
export function pickFinish(
  group: readonly ScanCard[],
  scores: readonly number[],
  plain: ScanCard,
): ScanCard {
  const plainAt = group.indexOf(plain)
  const plainScore = scores[plainAt]
  if (plainAt < 0 || plainScore === undefined) return plain
  let best = plainAt
  scores.forEach((score, at) => {
    if (score < (scores[best] ?? Infinity)) best = at
  })
  const bestScore = scores[best]
  return bestScore !== undefined && plainScore - bestScore >= FINISH_MARGIN
    ? (group[best] ?? plain)
    : plain
}

/** The live camera's running scores: one frame of glare does not flip the printing. */
export class FinishSmoother {
  private key = ""
  private scores: number[] = []

  /** Starts over when the leading card changes, also on frames that cannot be scored. */
  follow(key: string): void {
    if (key === this.key) return
    this.key = key
    this.scores = []
  }

  /** Folds this frame's scores in; a different group starts over. */
  update(key: string, scores: readonly number[]): number[] {
    this.follow(key)
    if (scores.length !== this.scores.length) this.scores = [...scores]
    else
      this.scores = this.scores.map(
        (old, at) => (1 - FINISH_EMA) * old + FINISH_EMA * (scores[at] ?? old),
      )
    return this.scores
  }

  reset(): void {
    this.key = ""
    this.scores = []
  }
}
