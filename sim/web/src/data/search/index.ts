import { NEUTRAL_CLASS, type QueryState } from "../../domain/query/model"
import { matchEntry, type Region, type SearchEntry } from "../../domain/search"
import type { CardSummary, Catalog } from "../catalog"
import { type JsonObject, stringValue } from "../format-v3/json"
import { Columns, StringPool } from "./columns"

interface IndexedEntry extends SearchEntry {
  readonly printingSets: Readonly<Record<string, string | null>>
}

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

/** The active generation contains no bootstrap, decoded rows, or Catalog closures. */
export class SearchIndex {
  private readonly pool = new StringPool()
  private readonly entries = new Columns(this.pool)
  private readonly summaries = new Columns(this.pool)
  private readonly printingOrdinals = new Map<string, number>()
  private readonly sets: ReadonlySet<string>
  private readonly edition: Region

  constructor(catalog: Catalog, edition: Region) {
    this.edition = edition
    this.sets = catalog.sets
    const familyCodes = new Map(
      catalog.index.families.flatMap(
        (row) =>
          [
            [stringValue(row["id"]), stringValue(row["code"])],
            [stringValue(row["public_code"]), stringValue(row["code"])],
          ] as [string, string][],
      ),
    )
    const entries = catalog.entries.flatMap((entry) => {
      const printings = entry.printings.filter((printing) => printing.region === edition)
      if (printings.length === 0) return []
      const representative = catalog.summary(
        entry.defaultPrinting[edition] ?? printings[0]?.id ?? "",
      )
      return [
        {
          ...entry,
          classCode: representative ? representative.classCode : entry.classCode,
          printingSets: Object.fromEntries(
            printings.map((printing) => {
              const owner = catalog.index.printing(printing.id)?.["home_set_id"]
              return [
                printing.id,
                typeof owner === "string" ? (familyCodes.get(owner) ?? null) : null,
              ]
            }),
          ),
          printings,
          defaultPrinting: { [edition]: entry.defaultPrinting[edition] ?? printings[0]?.id },
        },
      ]
    })
    this.entries.append(entries as unknown as JsonObject[])
    const summaries: CardSummary[] = []
    for (const entry of entries)
      for (const printing of entry.printings) {
        const summary = catalog.summary(printing.id)
        if (summary) {
          this.printingOrdinals.set(printing.id, summaries.length)
          summaries.push(summary)
        }
      }
    this.summaries.append(summaries as unknown as JsonObject[])
    this.pool.seal()
  }

  search(query: SearchQuery): SearchPage {
    if (query.edition !== this.edition) throw new Error("edition is not ready")
    const { state } = query
    if (
      state.types.length ||
      state.rarities.length ||
      state.altArtOnly ||
      Object.keys(state.mechanics).length ||
      state.unit !== "card" ||
      state.sort !== "no"
    )
      throw new Error("requested search facet is not ready")
    const matches: { ordinal: number; printingId: string; rank: number; order: number }[] = []
    for (let ordinal = 0; ordinal < this.entries.length; ordinal += 1) {
      let entry = this.entries.row(ordinal) as unknown as IndexedEntry
      if (state.classes.length && !state.classes.includes(entry.classCode ?? NEUTRAL_CLASS))
        continue
      if (state.sets.length) {
        const printings = entry.printings.filter((printing) =>
          state.sets.includes(entry.printingSets[printing.id] ?? ""),
        )
        if (!printings.length) continue
        const preferred = entry.defaultPrinting[query.edition]
        entry = {
          ...entry,
          printings,
          defaultPrinting: {
            [query.edition]:
              printings.find((printing) => printing.id === preferred)?.id ?? printings[0]?.id,
          },
        }
      }
      const match =
        state.text === ""
          ? null
          : matchEntry(state.text, entry, { edition: query.edition, sets: this.sets })
      if (state.text !== "" && !match) continue
      const printingId = match?.printingId ?? entry.defaultPrinting[query.edition]
      if (!printingId) continue
      const { min, max } = state.cost
      // A max of 7 means 7 or more; summaries are decoded only when a bound applies.
      const upper = max !== undefined && max < 7 ? max : undefined
      if (min !== undefined || upper !== undefined) {
        const cost = this.summary(printingId)?.cost
        if (cost === null || cost === undefined) continue
        if ((min !== undefined && cost < min) || (upper !== undefined && cost > upper)) continue
      }
      matches.push({ ordinal, printingId, rank: match?.rank ?? 0, order: entry.order })
    }
    matches.sort((a, b) => a.rank - b.rank || a.order - b.order)
    const offset = Math.max(0, Math.trunc(query.offset ?? 0))
    const limit = Math.max(1, Math.min(100, Math.trunc(query.limit ?? 24)))
    return {
      total: matches.length,
      offset,
      items: matches.slice(offset, offset + limit).map((match) => ({
        key: stringValue(this.entries.value(match.ordinal, "cardId")),
        printingId: match.printingId,
        summary: this.summary(match.printingId),
      })),
    }
  }

  private summary(printingId: string): CardSummary | undefined {
    const ordinal = this.printingOrdinals.get(printingId)
    return ordinal === undefined
      ? undefined
      : (this.summaries.row(ordinal) as unknown as CardSummary)
  }

  allocation(): {
    readonly arrayBuffers: number
    readonly stringBytes: number
    readonly cards: number
  } {
    return {
      arrayBuffers: this.entries.bytes() + this.summaries.bytes() + this.pool.bufferBytes(),
      stringBytes: this.pool.bytes(),
      cards: this.entries.length,
    }
  }
}
