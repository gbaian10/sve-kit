import { cardNoKey, cardNoLookupKey } from "../../domain/cardNo"
import { normalizeText } from "../../domain/normalize"
import { DEFAULT_QUERY, NEUTRAL_CLASS, type QueryState } from "../../domain/query/model"
import type { MatchedField, Region, TextLang } from "../../domain/search"
import { datedSets, type PackCounts, packCounts } from "../../domain/sets"
import { type CardSummary, catalogCards } from "../catalog"
import {
  arrayValue,
  integerValue,
  type JsonObject,
  objectValue,
  stringValue,
} from "../format-v3/json"
import type { BootstrapColumns } from "./bootstrap"
import { cardNumberOrder } from "./card-number-order"
import { Columns, StringPool } from "./columns"
import { pack, unpack } from "./image"

export interface SearchQuery {
  readonly state: QueryState
  readonly edition: Region
  readonly offset?: number
  readonly limit?: number
}
interface SearchOption {
  readonly id: string
  readonly code: string
  readonly kind?: string
  readonly publicCode?: string
  readonly name: CardSummary["name"]
}
export interface SearchOptions {
  readonly classes: readonly SearchOption[]
  readonly sets: readonly SearchOption[]
}
/** A set with its first day-precision release in the index's edition. */
export interface SetRelease extends SearchOption {
  readonly publicCode: string
  readonly kind: string
  readonly releasedOn: string
}
export interface ImageTarget {
  readonly printingId: string
  readonly faceId: string
  readonly intId: number
  readonly ordinal: number
  readonly owner: string
  readonly rarityCode?: string
}
export interface SuggestedSummary {
  readonly summary: CardSummary
  readonly field: MatchedField
  readonly rank: number
}
export interface RecentSummary {
  readonly printingId: string
  readonly state: "available" | "not-in-edition"
  readonly summary?: CardSummary
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
  "type",
  "rarity",
  "art",
  "alt",
] as const
const uintColumns = <T extends string>(names: readonly T[]) =>
  Object.fromEntries(names.map((name) => [name, [] as number[]])) as Record<T, number[]>
const seal = <T extends string>(columns: Record<T, number[]>): Record<T, Uint32Array> =>
  Object.fromEntries(
    Object.entries(columns).map(([key, values]) => [key, Uint32Array.from(values as number[])]),
  ) as Record<T, Uint32Array>

/** Bump when the stored layout or derived data changes, so older images are rebuilt. */
export const INDEX_IMAGE_FORMAT = 8

interface Parts {
  readonly pool: StringPool
  readonly summaries: Columns
  readonly cards: Record<(typeof CARD_COLUMNS)[number], Uint32Array>
  readonly printings: Record<(typeof PRINTING_COLUMNS)[number], Uint32Array>
  readonly names: Uint32Array
  readonly aliases: Uint32Array
  readonly order: Uint32Array
  readonly costs: Float64Array
  readonly sets: ReadonlySet<string>
  readonly options: SearchOptions
  readonly releases: readonly SetRelease[]
  readonly targets: Columns
}

