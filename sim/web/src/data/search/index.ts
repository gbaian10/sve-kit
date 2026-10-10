import { cardNoKey, cardNoLookupKey } from "../../domain/cardNo"
import { normalizeText } from "../../domain/normalize"
import { NEUTRAL_CLASS, type QueryState } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import { type CardSummary, catalogCards } from "../catalog"
import { type JsonObject, stringValue } from "../format-v3/json"
import type { BootstrapColumns } from "./bootstrap"
import { Columns, StringPool } from "./columns"

export interface SearchQuery {
  readonly state: QueryState
  readonly edition: Region
  readonly offset?: number
  readonly limit?: number
}
export interface SearchPage {
  readonly total: number
  readonly offset: number
  readonly items: readonly {
    readonly key: string
    readonly printingId: string
    readonly summary: CardSummary | undefined
  }[]
}
type UnsupportedFacet = "types" | "rarities" | "altArtOnly" | "mechanics" | "unit" | "sort"
export interface QueryFailure {
  readonly kind: "unsupported-query" | "edition-not-ready" | "query-failed" | "load-unavailable"
  readonly message: string
  readonly facets?: readonly UnsupportedFacet[]
}
export class SearchQueryError extends Error {
  readonly failure: QueryFailure

  constructor(failure: QueryFailure) {
    super(failure.message)
    this.failure = failure
  }
}

const CARD_COLUMNS = ["id", "class", "default", "printingStart", "nameStart", "aliasStart"] as const
const PRINTING_COLUMNS = [
  "id",
  "lookup",
  "flat",
  "set",
  "number",
  "owner",
  "card",
  "summary",
] as const
const uintColumns = <T extends string>(names: readonly T[]) =>
  Object.fromEntries(names.map((name) => [name, [] as number[]])) as Record<T, number[]>
const seal = <T extends string>(columns: Record<T, number[]>): Record<T, Uint32Array> =>
  Object.fromEntries(
    Object.entries(columns).map(([key, values]) => [key, Uint32Array.from(values as number[])]),
  ) as Record<T, Uint32Array>

/** Matching reads scalar columns; summaries are decoded only for the requested page. */
export class SearchIndex {
  private readonly pool = new StringPool()
  private readonly summaries = new Columns(this.pool)
  private readonly cards: Record<(typeof CARD_COLUMNS)[number], Uint32Array>
  private readonly printings: Record<(typeof PRINTING_COLUMNS)[number], Uint32Array>
  private readonly names: Uint32Array
  private readonly aliases: Uint32Array
  private readonly costs: Float64Array
  private readonly sets: ReadonlySet<string>
  private readonly edition: Region

  constructor(bootstrap: BootstrapColumns, edition: Region) {
    this.edition = edition
    const index = bootstrap.index()
    this.sets = new Set(index.families.map((family) => stringValue(family["code"])))
    const familyCodes = new Map(
      index.families.flatMap(
        (row) =>
          [
            [stringValue(row["id"]), stringValue(row["code"])],
            [stringValue(row["public_code"]), stringValue(row["code"])],
          ] as [string, string][],
      ),
    )
    const cards = uintColumns(CARD_COLUMNS)
    const printings = uintColumns(PRINTING_COLUMNS)
    const names: number[] = []
    const aliases: number[] = []
    const costs: number[] = []
    let batch: CardSummary[] = []
    let summaryCount = 0
    const intern = (value: string | null | undefined) => this.pool.intern(value ?? "")
    intern("")
    for (const { entry, summaries } of catalogCards(
      index,
      bootstrap.cards(),
      bootstrap.rows("search_alias"),
      this.sets,
      edition,
    )) {
      const card = cards.id.length
      cards.id.push(intern(entry.cardId))
      cards.class.push(intern(entry.classCode ?? NEUTRAL_CLASS))
      cards.printingStart.push(printings.id.length)
      cards.nameStart.push(names.length)
      cards.aliasStart.push(aliases.length)
      names.push(...entry.names.map((name) => intern(name.normalized)))
      aliases.push(...entry.aliases.map((name) => intern(name.normalized)))
      let preferred = printings.id.length
      for (const printing of entry.printings) {
        if (printing.id === entry.defaultPrinting[edition]) preferred = printings.id.length
        const summary = summaries.find((value) => value.printingId === printing.id)
        printings.id.push(intern(printing.id))
        printings.lookup.push(intern(printing.lookupKey))
        printings.flat.push(intern(printing.flat))
        printings.set.push(intern(printing.key?.set))
        printings.number.push(intern(printing.key?.number))
        const owner = index.printing(printing.id)?.["home_set_id"]
        printings.owner.push(intern(typeof owner === "string" ? familyCodes.get(owner) : undefined))
        printings.card.push(card)
        printings.summary.push(summary ? ++summaryCount : 0)
        costs.push(summary?.cost ?? Number.NaN)
        if (summary) batch.push(summary)
        if (batch.length >= 256) {
          this.summaries.append(batch as unknown as JsonObject[])
          batch = []
        }
      }
      cards.default.push(preferred)
    }
    this.summaries.append(batch as unknown as JsonObject[])
    cards.printingStart.push(printings.id.length)
    cards.nameStart.push(names.length)
    cards.aliasStart.push(aliases.length)
    this.cards = seal(cards)
    this.printings = seal(printings)
    this.names = Uint32Array.from(names)
    this.aliases = Uint32Array.from(aliases)
    this.costs = Float64Array.from(costs)
    this.pool.seal()
  }

