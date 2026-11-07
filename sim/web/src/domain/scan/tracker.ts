import type { CardMatch, ScanCard } from "./search"

// The live camera sees the same card many times a second with noise, glare and hand shake. The
// tracker smooths embeddings over a few frames, keeps a decaying score per card, and only lets a
// new card take the lead after it has stayed ahead for a while, so the answer does not flicker.

const SMOOTH_FRAMES = 6
/** Below this cosine to the running mean, the camera moved to something else: start over. */
const SMOOTH_RESET = 0.8
const HALF_LIFE_MS = 700
const SWITCH_MS = 500
/** A confident leader must hold this long before it shows as recognised. */
const CONFIRM_MS = 600
const SCENE_RESET = 0.8
const SCENE_FRAMES = 2
const KEEP_CARDS = 40
const RECENT_MS = 8000
const RECENT_SHOWN = 4

type TrackStatus = "ok" | "busy" | "unsure"

export interface TrackFrame {
  /** Leader first, then the next two cards. */
  readonly picks: readonly CardMatch[]
  readonly status: TrackStatus
  /** Cards seen in the last seconds that are not among the picks, best first. */
  readonly recent: readonly ScanCard[]
}

function dot(a: Float32Array, b: Float32Array, n: number): number {
  let sum = 0
  for (let j = 0; j < n; j += 1) sum += (a[j] ?? 0) * (b[j] ?? 0)
  return sum
}

export class ScanTracker {
  private readonly dim: number
  private smoothing: Float32Array[][] = []
  private scores = new Map<string, { card: ScanCard; sum: number }>()
  private weight = 0
  private lastTime = 0
  private scene: Float32Array | null = null
  private oddFrames = 0
  private leader: string | null = null
  private leaderSince = 0
  private challenger: string | null = null
  private challengerSince = 0
  private readonly recent = new Map<string, { card: ScanCard; last: number; best: number }>()

  constructor(dim: number) {
    this.dim = dim
  }

  /** Forget the current card, e.g. after the edition, classes or orientation change. */
  reset(): void {
    this.smoothing = []
    this.resetRanking()
  }

  /** Forget the recently seen list too. */
  clear(): void {
    this.reset()
    this.recent.clear()
  }

  /** Mean of the last frames' embeddings per view, unit length again; what the index is searched with. */
  smooth(views: readonly Float32Array[]): Float32Array[] {
    const first = views[0]
    if (first && this.smoothing.length > 0) {
      const mean = new Float32Array(this.dim)
      for (const frame of this.smoothing)
        for (let j = 0; j < this.dim; j += 1) mean[j] = (mean[j] ?? 0) + (frame[0]?.[j] ?? 0)
      if (dot(mean, first, this.dim) / Math.sqrt(dot(mean, mean, this.dim)) < SMOOTH_RESET)
        this.smoothing = []
    }
    this.smoothing.push(views.map((view) => Float32Array.from(view)))
    if (this.smoothing.length > SMOOTH_FRAMES) this.smoothing.shift()
    return views.map((_, at) => {
      const out = new Float32Array(this.dim)
      for (const frame of this.smoothing)
        for (let j = 0; j < this.dim; j += 1) out[j] = (out[j] ?? 0) + (frame[at]?.[j] ?? 0)
      const norm = Math.sqrt(dot(out, out, this.dim)) || 1
      for (let j = 0; j < this.dim; j += 1) out[j] = (out[j] ?? 0) / norm
      return out
    })
  }

  /**
   * Folds one frame in. `scene` is this frame's raw (unsmoothed) embedding, `matches` the index
   * matches of the smoothed one, `confident` the recogniser's own threshold check.
   */
  step(
    scene: Float32Array,
    matches: readonly CardMatch[],
    now: number,
    confident: (picks: readonly CardMatch[]) => boolean,
  ): TrackFrame {
    const { picks, held } = this.rank(scene, matches, now)
    const sure = picks.length > 0 && confident(picks)
    const status: TrackStatus = sure ? (held >= CONFIRM_MS ? "ok" : "busy") : "unsure"
    return { picks, status, recent: this.remember(matches.slice(0, 3), picks, now) }
  }