function build(bootstrap: BootstrapColumns, edition: Region): Parts {
  const pool = new StringPool()
  const summaryColumns = new Columns(pool)
  const index = bootstrap.index()
  const sets = new Set(index.families.map((family) => stringValue(family["code"])))
  const familyCodes = new Map(
    index.families.flatMap(
      (row) =>
        [
          [stringValue(row["id"]), stringValue(row["code"])],
          [stringValue(row["public_code"]), stringValue(row["code"])],
        ] as [string, string][],
    ),
  )
  const label = (row: JsonObject, field: string): CardSummary["name"] => {
    const unit = index.textUnit(stringValue(row[field]))
    const translations: Partial<Record<TextLang, string>> = {}
    for (const item of arrayValue(row["translations"])) {
      const ref = objectValue(item)
      const translation = index.translation(stringValue(ref["translation_id"]))
      const text = translation && index.textUnit(stringValue(translation["text_unit_id"]))
      if (text)
        translations[stringValue(ref["target_lang"]) as TextLang] = stringValue(text["text"])
    }
    return {
      original: {
        lang: (unit?.["lang"] ?? "ja") as TextLang,
        text: unit ? stringValue(unit["text"]) : stringValue(row["code"]),
      },
      translations,
    }
  }
  const option = (row: JsonObject, field: string): SearchOption => ({
    id:
      typeof row["id"] === "string"
        ? row["id"]
        : `${stringValue(row["kind"])}:${stringValue(row["code"])}`,
    code: stringValue(row["code"]),
    ...(typeof row["public_code"] === "string" ? { publicCode: row["public_code"] } : {}),
    ...(typeof row["kind"] === "string" ? { kind: row["kind"] } : {}),
    name: label(row, field),
  })
  const editionOwners = new Set(
    bootstrap
      .rows("printing")
      .filter((row) => row["region"] === edition)
      .map((row) => stringValue(row["home_set_id"])),
  )
  const options: SearchOptions = {
    classes: bootstrap
      .rows("vocabulary")
      .filter((row) => row["kind"] === "class" && row["active"] === true)
      .map((row) => option(row, "label_unit_id")),
    sets: index.families
      .filter((row) => editionOwners.has(stringValue(row["id"])))
      .map((row) => option(row, "name_unit_id"))
      .sort(
        (a, b) => a.code.localeCompare(b.code, "en", { numeric: true }) || a.id.localeCompare(b.id),
      ),
  }
  const releases: SetRelease[] = datedSets(
    index.families.map((row) => ({
      ...option(row, "name_unit_id"),
      publicCode: stringValue(row["public_code"]),
      kind: stringValue(row["kind"]),
    })),
    bootstrap.rows("product").map((row) => ({
      familyIds: arrayValue(row["family_ids"]).map((id) => stringValue(id)),
      region: stringValue(row["region"]),
      releasedOn:
        row["date_precision"] === "day" && typeof row["released_on"] === "string"
          ? row["released_on"]
          : null,
    })),
    edition,
  )
  const targets = new Columns(pool)
  let targetBatch: JsonObject[] = []
  const cards = uintColumns(CARD_COLUMNS)
  const printings = uintColumns(PRINTING_COLUMNS)
  const names: number[] = []
  const aliases: number[] = []
  const costs: number[] = []
  let batch: CardSummary[] = []
  let summaryCount = 0
  const intern = (value: string | null | undefined) => pool.intern(value ?? "")
  intern("")
  for (const { entry, summaries } of catalogCards(
    index,
    bootstrap.cards(),
    bootstrap.rows("search_alias"),
    sets,
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
      const fullPrinting = index.printing(printing.id)
      const front = index.facesOf(entry.cardId)[0]
      const revision = front ? index.displayRevision(stringValue(front["id"]), edition) : undefined
      const arts = (row: JsonObject | undefined) => {
        const first = Array.isArray(row?.["faces"]) ? row["faces"][0] : undefined
        return first ? stringValue(objectValue(first)["art_id"] ?? "") : ""
      }
      const art = arts(fullPrinting)
      const defaultArt = arts(index.printing(entry.defaultPrinting[edition] ?? ""))
      printings.type.push(
        intern(typeof revision?.["type_code"] === "string" ? revision["type_code"] : ""),
      )
      printings.rarity.push(
        intern(
          typeof fullPrinting?.["rarity_code"] === "string" ? fullPrinting["rarity_code"] : "",
        ),
      )
      printings.art.push(intern(art || printing.id))
      printings.alt.push(Number(!!art && !!defaultArt && art !== defaultArt))
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
      if (summary) {
        batch.push(summary)
        const row = index.printing(printing.id)
        targetBatch.push(
          ...index.facesOf(entry.cardId).map((face) => ({
            printingId: printing.id,
            ...(typeof fullPrinting?.["rarity_code"] === "string"
              ? { rarityCode: fullPrinting["rarity_code"] }
              : {}),
            cardId: entry.cardId,
            faceId: stringValue(face["id"]),
            intId: typeof row?.["int_id"] === "number" ? integerValue(row["int_id"]) : 0,
            ordinal: integerValue(face["ordinal"]),
            owner: stringValue(row?.["home_set_id"]),
          })),
        )
        if (targetBatch.length >= 256) {
          targets.append(targetBatch)
          targetBatch = []
        }
      }
      if (batch.length >= 256) {
        summaryColumns.append(batch as unknown as JsonObject[])
        batch = []
      }
    }
    cards.default.push(preferred)
  }
  summaryColumns.append(batch as unknown as JsonObject[])
  targets.append(targetBatch)
  cards.printingStart.push(printings.id.length)
  cards.nameStart.push(names.length)
  cards.aliasStart.push(aliases.length)
  pool.seal()
  return {
    pool,
    summaries: summaryColumns,
    cards: seal(cards),
    printings: seal(printings),
    names: Uint32Array.from(names),
    aliases: Uint32Array.from(aliases),
    order: cardNumberOrder(
      printings.id.map((id, ordinal) => ({
        lookup: pool.get(printings.lookup[ordinal] ?? 0),
        flat: pool.get(printings.flat[ordinal] ?? 0),
        printingId: pool.get(id),
        cardId: pool.get(cards.id[printings.card[ordinal] ?? 0] ?? 0),
      })),
    ),
    costs: Float64Array.from(costs),
    sets,
    options,
    releases,
    targets,
  }
}