  search(query: SearchQuery): SearchPage {
    if (query.edition !== this.edition)
      throw new SearchQueryError({ kind: "edition-not-ready", message: "edition is not ready" })
    const { state } = query
    const facets: UnsupportedFacet[] = []
    if (state.types.length) facets.push("types")
    if (state.rarities.length) facets.push("rarities")
    if (state.altArtOnly) facets.push("altArtOnly")
    if (Object.keys(state.mechanics).length) facets.push("mechanics")
    if (state.unit !== "card") facets.push("unit")
    if (state.sort !== "no") facets.push("sort")
    if (facets.length)
      throw new SearchQueryError({
        kind: "unsupported-query",
        message: "requested search facet is not ready",
        facets,
      })
    const flat = normalizeText(state.text)
    const lookup = cardNoLookupKey(state.text, this.sets)
    const typed = cardNoKey(state.text, this.sets)
    const digits = /^\d+$/u.test(flat) ? String(Number(flat)) : null
    const buckets: number[][] = [[], [], [], []]
    const allowed = (printing: number) =>
      !state.sets.length || state.sets.includes(this.pool.get(this.printings.owner[printing] ?? 0))
    for (let card = 0; card < this.cards.id.length; card += 1) {
      if (
        state.classes.length &&
        !state.classes.includes(this.pool.get(this.cards.class[card] ?? 0))
      )
        continue
      const start = this.cards.printingStart[card] ?? 0
      const end = this.cards.printingStart[card + 1] ?? start
      let representative = this.cards.default[card] ?? start
      if (!allowed(representative)) {
        representative = -1
        for (let p = start; p < end; p += 1)
          if (allowed(p)) {
            representative = p
            break
          }
      }
      if (representative < 0) continue
      let printing = representative
      let rank = 0
      if (state.text !== "") {
        if (!flat) continue
        printing = -1
        if (lookup !== null)
          for (let p = start; p < end; p += 1)
            if (allowed(p) && this.pool.get(this.printings.lookup[p] ?? 0) === lookup) {
              printing = p
              break
            }
        if (printing < 0 && digits !== null)
          for (let p = start; p < end; p += 1)
            if (allowed(p) && this.pool.get(this.printings.number[p] ?? 0) === digits) {
              printing = p
              break
            }
        if (printing < 0) {
          rank = 1
          for (let p = start; p < end; p += 1) {
            if (!allowed(p)) continue
            const number = this.pool.get(this.printings.number[p] ?? 0)
            if (
              this.pool.get(this.printings.flat[p] ?? 0).startsWith(flat) ||
              (number !== "" &&
                typed !== null &&
                typed.suffix === "" &&
                this.pool.get(this.printings.set[p] ?? 0) === typed.set &&
                number.startsWith(typed.number)) ||
              (number !== "" && digits !== null && number.startsWith(digits))
            ) {
              printing = p
              break
            }
          }
        }
        if (printing < 0) {
          const name = this.textRank(this.names, this.cards.nameStart, card, flat)
          const alias = this.textRank(this.aliases, this.cards.aliasStart, card, flat)
          rank = Math.min(name, alias)
          if (rank > 3) continue
          printing = representative
        }
      }
      const cost = this.costs[printing] ?? Number.NaN
      const upper = state.cost.max !== undefined && state.cost.max < 7 ? state.cost.max : undefined
      if (
        (state.cost.min !== undefined || upper !== undefined) &&
        (Number.isNaN(cost) ||
          (state.cost.min !== undefined && cost < state.cost.min) ||
          (upper !== undefined && cost > upper))
      )
        continue
      buckets[rank]?.push(printing)
    }
    const matches = buckets.flat()
    const offset = Math.max(0, Math.trunc(query.offset ?? 0))
    const limit = Math.max(1, Math.min(100, Math.trunc(query.limit ?? 24)))
    return {
      total: matches.length,
      offset,
      items: matches.slice(offset, offset + limit).map((printing) => {
        const card = this.printings.card[printing] ?? 0
        const summary = this.printings.summary[printing] ?? 0
        return {
          key: this.pool.get(this.cards.id[card] ?? 0),
          printingId: this.pool.get(this.printings.id[printing] ?? 0),
          summary: summary
            ? (this.summaries.row(summary - 1) as unknown as CardSummary)
            : undefined,
        }
      }),
    }
  }

  private textRank(values: Uint32Array, offsets: Uint32Array, card: number, flat: string): number {
    let rank = 4
    for (let n = offsets[card] ?? 0; n < (offsets[card + 1] ?? 0); n += 1) {
      const text = this.pool.get(values[n] ?? 0)
      if (text !== "" && text.startsWith(flat)) return 2
      if (text !== "" && text.includes(flat)) rank = 3
    }
    return rank
  }

  allocation(): {
    readonly arrayBuffers: number
    readonly stringBytes: number
    readonly cards: number
  } {
    return {
      arrayBuffers:
        this.summaries.bytes() +
        this.pool.bufferBytes() +
        [
          ...Object.values(this.cards),
          ...Object.values(this.printings),
          this.names,
          this.aliases,
          this.costs,
        ].reduce((total, value) => total + value.byteLength, 0),
      stringBytes: this.pool.bytes(),
      cards: this.cards.id.length,
    }
  }
}