  private resetRanking(): void {
    this.scores = new Map()
    this.weight = 0
    this.lastTime = 0
    this.scene = null
    this.oddFrames = 0
    this.leader = null
    this.leaderSince = 0
    this.challenger = null
    this.challengerSince = 0
  }

  private rank(
    embedding: Float32Array,
    matches: readonly CardMatch[],
    now: number,
  ): { picks: CardMatch[]; held: number } {
    let odd = false
    if (this.scene) {
      odd =
        dot(this.scene, embedding, this.dim) / Math.sqrt(dot(this.scene, this.scene, this.dim)) <
        SCENE_RESET
      this.oddFrames = odd ? this.oddFrames + 1 : 0
      if (this.oddFrames >= SCENE_FRAMES) {
        this.resetRanking()
        odd = false
      }
    }
    if (!this.scene) this.scene = Float32Array.from(embedding.subarray(0, this.dim))
    else if (!odd)
      for (let j = 0; j < this.dim; j += 1)
        this.scene[j] = 0.7 * (this.scene[j] ?? 0) + 0.3 * (embedding[j] ?? 0)

    const decay = this.lastTime ? 0.5 ** ((now - this.lastTime) / HALF_LIFE_MS) : 0
    this.lastTime = now
    // A card missing from this frame counts as the frame's weakest match, not as zero.
    const floor = matches.at(-1)?.raw ?? 0
    const here = new Map(matches.map((match) => [match.card.cardId, match]))
    for (const [id, entry] of this.scores)
      entry.sum = entry.sum * decay + (here.get(id)?.raw ?? floor)
    for (const match of matches)
      if (!this.scores.has(match.card.cardId))
        this.scores.set(match.card.cardId, {
          card: match.card,
          sum: this.weight * decay * floor + match.raw,
        })
    this.weight = this.weight * decay + 1
    const ranked = [...this.scores.entries()]
      .map(([id, entry]) => ({ id, card: entry.card, score: entry.sum / this.weight }))
      .sort((a, b) => b.score - a.score)
    for (const dropped of ranked.slice(KEEP_CARDS)) this.scores.delete(dropped.id)
    const best = ranked[0]
    if (!best) return { picks: [], held: 0 }

    if (!this.leader || !this.scores.has(this.leader)) {
      this.leader = best.id
      this.leaderSince = now
      this.challenger = null
    } else if (best.id !== this.leader) {
      if (this.challenger !== best.id) {
        this.challenger = best.id
        this.challengerSince = now
      } else if (now - this.challengerSince >= SWITCH_MS) {
        this.leader = best.id
        this.leaderSince = now
        this.challenger = null
      }
    } else this.challenger = null
    const lead = ranked.find((entry) => entry.id === this.leader) ?? best
    const picks = [lead, ...ranked.filter((entry) => entry !== lead)]
      .slice(0, 3)
      .map((entry) => ({ card: entry.card, score: entry.score, raw: entry.score }))
    return { picks, held: now - this.leaderSince }
  }

  private remember(
    seen: readonly CardMatch[],
    picks: readonly CardMatch[],
    now: number,
  ): ScanCard[] {
    for (const match of seen) {
      const entry = this.recent.get(match.card.cardId)
      if (entry) {
        entry.last = now
        entry.best = Math.max(entry.best, match.raw)
        entry.card = match.card
      } else this.recent.set(match.card.cardId, { card: match.card, last: now, best: match.raw })
    }
    for (const [id, entry] of this.recent) if (now - entry.last > RECENT_MS) this.recent.delete(id)
    const live = new Set(picks.map((pick) => pick.card.cardId))
    return [...this.recent.entries()]
      .filter(([id]) => !live.has(id))
      .sort((a, b) => b[1].best - a[1].best)
      .slice(0, RECENT_SHOWN)
      .map(([, entry]) => entry.card)
  }
}