// Images are digest-checked by the cache, so type checks here only guard against layout bugs.
function restore(bytes: Uint8Array, edition: Region): Parts {
  const { header, arrays } = unpack(bytes)
  const image = objectValue(header)
  if (image["edition"] !== edition) throw new Error("index image belongs to another edition")
  let next = 0
  const take = <T extends Uint8Array | Uint32Array | Float64Array>(
    type: new (length: number) => T,
  ): T => {
    const array = arrays[next++]
    if (!(array instanceof type)) throw new Error("index image array has the wrong type")
    return array
  }
  const pool = StringPool.restore(
    arrayValue(image["strings"]).map((value) => stringValue(value)),
    take(Uint32Array),
    take(Float64Array),
  )
  const columns = <T extends string>(names: readonly T[]) =>
    Object.fromEntries(names.map((name) => [name, take(Uint32Array)])) as Record<T, Uint32Array>
  const cards = columns(CARD_COLUMNS)
  const printings = columns(PRINTING_COLUMNS)
  const names = take(Uint32Array)
  const aliases = take(Uint32Array)
  const costs = take(Float64Array)
  const order = take(Uint32Array)
  const readColumns = (value: unknown) =>
    Columns.restore(
      pool,
      arrayValue(value as never).map((value) => {
        const block = objectValue(value)
        return {
          start: integerValue(block["start"]),
          length: integerValue(block["length"]),
          columns: arrayValue(block["columns"]).map((name): [string, Uint8Array, Uint32Array] => [
            stringValue(name),
            take(Uint8Array),
            take(Uint32Array),
          ]),
        }
      }),
    )
  const summaries = Columns.restore(
    pool,
    arrayValue(image["blocks"]).map((value) => {
      const block = objectValue(value)
      return {
        start: integerValue(block["start"]),
        length: integerValue(block["length"]),
        columns: arrayValue(block["columns"]).map((name): [string, Uint8Array, Uint32Array] => [
          stringValue(name),
          take(Uint8Array),
          take(Uint32Array),
        ]),
      }
    }),
  )
  const targets = readColumns(image["targets"])
  const options = image["options"] as unknown as SearchOptions
  const releases = image["releases"] as unknown as SetRelease[]
  if (next !== arrays.length) throw new Error("index image has unread arrays")
  return {
    pool,
    summaries,
    cards,
    printings,
    names,
    aliases,
    costs,
    order,
    sets: new Set(arrayValue(image["sets"]).map((value) => stringValue(value))),
    options,
    releases,
    targets,
  }
}

/** Matching reads scalar columns; summaries are decoded only for the requested page. */
export class SearchIndex {
  private readonly pool: StringPool
  private readonly summaries: Columns
  private readonly cards: Record<(typeof CARD_COLUMNS)[number], Uint32Array>
  private readonly printings: Record<(typeof PRINTING_COLUMNS)[number], Uint32Array>
  private readonly names: Uint32Array
  private readonly aliases: Uint32Array
  private readonly order: Uint32Array
  private readonly costs: Float64Array
  private readonly sets: ReadonlySet<string>
  readonly options: SearchOptions
  readonly releases: readonly SetRelease[]
  private readonly targetGroups = new Map<string, number[]>()
  private readonly targets: Columns
  private readonly faceCards = new Map<string, string>()
  private readonly edition: Region

