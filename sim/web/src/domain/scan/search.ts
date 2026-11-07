import type { Scoring } from "./manifest"

/** One row of the index card list, in index row order. */
export interface ScanCard {
  /** Card number exactly as printed, e.g. `BP01-001` or `BP01-001EN`. */
  readonly no: string
  readonly cardId: string
  /** Integer image id (`images/<size>/<n>.webp`). */
  readonly imageId: number
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
    if (typeof imageId !== "number" || !Number.isInteger(imageId))
      throw new Error(`cards[${String(at)}].n must be an integer`)
    return {
      no: text(row, "no", at),
      cardId: text(row, "c", at),
      imageId,
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

/** Same-art printings look alike to the recogniser, so a match shows the plain-numbered one. */
function representatives(cards: readonly ScanCard[]): Map<string, ScanCard> {
  const map = new Map<string, ScanCard>()
  for (const card of cards) {
    const key = `${card.cardId}|${card.artGroup}`
    const current = map.get(key)
    if (!current || (!PLAIN_NO.test(current.no) && PLAIN_NO.test(card.no))) map.set(key, card)
  }
  return map
}

const representativeCache = new WeakMap<readonly ScanCard[], Map<string, ScanCard>>()

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
  const { dim, vectors, scale, hub, cards } = index
  const hubWeight = index.scoring.hubWeight
  // Same-card printings crowd the top rows, so keep more rows than distinct cards wanted.
  const keep = limit * 4
  const rows: { at: number; score: number; raw: number }[] = []
  for (let at = 0; at < cards.length; at += 1) {
    if (mask && mask[at] === 0) continue
    const base = at * dim
    let best = -Infinity
    for (const view of views) {
      let dot = 0
      for (let j = 0; j < dim; j += 1) dot += (vectors[base + j] ?? 0) * (view[j] ?? 0)
      if (dot > best) best = dot
    }
    const raw = best * (scale[at] ?? 0)
    const score = raw - hubWeight * (hub[at] ?? 0)
    if (rows.length === keep && score <= (rows[keep - 1]?.score ?? -Infinity)) continue
    let k = rows.length
    while (k > 0 && (rows[k - 1]?.score ?? Infinity) < score) k -= 1
    rows.splice(k, 0, { at, score, raw })
    if (rows.length > keep) rows.pop()
  }
  let reps = representativeCache.get(cards)
  if (!reps) {
    reps = representatives(cards)
    representativeCache.set(cards, reps)
  }
  const seen = new Set<string>()
  const out: CardMatch[] = []
  for (const row of rows) {
    const card = cards[row.at]
    if (!card || seen.has(card.cardId)) continue
    seen.add(card.cardId)
    out.push({
      card: reps.get(`${card.cardId}|${card.artGroup}`) ?? card,
      score: row.score,
      raw: row.raw,
    })
    if (out.length === limit) break
  }
  return out
}

/** Whether the best match is good enough and far enough ahead of the next card. */
export function isConfident(matches: readonly CardMatch[], scoring: Scoring): boolean {
  const [best, next] = matches
  if (!best || best.raw < scoring.accept) return false
  return !next || best.score - next.score >= scoring.margin
}
