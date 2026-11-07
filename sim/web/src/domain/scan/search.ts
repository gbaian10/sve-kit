import type { IndexDots } from "./dots"
import type { Scoring } from "./manifest"

/** One row of the index card list, in index row order. */
export interface ScanCard {
  /** Card number exactly as printed, e.g. `BP01-001` or `BP01-001EN`. */
  readonly no: string
  readonly cardId: string
  /** Integer image id (`images/<size>/<n>.webp`). */
  readonly imageId: number
  /** Face ordinal: 0 is the front; other faces' images are `<n>-f<face>.webp`. */
  readonly face: number
  /** Original card name. */
  readonly name: string
  /** Printings that share this art share the group. */
  readonly artGroup: string
  /** Class code, `neutral`, or "" for cards without a class. */
  readonly classCode: string
}

export interface ScanIndex {
  readonly dim: number
  readonly vectors: Int8Array
  readonly scale: Float32Array
  readonly hub: Float32Array
  readonly cards: readonly ScanCard[]
  readonly scoring: Scoring
  /** The rows in a SIMD kernel when the browser has one; otherwise rows are scored in JavaScript. */
  readonly dots?: IndexDots | null
}

export interface CardMatch {
  readonly card: ScanCard
  /** Similarity after the hub correction; ranks the candidates. */
  readonly score: number
  /** Plain cosine similarity; decides whether the best one is good enough. */
  readonly raw: number
}

function text(row: Record<string, unknown>, key: string, at: number): string {
  const value = row[key]
  if (typeof value !== "string") throw new Error(`cards[${String(at)}].${key} must be a string`)
  return value
}

/** Reads the index card list; it must have exactly one row per index row. */
export function parseScanCards(value: unknown, count: number): ScanCard[] {
  if (!Array.isArray(value) || value.length !== count)
    throw new Error(`card list must have ${String(count)} rows`)
  return value.map((item: unknown, at) => {
    if (typeof item !== "object" || item === null)
      throw new Error(`cards[${String(at)}] is not an object`)
    const row = item as Record<string, unknown>
    const imageId = row["n"]
    const face = row["f"]
    if (typeof imageId !== "number" || !Number.isInteger(imageId))
      throw new Error(`cards[${String(at)}].n must be an integer`)
    if (typeof face !== "number" || !Number.isInteger(face) || face < 0)
      throw new Error(`cards[${String(at)}].f must be a face ordinal`)
    return {
      no: text(row, "no", at),
      cardId: text(row, "c", at),
      imageId,
      face,
      name: text(row, "nm", at),
      artGroup: text(row, "a", at),
      classCode: text(row, "k", at),
    }
  })
}

/**
 * Which index rows to search: none picked means every card. Cards without a class (some tokens and
 * spells shared by all) stay in every selection.
 */
export function classMask(
  cards: readonly ScanCard[],
  classes: readonly string[],
): Uint8Array | null {
  if (classes.length === 0) return null
  const picked = new Set(classes)
  return Uint8Array.from(cards, (card) =>
    card.classCode === "" || picked.has(card.classCode) ? 1 : 0,
  )
}

const PLAIN_NO = /^[A-Za-z0-9]+-\d+(?:EN)?$/u

/**
 * Same-art printings look alike to the recogniser, so a match shows the plain-numbered one. The face
 * stays part of the key: a card's back is never shown as its front.
 */
function representatives(cards: readonly ScanCard[]): Map<string, ScanCard> {
  const map = new Map<string, ScanCard>()
  for (const card of cards) {
    const key = `${card.cardId}|${card.artGroup}|${String(card.face)}`
    const current = map.get(key)
    if (!current || (!PLAIN_NO.test(current.no) && PLAIN_NO.test(card.no))) map.set(key, card)
  }
  return map
}

const representativeCache = new WeakMap<readonly ScanCard[], Map<string, ScanCard>>()