  /** Builds from staged bootstrap columns, or restores the bytes `encode` produced. */
  constructor(source: BootstrapColumns | Uint8Array, edition: Region) {
    this.edition = edition
    const parts = ArrayBuffer.isView(source) ? restore(source, edition) : build(source, edition)
    this.pool = parts.pool
    this.summaries = parts.summaries
    this.cards = parts.cards
    this.printings = parts.printings
    this.names = parts.names
    this.aliases = parts.aliases
    this.costs = parts.costs
    this.order = parts.order
    this.sets = parts.sets
    this.options = parts.options
    this.releases = parts.releases
    this.targets = parts.targets
    for (let ordinal = 0; ordinal < this.targets.length; ordinal += 1) {
      const id = stringValue(this.targets.value(ordinal, "printingId"))
      const group = this.targetGroups.get(id) ?? []
      group.push(ordinal)
      this.targetGroups.set(id, group)
      this.faceCards.set(
        stringValue(this.targets.value(ordinal, "faceId")),
        stringValue(this.targets.value(ordinal, "cardId")),
      )
    }
  }

  /** Sealed columns and the string pool as one buffer; no row is decoded on either side. */
  encode(): Uint8Array<ArrayBuffer> {
    const pool = this.pool.parts()
    const blocks = this.summaries.parts()
    return pack(
      {
        edition: this.edition,
        sets: [...this.sets],
        options: this.options as unknown as JsonObject,
        releases: this.releases as unknown as JsonObject[],
        targets: this.targets.parts().map(({ start, length, columns }) => ({
          start,
          length,
          columns: columns.map(([name]) => name),
        })),
        strings: pool.values,
        blocks: blocks.map(({ start, length, columns }) => ({
          start,
          length,
          columns: columns.map(([name]) => name),
        })),
      },
      [
        pool.nodes,
        pool.numbers,
        ...CARD_COLUMNS.map((name) => this.cards[name]),
        ...PRINTING_COLUMNS.map((name) => this.printings[name]),
        this.names,
        this.aliases,
        this.costs,
        this.order,
        ...blocks.flatMap(({ columns }) => columns.flatMap(([, kinds, values]) => [kinds, values])),
        ...this.targets
          .parts()
          .flatMap(({ columns }) => columns.flatMap(([, kinds, values]) => [kinds, values])),
      ],
    )
  }

  private matches(query: SearchQuery, eligible?: ReadonlySet<string>): number[] {
    if (query.edition !== this.edition)
      throw new SearchQueryError({ kind: "edition-not-ready", message: "edition is not ready" })
    const state = eligible ? { ...query.state, text: "" } : query.state
    const facets: UnsupportedFacet[] = []
    if (Object.keys(state.mechanics).length) facets.push("mechanics")

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
      (!state.sets.length ||
        state.sets.includes(this.pool.get(this.printings.owner[printing] ?? 0))) &&
      (!state.types.length ||
        state.types.includes(this.pool.get(this.printings.type[printing] ?? 0))) &&
      (!state.rarities.length ||
        state.rarities.includes(this.pool.get(this.printings.rarity[printing] ?? 0))) &&
      (!state.altArtOnly || this.printings.alt[printing] === 1)
    const upper = state.cost.max !== undefined && state.cost.max < 7 ? state.cost.max : undefined
    const costAllowed = (printing: number) => {
      const cost = this.costs[printing] ?? Number.NaN
      return (
        (state.cost.min === undefined && upper === undefined) ||
        (!Number.isNaN(cost) &&
          (state.cost.min === undefined || cost >= state.cost.min) &&
          (upper === undefined || cost <= upper))
      )
    }
    for (let card = 0; card < this.cards.id.length; card += 1) {
      if (eligible && !eligible.has(this.pool.get(this.cards.id[card] ?? 0))) continue
      if (
        state.classes.length &&
        !state.classes.includes(this.pool.get(this.cards.class[card] ?? 0))
      )
        continue
      const start = this.cards.printingStart[card] ?? 0
      const end = this.cards.printingStart[card + 1] ?? start
      if (state.unit === "printing" || state.unit === "art") {
        const nameRank =
          state.text === ""
            ? 0
            : Math.min(
                this.textRank(this.names, this.cards.nameStart, card, flat),
                this.textRank(this.aliases, this.cards.aliasStart, card, flat),
              )
        for (let p = start; p < end; p += 1) {
          if (!allowed(p) || !costAllowed(p)) continue
          let rank = nameRank
          if (state.text !== "") {
            if (!flat) continue
            const number = this.pool.get(this.printings.number[p] ?? 0)
            if (
              (lookup !== null && this.pool.get(this.printings.lookup[p] ?? 0) === lookup) ||
              (digits !== null && number === digits)
            )
              rank = 0
            else if (
              this.pool.get(this.printings.flat[p] ?? 0).startsWith(flat) ||
              (number !== "" &&
                typed !== null &&
                typed.suffix === "" &&
                this.pool.get(this.printings.set[p] ?? 0) === typed.set &&
                number.startsWith(typed.number)) ||
              (number !== "" && digits !== null && number.startsWith(digits))
            )
              rank = 1
          }
          if (rank <= 3) buckets[rank]?.push(p)
        }
        continue
      }
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
      if (!costAllowed(printing)) continue
      buckets[rank]?.push(printing)
    }
    let matches = buckets.flat()
    if (state.text === "") matches.sort((a, b) => (this.order[a] ?? 0) - (this.order[b] ?? 0))
    if (state.unit === "art") {
      const representatives = new Map<number, number>()
      for (const printing of matches) {
        const art = this.printings.art[printing] ?? 0
        const card = this.printings.card[printing] ?? 0
        if (
          !representatives.has(art) ||
          (state.text === "" && this.cards.default[card] === printing)
        )
          representatives.set(art, printing)
      }
      matches = matches.filter(
        (printing) => representatives.get(this.printings.art[printing] ?? 0) === printing,
      )
    }
    const direction = state.order === "desc" ? -1 : 1
    if (state.sort === "no" && direction === -1)
      matches.sort((a, b) => (this.order[b] ?? 0) - (this.order[a] ?? 0))
    if (state.sort !== "no") {
      const summary = (p: number) =>
        this.summaries.row((this.printings.summary[p] ?? 1) - 1) as unknown as CardSummary
      const value = (p: number): number | string | null => {
        const row = summary(p)
        if (state.sort === "name") return row.name.original.text
        if (state.sort === "cost") return row.cost
        if (state.sort === "atk") return row.attack
        if (state.sort === "def") return row.defense
        return null
      }
      if (state.sort === "date")
        throw new SearchQueryError({
          kind: "unsupported-query",
          message: "release-date sorting is not ready",
          facets: ["sort"],
        })
      matches.sort((a, b) => {
        const first = value(a),
          second = value(b)
        return (
          (first === null && second === null
            ? 0
            : first === null
              ? 1
              : second === null
                ? -1
                : typeof first === "string" && typeof second === "string"
                  ? direction * first.localeCompare(second, this.edition === "jp" ? "ja" : "en")
                  : direction * (Number(first) - Number(second))) ||
          (this.order[a] ?? 0) - (this.order[b] ?? 0)
        )
      })
    }
    return matches
  }

  private item(printing: number, state: SearchQuery["state"]): SearchPage["items"][number] {
    const card = this.printings.card[printing] ?? 0
    const summary = this.printings.summary[printing] ?? 0
    return {
      key: this.pool.get(
        state.unit === "printing"
          ? (this.printings.id[printing] ?? 0)
          : state.unit === "art"
            ? (this.printings.art[printing] ?? 0)
            : (this.cards.id[card] ?? 0),
      ),
      printingId: this.pool.get(this.printings.id[printing] ?? 0),
      summary: summary ? (this.summaries.row(summary - 1) as unknown as CardSummary) : undefined,
    }
  }

  search(query: SearchQuery, eligible?: ReadonlySet<string>): SearchPage {
    const matches = this.matches(query, eligible)
    const offset = Math.max(0, Math.trunc(query.offset ?? 0))
    const limit = Math.max(1, Math.min(100, Math.trunc(query.limit ?? 24)))
    return {
      total: matches.length,
      offset,
      items: matches.slice(offset, offset + limit).map((p) => this.item(p, query.state)),
    }
  }

  /** Each matching card once, and how many of them have a name translation in `language`. */
  count(query: SearchQuery, language: TextLang): PackCounts {
    return packCounts(
      this.matches(query).map((printing) => {
        const summary = this.printings.summary[printing] ?? 0
        const name = summary
          ? (this.summaries.row(summary - 1) as unknown as CardSummary).name
          : undefined
        return {
          key: this.pool.get(this.cards.id[this.printings.card[printing] ?? 0] ?? 0),
          translated: Boolean(name?.translations[language]),
          lowConfidence: name?.translationQuality?.[language]?.lowConfidence ?? false,
        }
      }),
    )
  }