/** Each row's card as a small integer, so ranking needs no string lookups per row. */
interface CardSlots {
  readonly ofRow: Int32Array
  readonly count: number
}

const slotCache = new WeakMap<readonly ScanCard[], CardSlots>()

function cardSlots(cards: readonly ScanCard[]): CardSlots {
  let slots = slotCache.get(cards)
  if (!slots) {
    const ids = new Map<string, number>()
    const ofRow = Int32Array.from(cards, (card) => {
      const known = ids.get(card.cardId)
      if (known !== undefined) return known
      ids.set(card.cardId, ids.size)
      return ids.size - 1
    })
    slots = { ofRow, count: ids.size }
    slotCache.set(cards, slots)
  }
  return slots
}

/**
 * The best `limit` different cards for one photo crop. Each view is an embedding of the same crop
 * (for example upright and turned 180°); a card scores by its best view.
 */
export function matchCards(
  index: ScanIndex,
  views: readonly Float32Array[],
  mask: Uint8Array | null,
  limit: number,
): CardMatch[] {
  if (limit <= 0) return []
  const { dim, vectors, scale, hub, cards } = index
  const hubWeight = index.scoring.hubWeight
  // Each card scores by its best printing row, so many printings of one card cannot crowd out the next card.
  const { ofRow, count } = cardSlots(cards)
  const bestScore = new Float64Array(count).fill(-Infinity)
  const bestRaw = new Float64Array(count)
  const bestRow = new Int32Array(count).fill(-1)
  const fast = views.length > 0 ? index.dots?.best(views) : undefined
  for (let at = 0; at < cards.length; at += 1) {
    if (mask && mask[at] === 0) continue
    let top = fast ? (fast[at] ?? -Infinity) : -Infinity
    if (!fast) {
      const base = at * dim
      for (const view of views) {
        let dot = 0
        for (let j = 0; j < dim; j += 1) dot += (vectors[base + j] ?? 0) * (view[j] ?? 0)
        if (dot > top) top = dot
      }
    }
    const raw = top * (scale[at] ?? 0)
    const score = raw - hubWeight * (hub[at] ?? 0)
    const slot = ofRow[at] ?? 0
    if (bestRow[slot] === -1 || score > (bestScore[slot] ?? -Infinity)) {
      bestScore[slot] = score
      bestRaw[slot] = raw
      bestRow[slot] = at
    }
  }
  // The best `limit` cards, highest first; ties keep the card met first.
  const top: number[] = []
  for (let slot = 0; slot < count; slot += 1) {
    if (bestRow[slot] === -1) continue
    const score = bestScore[slot] ?? -Infinity
    if (top.length === limit && score <= (bestScore[top[limit - 1] ?? 0] ?? -Infinity)) continue
    let k = top.length < limit ? top.length : limit - 1
    while (k > 0 && (bestScore[top[k - 1] ?? 0] ?? -Infinity) < score) {
      top[k] = top[k - 1] ?? 0
      k -= 1
    }
    top[k] = slot
  }
  let reps = representativeCache.get(cards)
  if (!reps) {
    reps = representatives(cards)
    representativeCache.set(cards, reps)
  }
  const representative = reps
  return top
    .map((slot) => ({
      at: bestRow[slot] ?? -1,
      score: bestScore[slot] ?? -Infinity,
      raw: bestRaw[slot] ?? 0,
    }))
    .flatMap((row) => {
      const card = cards[row.at]
      if (!card) return []
      const shown =
        representative.get(`${card.cardId}|${card.artGroup}|${String(card.face)}`) ?? card
      return [{ card: shown, score: row.score, raw: row.raw }]
    })
}

/** Whether the best match is good enough and far enough ahead of the next card. */
export function isConfident(matches: readonly CardMatch[], scoring: Scoring): boolean {
  const [best, next] = matches
  if (!best || best.raw < scoring.accept) return false
  return !next || best.score - next.score >= scoring.margin
}