  neighbours(
    query: SearchQuery,
    target: string,
    maxItems?: number,
    eligible?: ReadonlySet<string>,
  ): SearchPage {
    const matches = this.matches(query, eligible).slice(
      0,
      maxItems === undefined ? undefined : Math.max(1, Math.trunc(maxItems)),
    )
    const position = matches.findIndex(
      (p) =>
        this.pool.get(this.printings.id[p] ?? 0) === target ||
        (query.state.unit === "art" && this.pool.get(this.printings.art[p] ?? 0) === target) ||
        (query.state.unit === "card" &&
          this.pool.get(this.cards.id[this.printings.card[p] ?? 0] ?? 0) === target),
    )
    return {
      total: matches.length,
      offset: 0,
      items: position < 0 ? [] : matches.map((p) => this.item(p, query.state)),
    }
  }

  cardForFace(faceId: string): string | undefined {
    return this.faceCards.get(faceId)
  }
  imageTargets(ids: readonly string[]): readonly ImageTarget[] {
    return ids.flatMap((id) =>
      (this.targetGroups.get(id) ?? []).map(
        (ordinal) => this.targets.row(ordinal) as unknown as ImageTarget,
      ),
    )
  }

  recent(ids: readonly string[]): readonly RecentSummary[] {
    return ids.slice(0, 20).map((printingId) => {
      for (let p = 0; p < this.printings.id.length; p += 1) {
        if (this.pool.get(this.printings.id[p] ?? 0) !== printingId) continue
        const summary = this.printings.summary[p] ?? 0
        if (summary)
          return {
            printingId,
            state: "available",
            summary: this.summaries.row(summary - 1) as unknown as CardSummary,
          } as const
      }
      return { printingId, state: "not-in-edition" } as const
    })
  }

  suggest(
    text: string,
    limit = 8,
    eligible?: ReadonlySet<string>,
  ): { total: number; items: readonly SuggestedSummary[] } {
    if (eligible) {
      const page = this.search({ state: DEFAULT_QUERY, edition: this.edition, limit }, eligible)
      return {
        total: page.total,
        items: page.items.flatMap((item) =>
          item.summary ? [{ summary: item.summary, field: "effect" as const, rank: 0 }] : [],
        ),
      }
    }
    if (!text) return { total: 0, items: [] }
    const page = this.search({
      state: { ...DEFAULT_QUERY, text },
      edition: this.edition,
      limit: Math.min(20, limit),
    })
    const flat = normalizeText(text)
    const lookup = cardNoLookupKey(text, this.sets)
    const digits = /^\d+$/u.test(flat) ? String(Number(flat)) : null
    return {
      total: page.total,
      items: page.items.flatMap((item) => {
        if (!item.summary) return []
        const card = this.cards.id.findIndex((id) => this.pool.get(id) === item.key)
        const printing = this.printings.id.findIndex((id) => this.pool.get(id) === item.printingId)
        const exact =
          (lookup !== null && this.pool.get(this.printings.lookup[printing] ?? 0) === lookup) ||
          (digits !== null && this.pool.get(this.printings.number[printing] ?? 0) === digits)
        const name = this.textRank(this.names, this.cards.nameStart, card, flat)
        const alias = this.textRank(this.aliases, this.cards.aliasStart, card, flat)
        const isNumber =
          exact ||
          this.pool.get(this.printings.flat[printing] ?? 0).startsWith(flat) ||
          (digits !== null &&
            this.pool.get(this.printings.number[printing] ?? 0).startsWith(digits)) ||
          (cardNoKey(text, this.sets)?.set === this.pool.get(this.printings.set[printing] ?? 0) &&
            this.pool
              .get(this.printings.number[printing] ?? 0)
              .startsWith(cardNoKey(text, this.sets)?.number ?? "!"))
        return [
          {
            summary: item.summary,
            field: isNumber ? "cardNo" : name <= alias ? "name" : "alias",
            rank: exact ? 0 : isNumber ? 1 : Math.min(name, alias),
          },
        ]
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
        this.targets.bytes() +
        this.summaries.bytes() +
        this.pool.bufferBytes() +
        [
          ...Object.values(this.cards),
          ...Object.values(this.printings),
          this.names,
          this.aliases,
          this.costs,
          this.order,
        ].reduce((total, value) => total + value.byteLength, 0),
      stringBytes: this.pool.bytes(),
      cards: this.cards.id.length,
    }
  }
}
