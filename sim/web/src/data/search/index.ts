import { CREDITED_ARTIST, UNCREDITED_ARTIST } from "../../domain/artistFilter"
import { cardNoKey, cardNoLookupKey } from "../../domain/cardNo"
import { deckLocationBadges } from "../../domain/deckLocation"
import {
  type MechanicCoverage,
  type MechanicProjection,
  mechanicState,
} from "../../domain/mechanics"
import { normalizeText } from "../../domain/normalize"
import {
  COST_CAP,
  DEFAULT_QUERY,
  NEUTRAL_CLASS,
  type QueryState,
  STAT_CAP,
  statSelected,
} from "../../domain/query/model"
import { matchesRaritySelection, PREMIUM_RARITY, UNRATED_RARITY } from "../../domain/rarityFilter"
import { pickRepresentative, UNKNOWN_RARITY } from "../../domain/representative"
import type { MatchedField, Region, TextLang } from "../../domain/search"
import { datedSets, type PackCounts, recentSets } from "../../domain/sets"
import { badgeOf, type CardSummary, catalogCards, nameOf } from "../catalog"
import {
  arrayValue,
  integerValue,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "../format-v3/json"
import type { CardRestriction, RestrictionSelection, RestrictionStatus } from "../restrictions"
import type { CardIndex } from "../store"
import type { BootstrapColumns } from "./bootstrap"
import { cardNumberOrder } from "./card-number-order"
import { type ColumnEdit, Columns, StringPool } from "./columns"
import { pack, unpack } from "./image"
import { uniquePromoLeader } from "./promo-leader"

export interface SearchQuery {
  readonly state: QueryState
  readonly edition: Region
  readonly offset?: number
  readonly limit?: number
}
export interface SearchOption {
  readonly id: string
  readonly code: string
  readonly kind?: string
  readonly publicCode?: string
  readonly name: CardSummary["name"]
}
interface ArtistOption {
  readonly id: string
  /** The credit as the official site prints it; never translated. */
  readonly name: string
}
export interface SearchOptions {
  readonly restrictions?: RestrictionStatus
  readonly restrictionDetails?: Readonly<Record<string, readonly CardRestriction[]>>
  readonly keywords: readonly SearchOption[]
  readonly keywordUnknownCards?: number | undefined
  readonly keywordCoverage: "pending" | "unknown" | "partial" | "complete"
  readonly types: readonly SearchOption[]
  /** Printings per printed type code in this edition. */
  readonly typeCounts: Readonly<Record<string, number>>
  /** Printings per deck location token (`main`, `token:spell`, `leader:gold`, …). */
  readonly locationCounts: Readonly<Record<string, number>>
  readonly traits: readonly SearchOption[]
  /** Printings per trait code in this edition. */
  readonly traitCounts: Readonly<Record<string, number>>
  /** Artists credited on this edition's printings; empty until the printing closure completes. */
  readonly artists: readonly ArtistOption[]
  /** Printings per artist id in this edition. */
  readonly artistCounts: Readonly<Record<string, number>>
  readonly prBadgeStyles: readonly string[]
  readonly rarityCounts: Readonly<Record<string, number>>
  readonly rarities: readonly SearchOption[]
  readonly classes: readonly SearchOption[]
  readonly sets: readonly SearchOption[]
  readonly quickSets: {
    readonly recent?: SearchOption
    readonly promo?: SearchOption
  }
  /** Printings per home-set family code in this edition, for the set filter's counts. */
  readonly setCounts: Readonly<Record<string, number>>
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

const CARD_COLUMNS = [
  "id",
  "class",
  "default",
  "printingStart",
  "nameStart",
  "aliasStart",
  "traitStart",
  "kinds",
  "present",
  "absent",
  "errata",
  "restricted",
] as const
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
  "token",
  "premium",
  "rarity",
  "badge",
  "leaderFrame",
  "reprint",
  "stamp",
  "serial",
  "art",
  "alt",
  "releaseDate",
  "artists",
] as const
const uintColumns = <T extends string>(names: readonly T[]) =>
  Object.fromEntries(names.map((name) => [name, [] as number[]])) as Record<T, number[]>
const seal = <T extends string>(columns: Record<T, number[]>): Record<T, Uint32Array> =>
  Object.fromEntries(
    Object.entries(columns).map(([key, values]) => [key, Uint32Array.from(values as number[])]),
  ) as Record<T, Uint32Array>

/** Bump when the stored layout or derived data changes, so older images are rebuilt. */
export const INDEX_IMAGE_FORMAT = 28

// Card-level special kinds, packed into the `kinds` column; Token stays per printing.
const KIND_EVOLVE = 1
const KIND_ADVANCE = 2

/** What decides a printing's deck locations: its type, special kinds and leader frame. */
interface DeckKinds {
  readonly type: string
  readonly token: boolean
  readonly frame: string
  readonly evolve: boolean
  readonly advance: boolean
}

// One parent location per printing, so counts add up and never overlap; combined special kinds
// (none in the data today) resolve in this fixed order, Token first and main last.
function locationOf(kinds: DeckKinds): string {
  if (kinds.token) return "token"
  if (kinds.type === "leader") return "leader"
  if (kinds.type === "ep" || kinds.type === "sep") return kinds.type
  if (kinds.evolve) return "evolve"
  if (kinds.advance) return "advance"
  return "main"
}

/** The parent location a printing counts under, plus its Token kind or leader frame sub-value. */
function locationTokens(kinds: DeckKinds): string[] {
  const parent = locationOf(kinds)
  if (parent === "token") return [parent, `token:${kinds.type}`]
  if (parent === "leader") return [parent, `leader:${kinds.frame}`]
  return [parent]
}

function tally(counts: Record<string, number>, keys: readonly string[]): void {
  for (const key of keys) counts[key] = (counts[key] ?? 0) + 1
}

/** Printings per filter value, built the same way at index build and at printing completion. */
interface FacetCounts {
  readonly typeCounts: Record<string, number>
  readonly locationCounts: Record<string, number>
  readonly traitCounts: Record<string, number>
  readonly setCounts: Record<string, number>
}
const facetCounts = (): FacetCounts => ({
  typeCounts: {},
  locationCounts: {},
  traitCounts: {},
  setCounts: {},
})
function countPrinting(
  counts: FacetCounts,
  kinds: DeckKinds,
  traits: readonly string[],
  setCode: string | undefined,
): void {
  if (kinds.type) tally(counts.typeCounts, [kinds.type])
  tally(counts.locationCounts, locationTokens(kinds))
  tally(counts.traitCounts, traits)
  if (setCode !== undefined) tally(counts.setCounts, [setCode])
}

interface Parts {
  readonly pool: StringPool
  readonly summaries: Columns
  readonly cards: Record<(typeof CARD_COLUMNS)[number], Uint32Array>
  readonly printings: Record<(typeof PRINTING_COLUMNS)[number], Uint32Array>
  readonly names: Uint32Array
  readonly aliases: Uint32Array
  readonly traits: Uint32Array
  readonly order: Uint32Array
  readonly costs: Float64Array
  readonly attacks: Float64Array
  readonly defenses: Float64Array
  readonly sets: ReadonlySet<string>
  readonly options: SearchOptions
  readonly releases: readonly SetRelease[]
  readonly targets: Columns
}

function optionOf(index: CardIndex, row: JsonObject, field: string): SearchOption {
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
  return {
    id:
      typeof row["id"] === "string"
        ? row["id"]
        : `${stringValue(row["kind"])}:${stringValue(row["code"])}`,
    code: stringValue(row["code"]),
    ...(typeof row["public_code"] === "string" ? { publicCode: row["public_code"] } : {}),
    ...(typeof row["kind"] === "string" ? { kind: row["kind"] } : {}),
    name: label(row, field),
  }
}

function optionsOf(
  bootstrap: BootstrapColumns,
  index: CardIndex,
  edition: Region,
  counts: FacetCounts,
): { options: SearchOptions; releases: SetRelease[] } {
  const { typeCounts, locationCounts, traitCounts, setCounts } = counts
  const printingRows = bootstrap.rows("printing")
  const editionOwners = new Set(
    printingRows
      .filter((row) => row["region"] === edition)
      .map((row) => stringValue(row["home_set_id"])),
  )
  const rarityCounts: Record<string, number> = {}
  for (const row of printingRows) {
    // Locator-only rows do not yet carry an official classification.
    if (row["region"] !== edition || !Object.hasOwn(row, "rarity_code")) continue
    const official = typeof row["rarity_code"] === "string" ? row["rarity_code"] : ""
    const premium = row["premium"] === true
    const token = !official || official === "-" ? UNRATED_RARITY : official
    const countToken = premium ? PREMIUM_RARITY : token
    rarityCounts[countToken] = (rarityCounts[countToken] ?? 0) + 1
    if (official === "pr" && !premium) {
      const child = `pr:${badgeOf(row)?.style ?? "unknown"}`
      rarityCounts[child] = (rarityCounts[child] ?? 0) + 1
    }
  }
  const releases: SetRelease[] = datedSets(
    index.families.map((row) => ({
      ...optionOf(index, row, "name_unit_id"),
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
  const vocabulary = (kind: string, counts: Record<string, number>) =>
    bootstrap
      .rows("vocabulary")
      .filter(
        (row) =>
          row["kind"] === kind &&
          row["active"] === true &&
          (counts[stringValue(row["code"])] ?? 0) > 0,
      )
      .map((row) => optionOf(index, row, "label_unit_id"))
  const types = vocabulary("type", typeCounts)
  const traits = vocabulary("trait", traitCounts)
  const setOptions = index.families
    .filter((row) => editionOwners.has(stringValue(row["id"])))
    .map((row) => ({
      ...optionOf(index, row, "name_unit_id"),
      publicCode: stringValue(row["public_code"]),
      kind: stringValue(row["kind"]),
    }))
    .sort(
      (a, b) => a.code.localeCompare(b.code, "en", { numeric: true }) || a.id.localeCompare(b.id),
    )
  const promoSets = setOptions.filter((set) => set.kind === "promo")
  const promoLeader = uniquePromoLeader(promoSets, setCounts)
  const recentSet = recentSets(
    releases.filter((release) => editionOwners.has(release.id)),
    1,
  )[0]
  const options: SearchOptions = {
    keywords: bootstrap.rows("keyword").map((row) =>
      optionOf(
        index,
        {
          ...row,
          translations: arrayValue(row["translations"]).filter(
            (ref) => objectValue(ref)["field"] === "label",
          ),
        },
        "name_unit_id",
      ),
    ),
    keywordCoverage: "unknown",
    rarityCounts,
    typeCounts,
    types,
    locationCounts,
    traits,
    traitCounts,
    artists: [],
    artistCounts: {},
    prBadgeStyles: [
      ...new Set(
        printingRows
          .filter(
            (row) =>
              row["region"] === edition && row["rarity_code"] === "pr" && row["premium"] !== true,
          )
          .map((row) => badgeOf(row)?.style ?? "unknown"),
      ),
    ],
    rarities: bootstrap
      .rows("vocabulary")
      .filter((row) => row["kind"] === "rarity" && row["active"] === true)
      .map((row) => optionOf(index, row, "label_unit_id")),
    classes: bootstrap
      .rows("vocabulary")
      .filter((row) => row["kind"] === "class" && row["active"] === true)
      .map((row) => optionOf(index, row, "label_unit_id")),
    sets: setOptions,
    quickSets: {
      ...(recentSet ? { recent: recentSet } : {}),
      ...(promoLeader ? { promo: promoLeader } : {}),
    },
    setCounts,
  }
  return { options, releases }
}

function artOf(row: JsonObject | undefined): string {
  const first = Array.isArray(row?.["faces"]) ? row["faces"][0] : undefined
  return first ? stringValue(objectValue(first)["art_id"] ?? "") : ""
}

/** Every face's art: a double-faced printing is credited to the artists of both faces. */
function artsOf(row: JsonObject | undefined): string[] {
  const faces = Array.isArray(row?.["faces"]) ? row["faces"] : []
  return faces.flatMap((face) => {
    const art = objectValue(face)["art_id"]
    return typeof art === "string" && art !== "" ? [art] : []
  })
}

/**
 * Artist options with their printing counts. The uncredited count only counts when some printing
 * is credited at all: a snapshot without credits has nothing to tell the two apart.
 */
function artistOptions(counts: Record<string, number>, names: ReadonlyMap<string, string>) {
  const { [UNCREDITED_ARTIST]: uncredited, [CREDITED_ARTIST]: anyCredit, ...credited } = counts
  const hasCredits = Object.keys(credited).length > 0
  return {
    artists: Object.keys(credited).map((id) => ({ id, name: names.get(id) ?? id })),
    artistCounts: {
      ...credited,
      ...(hasCredits && anyCredit ? { [CREDITED_ARTIST]: anyCredit } : {}),
      ...(hasCredits && uncredited ? { [UNCREDITED_ARTIST]: uncredited } : {}),
    },
  }
}

/** The role prefix an edition's credits carry (`illustration_jp`, `illustration_en`). */
const artistRole = (edition: Region) => `illustration_${edition}`

/** Art id to the credited artist ids of one edition, and the names those ids stand for. */
function artistCredits(bootstrap: BootstrapColumns, edition: Region) {
  const names = new Map<string, string>()
  for (const row of bootstrap.rows("artist"))
    names.set(stringValue(row["id"]), stringValue(row["display_name"]))
  const credits = new Map<string, string[]>()
  for (const row of bootstrap.rows("art")) {
    const ids = arrayValue(row["artists"] ?? [])
      .map((raw) => objectValue(raw))
      .filter((credit) => credit["role"] === artistRole(edition))
      .map((credit) => stringValue(credit["artist_id"]))
      .filter((id) => names.has(id))
    if (ids.length) credits.set(stringValue(row["id"]), [...new Set(ids)].sort())
  }
  return { names, credits }
}

function mechanicMembership(bootstrap: BootstrapColumns, edition: Region) {
  const group = (table: string) => {
    const result = new Map<string, JsonObject[]>()
    for (const row of bootstrap.rows(table)) {
      const id = stringValue(row["card_id"])
      result.set(id, [...(result.get(id) ?? []), row])
    }
    return result
  }
  const projections = group("mechanic_projection")
  const coverage = group("card_mechanic_coverage")
  const support = group("card_engine_support")
  const keywords = bootstrap.rows("keyword").map((row) => stringValue(row["id"]))
  const strings = (value: JsonValue | undefined) =>
    arrayValue(value ?? []).map((id) => stringValue(id))
  return (cardId: string) => {
    const projected: MechanicProjection[] = (projections.get(cardId) ?? []).map((row) => ({
      keywordId: stringValue(row["keyword_id"]),
      scope: stringValue(row["scope"]),
      relations: strings(row["relations"]),
    }))
    const covered: MechanicCoverage[] = (coverage.get(cardId) ?? []).map((row) => ({
      scope: stringValue(row["scope"]),
      hasCompleteKeywordIds: strings(row["has_complete_keyword_ids"]),
      completeAll: row["complete_all"] === true,
      completeMode: stringValue(row["complete_mode"]),
      completeKeywordIds: strings(row["complete_keyword_ids"]),
    }))
    const blocked = (support.get(cardId) ?? []).some((row) =>
      arrayValue(row["region_blocks"] ?? []).some(
        (block) => objectValue(block)["region"] === edition,
      ),
    )
    const present: string[] = []
    const absent: string[] = []
    for (const keyword of keywords) {
      const state = mechanicState(keyword, edition, projected, covered, blocked)
      if (state === "present") present.push(keyword)
      if (state === "absent") absent.push(keyword)
    }
    const annotated =
      projected.length + covered.length > 0 &&
      (edition === "jp" ||
        !blocked ||
        projected.some((row) => row.scope === "en_override") ||
        covered.some((row) => row.scope === "en_override"))
    return { present, absent, annotated, unknown: present.length + absent.length < keywords.length }
  }
}

function releaseDates(bootstrap: BootstrapColumns): Map<string, string | null> {
  const products = new Map(bootstrap.rows("product").map((row) => [row["id"], row]))
  const result = new Map<string, string | null>()
  for (const row of bootstrap.rows("printing_product")) {
    const product = products.get(row["product_id"])
    const precision = row["date_precision"] ?? product?.["date_precision"]
    const value = row["date_precision"] == null ? product?.["released_on"] : row["available_on"]
    const date = precision === "day" && typeof value === "string" ? value : null
    const id = stringValue(row["printing_id"])
    const previous = result.get(id)
    result.set(
      id,
      previous === null || date === null
        ? null
        : previous === undefined || date < previous
          ? date
          : previous,
    )
  }
  return result
}

function build(
  bootstrap: BootstrapColumns,
  edition: Region,
  errataCards: ReadonlySet<string>,
  restrictions?: RestrictionSelection,
): Parts {
  const dates = releaseDates(bootstrap)
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
  const targets = new Columns(pool)
  let targetBatch: JsonObject[] = []
  const cards = uintColumns(CARD_COLUMNS)
  const printings = uintColumns(PRINTING_COLUMNS)
  const names: number[] = []
  const aliases: number[] = []
  const traits: number[] = []
  const costs: number[] = []
  const attacks: number[] = []
  const defenses: number[] = []
  const counts = facetCounts()
  const artistCounts: Record<string, number> = {}
  const { names: artistNames, credits: artistsByArt } = artistCredits(bootstrap, edition)
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
    cards.present.push(pool.encode([]))
    cards.absent.push(pool.encode([]))
    const card = cards.id.length
    cards.id.push(intern(entry.cardId))
    cards.errata.push(Number(errataCards.has(entry.cardId)))
    cards.restricted.push(restrictions?.cards.has(entry.cardId) ? 1 : 0)
    cards.class.push(intern(entry.classCode ?? NEUTRAL_CLASS))
    cards.printingStart.push(printings.id.length)
    cards.nameStart.push(names.length)
    cards.aliasStart.push(aliases.length)
    names.push(...entry.names.map((name) => intern(name.normalized)))
    aliases.push(...entry.aliases.map((name) => intern(name.normalized)))
    // Type, traits and special kinds come from the front face's revision shown in this edition.
    const front = index.facesOf(entry.cardId)[0]
    const revision = front ? index.displayRevision(stringValue(front["id"]), edition) : undefined
    const type = typeof revision?.["type_code"] === "string" ? revision["type_code"] : ""
    const special = arrayValue(revision?.["special_kinds"] ?? [])
    const locations = deckLocationBadges(type, special)
    const token = locations.includes("token")
    const evolve = locations.includes("evolve")
    const advance = locations.includes("advance")
    const cardTraits = arrayValue(revision?.["traits"] ?? []).map((value) => stringValue(value))
    cards.kinds.push((evolve ? KIND_EVOLVE : 0) | (advance ? KIND_ADVANCE : 0))
    cards.traitStart.push(traits.length)
    traits.push(...cardTraits.map(intern))
    let preferred = printings.id.length
    for (const printing of entry.printings) {
      if (printing.id === entry.defaultPrinting[edition]) preferred = printings.id.length
      const summary = summaries.find((value) => value.printingId === printing.id)
      const fullPrinting = index.printing(printing.id)
      const art = artOf(fullPrinting)
      const defaultArt = artOf(index.printing(entry.defaultPrinting[edition] ?? ""))
      printings.type.push(intern(type))
      printings.token.push(Number(token))
      printings.premium.push(Number(fullPrinting?.["premium"] === true))
      const leaderFrame =
        typeof fullPrinting?.["leader_name_frame"] === "string"
          ? fullPrinting["leader_name_frame"]
          : "unknown"
      printings.leaderFrame.push(intern(leaderFrame))
      printings.reprint.push(Number(fullPrinting?.["reprint"] === true))
      printings.stamp.push(Number(fullPrinting?.["stamp"] != null))
      printings.serial.push(Number(fullPrinting?.["serial"] === true))
      // A locator row has no classification yet; a full row without a code has none at all.
      printings.rarity.push(
        intern(
          !fullPrinting || !Object.hasOwn(fullPrinting, "rarity_code")
            ? UNKNOWN_RARITY
            : typeof fullPrinting["rarity_code"] === "string"
              ? fullPrinting["rarity_code"]
              : "",
        ),
      )
      printings.badge.push(
        intern(fullPrinting ? (badgeOf(fullPrinting)?.style ?? "unknown") : "unknown"),
      )
      printings.art.push(intern(art || printing.id))
      const credited = [
        ...new Set(artsOf(fullPrinting).flatMap((value) => artistsByArt.get(value) ?? [])),
      ].sort()
      printings.artists.push(pool.encode(credited))
      tally(artistCounts, credited.length ? [...credited, CREDITED_ARTIST] : [UNCREDITED_ARTIST])
      printings.alt.push(Number(!!art && !!defaultArt && art !== defaultArt))
      printings.id.push(intern(printing.id))
      printings.releaseDate.push(intern(dates.get(printing.id)))
      printings.lookup.push(intern(printing.lookupKey))
      printings.flat.push(intern(printing.flat))
      printings.set.push(intern(printing.key?.set))
      printings.number.push(intern(printing.key?.number))
      const owner = index.printing(printing.id)?.["home_set_id"]
      const ownerCode = typeof owner === "string" ? familyCodes.get(owner) : undefined
      printings.owner.push(intern(ownerCode))
      countPrinting(
        counts,
        { type, token, frame: leaderFrame, evolve, advance },
        cardTraits,
        ownerCode,
      )
      printings.card.push(card)
      printings.summary.push(summary ? ++summaryCount : 0)
      costs.push(summary?.cost ?? Number.NaN)
      attacks.push(summary?.attack ?? Number.NaN)
      defenses.push(summary?.defense ?? Number.NaN)
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
  const { options, releases } = optionsOf(bootstrap, index, edition, counts)
  summaryColumns.append(batch as unknown as JsonObject[])
  targets.append(targetBatch)
  cards.printingStart.push(printings.id.length)
  cards.nameStart.push(names.length)
  cards.aliasStart.push(aliases.length)
  cards.traitStart.push(traits.length)
  pool.seal()
  return {
    pool,
    summaries: summaryColumns,
    cards: seal(cards),
    printings: seal(printings),
    names: Uint32Array.from(names),
    aliases: Uint32Array.from(aliases),
    traits: Uint32Array.from(traits),
    order: cardNumberOrder(
      printings.id.map((id, ordinal) => ({
        lookup: pool.get(printings.lookup[ordinal] ?? 0),
        flat: pool.get(printings.flat[ordinal] ?? 0),
        printingId: pool.get(id),
        cardId: pool.get(cards.id[printings.card[ordinal] ?? 0] ?? 0),
      })),
    ),
    costs: Float64Array.from(costs),
    attacks: Float64Array.from(attacks),
    defenses: Float64Array.from(defenses),
    sets,
    options: {
      ...options,
      ...artistOptions(artistCounts, artistNames),
      keywordCoverage: "pending",
      ...(restrictions
        ? {
            restrictions: restrictions.status,
            restrictionDetails: Object.fromEntries(restrictions.details),
          }
        : {}),
    },
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
  const traits = take(Uint32Array)
  const costs = take(Float64Array)
  const attacks = take(Float64Array)
  const defenses = take(Float64Array)
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
    traits,
    costs,
    attacks,
    defenses,
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
  private summaries: Columns
  private readonly cards: Record<(typeof CARD_COLUMNS)[number], Uint32Array>
  private printings: Record<(typeof PRINTING_COLUMNS)[number], Uint32Array>
  private readonly names: Uint32Array
  private readonly aliases: Uint32Array
  private readonly traits: Uint32Array
  private order: Uint32Array
  private costs: Float64Array
  private attacks: Float64Array
  private defenses: Float64Array
  private readonly sets: ReadonlySet<string>
  private optionsValue: SearchOptions
  private releasesValue: readonly SetRelease[]
  private readonly targetGroups = new Map<string, number[]>()
  private targets: Columns
  private readonly faceCards = new Map<string, string>()
  private readonly edition: Region

  /** Builds from staged bootstrap columns, or restores the bytes `encode` produced. */
  constructor(
    source: BootstrapColumns | Uint8Array,
    edition: Region,
    errataCards: ReadonlySet<string> = new Set(),
    restrictions?: RestrictionSelection,
  ) {
    this.edition = edition
    const parts = ArrayBuffer.isView(source)
      ? restore(source, edition)
      : build(source, edition, errataCards, restrictions)
    this.pool = parts.pool
    this.summaries = parts.summaries
    this.cards = parts.cards
    this.printings = parts.printings
    this.names = parts.names
    this.aliases = parts.aliases
    this.traits = parts.traits
    this.costs = parts.costs
    this.attacks = parts.attacks
    this.defenses = parts.defenses
    this.order = parts.order
    this.sets = parts.sets
    this.optionsValue = parts.options
    this.releasesValue = parts.releases
    this.targets = parts.targets
    this.indexTargets()
  }

  get options(): SearchOptions {
    return this.optionsValue
  }
  get releases(): readonly SetRelease[] {
    return this.releasesValue
  }

  private indexTargets(): void {
    this.targetGroups.clear()
    this.faceCards.clear()
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

  /** Locators already enumerate the printing IDs; completion replaces their metadata only. */
  completePrintings(bootstrap: BootstrapColumns): void {
    const dates = releaseDates(bootstrap)
    const index = bootstrap.index()
    const slots = new Map(
      Array.from(this.printings.id, (id, ordinal) => [this.pool.get(id), ordinal]),
    )
    const familyCodes = new Map(
      index.families.map((row) => [stringValue(row["id"]), stringValue(row["code"])]),
    )
    const entries: { row: JsonObject; old: number; card: number }[] = []
    const defaults = new Uint32Array(this.cards.default.length)
    const preferredIds: (JsonValue | undefined)[] = []
    for (let card = 0; card < this.cards.id.length; card++) {
      const id = this.pool.get(this.cards.id[card] ?? 0)
      const rows = index.printingsOf(id).filter((row) => row["region"] === this.edition)
      if (
        rows.length !==
        (this.cards.printingStart[card + 1] ?? 0) - (this.cards.printingStart[card] ?? 0)
      )
        throw new Error("printing closure changes catalog membership")
      const preferred =
        ((index.card(id)?.["regions"] ?? []) as JsonObject[]).find(
          (row) => row["region"] === this.edition,
        )?.["default_printing_id"] ?? rows[0]?.["id"]
      preferredIds[card] = preferred
      defaults[card] = entries.length
      for (const row of rows) {
        const old = slots.get(stringValue(row["id"]))
        if (old === undefined || this.printings.card[old] !== card)
          throw new Error("printing closure is missing its catalog locator")
        if (row["id"] === preferred) defaults[card] = entries.length
        entries.push({ row, old, card })
      }
    }
    const permutation = entries.map((entry) => entry.old)
    const printings = Object.fromEntries(
      PRINTING_COLUMNS.map((name) => [
        name,
        Uint32Array.from(permutation, (old) => this.printings[name][old] ?? 0),
      ]),
    ) as Parts["printings"]
    const summaryBatches: ColumnEdit[][] = []
    const targetBatches: ColumnEdit[][] = []
    let summaries: ColumnEdit[] = [],
      targets: ColumnEdit[] = []
    let summaryCount = 0
    const counts = facetCounts()
    const artistCounts: Record<string, number> = {}
    const { names: artistNames, credits: artistsByArt } = artistCredits(bootstrap, this.edition)
    const rarities = new Map<string, JsonValue | undefined>()
    this.pool.reopen()
    try {
      for (const [ordinal, { row, old, card }] of entries.entries()) {
        const id = stringValue(row["id"])
        const cardId = this.pool.get(this.cards.id[card] ?? 0)
        const cardRow = index.card(cardId)
        const preferred = preferredIds[card]
        const art = artOf(row),
          defaultArt = artOf(index.printing(typeof preferred === "string" ? preferred : ""))
        const frame =
          typeof row["leader_name_frame"] === "string" ? row["leader_name_frame"] : "unknown"
        const official = typeof row["rarity_code"] === "string" ? row["rarity_code"] : ""
        const cardNo = stringValue(row["card_no"]),
          key = cardNoKey(cardNo, this.sets)
        const owner = stringValue(row["home_set_id"]),
          code = familyCodes.get(owner)
        const intern = (value: string | undefined | null) => this.pool.intern(value ?? "")
        printings.lookup[ordinal] = intern(cardNoLookupKey(cardNo, this.sets))
        printings.premium[ordinal] = Number(row["premium"] === true)
        printings.flat[ordinal] = intern(normalizeText(cardNo))
        printings.set[ordinal] = intern(key?.set)
        printings.number[ordinal] = intern(key?.number)
        printings.owner[ordinal] = intern(code)
        printings.releaseDate[ordinal] = intern(dates.get(id))
        printings.leaderFrame[ordinal] = intern(frame)
        printings.reprint[ordinal] = Number(row["reprint"] === true)
        printings.stamp[ordinal] = Number(row["stamp"] != null)
        printings.serial[ordinal] = Number(row["serial"] === true)
        printings.rarity[ordinal] = intern(official)
        printings.badge[ordinal] = intern(badgeOf(row)?.style ?? "unknown")
        printings.art[ordinal] = intern(art || id)
        printings.alt[ordinal] = Number(!!art && !!defaultArt && art !== defaultArt)
        const credited = [...new Set(artsOf(row).flatMap((value) => artistsByArt.get(value) ?? []))]
        printings.artists[ordinal] = this.pool.encode(credited.sort())
        tally(artistCounts, credited.length ? [...credited, CREDITED_ARTIST] : [UNCREDITED_ARTIST])
        const bits = this.cards.kinds[card] ?? 0
        countPrinting(
          counts,
          {
            type: this.pool.get(printings.type[ordinal] ?? 0),
            token: printings.token[ordinal] === 1,
            frame,
            evolve: (bits & KIND_EVOLVE) !== 0,
            advance: (bits & KIND_ADVANCE) !== 0,
          },
          this.traitsOf(card),
          code,
        )
        const summary = this.printings.summary[old] ?? 0
        if (summary) {
          if (!rarities.has(official)) {
            const vocabulary = index.vocabulary("rarity", official)
            rarities.set(
              official,
              vocabulary ? (nameOf(index, vocabulary, "label") as unknown as JsonValue) : undefined,
            )
          }
          const intId = typeof row["int_id"] === "number" ? row["int_id"] : undefined
          summaries.push({
            source: summary - 1,
            changes: {
              cardNo,
              route: { cardOwner: stringValue(cardRow?.["home_set_id"]), printingOwner: owner },
              intId,
              cardNoState:
                typeof row["card_no_state"] === "string" ? row["card_no_state"] : undefined,
              rarityCode:
                typeof row["rarity_code"] === "string"
                  ? row["rarity_code"]
                  : row["rarity_raw"] === "-"
                    ? "-"
                    : null,
              rarityBadge: badgeOf(row),
              premium: typeof row["premium"] === "boolean" ? row["premium"] : null,
              rarity: rarities.get(official),
            },
          })
          printings.summary[ordinal] = ++summaryCount
          for (const source of this.targetGroups.get(id) ?? [])
            targets.push({
              source,
              changes: {
                intId: intId ?? 0,
                owner,
                rarityCode: typeof row["rarity_code"] === "string" ? row["rarity_code"] : undefined,
              },
            })
          if (targets.length >= 256) {
            targetBatches.push(targets)
            targets = []
          }
          if (summaries.length >= 256) {
            summaryBatches.push(summaries)
            summaries = []
          }
        }
      }
      summaryBatches.push(summaries)
      targetBatches.push(targets)
      const completedSummaries = this.summaries.remap(summaryBatches)
      const completedTargets = this.targets.remap(targetBatches)
      const { options, releases } = optionsOf(bootstrap, index, this.edition, counts)
      const order = cardNumberOrder(
        entries.map(({ card }, ordinal) => ({
          lookup: this.pool.get(printings.lookup[ordinal] ?? 0),
          flat: this.pool.get(printings.flat[ordinal] ?? 0),
          printingId: this.pool.get(printings.id[ordinal] ?? 0),
          cardId: this.pool.get(this.cards.id[card] ?? 0),
        })),
      )
      this.costs = Float64Array.from(permutation, (old) => this.costs[old] ?? Number.NaN)
      this.attacks = Float64Array.from(permutation, (old) => this.attacks[old] ?? Number.NaN)
      this.defenses = Float64Array.from(permutation, (old) => this.defenses[old] ?? Number.NaN)
      this.cards.default = defaults
      this.printings = printings
      this.summaries = completedSummaries
      this.targets = completedTargets
      this.order = order
      this.optionsValue = {
        ...options,
        ...artistOptions(artistCounts, artistNames),
        keywordCoverage: this.optionsValue.keywordCoverage,
        keywordUnknownCards: this.optionsValue.keywordUnknownCards,
        ...(this.optionsValue.restrictions
          ? {
              restrictions: this.optionsValue.restrictions,
              restrictionDetails: this.optionsValue.restrictionDetails,
            }
          : {}),
      }
      this.releasesValue = releases
      this.indexTargets()
    } finally {
      this.pool.seal()
    }
  }

  /** Completes only mechanic membership after its independent payload closure is verified. */
  completeMechanics(bootstrap: BootstrapColumns): void {
    this.pool.reopen()
    try {
      const membership = mechanicMembership(bootstrap, this.edition)
      let known = false
      let unknown = 0
      for (let card = 0; card < this.cards.id.length; card++) {
        const states = membership(this.pool.get(this.cards.id[card] ?? 0))
        this.cards.present[card] = this.pool.encode(states.present)
        this.cards.absent[card] = this.pool.encode(states.absent)
        known ||= states.annotated
        unknown += Number(states.unknown)
      }
      this.optionsValue = {
        ...this.optionsValue,
        keywordCoverage: known ? (unknown ? "partial" : "complete") : "unknown",
        keywordUnknownCards: unknown,
      }
    } finally {
      this.pool.seal()
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
        this.traits,
        this.costs,
        this.attacks,
        this.defenses,
        this.order,
        ...blocks.flatMap(({ columns }) => columns.flatMap(([, kinds, values]) => [kinds, values])),
        ...this.targets
          .parts()
          .flatMap(({ columns }) => columns.flatMap(([, kinds, values]) => [kinds, values])),
      ],
    )
  }

  private matches(query: SearchQuery, eligible?: ReadonlySet<string>, ranked = false): number[] {
    if (query.edition !== this.edition)
      throw new SearchQueryError({ kind: "edition-not-ready", message: "edition is not ready" })
    const state = eligible ? { ...query.state, text: "" } : query.state
    const facets: UnsupportedFacet[] = []
    if (
      Object.keys(state.mechanics).length > 0 &&
      (this.options.keywordCoverage === "unknown" ||
        this.options.keywordCoverage === "pending" ||
        Object.keys(state.mechanics).some(
          (id) => !this.options.keywords.some((keyword) => keyword.id === id),
        ))
    )
      facets.push("mechanics")

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
      (!state.locations.length ||
        locationTokens(this.kindsOf(printing)).some((token) => state.locations.includes(token))) &&
      (!state.traits.length ||
        this.traitsOf(this.printings.card[printing] ?? 0).some((trait) =>
          state.traits.includes(trait),
        )) &&
      (!state.artists.length || this.artistAllowed(state.artists, printing)) &&
      matchesRaritySelection(
        this.pool.get(this.printings.rarity[printing] ?? 0),
        this.printings.premium[printing] === 1,
        state.rarities,
        this.pool.get(this.printings.badge[printing] ?? 0),
      ) &&
      (!state.altArtOnly || this.printings.alt[printing] === 1) &&
      (!state.stampOnly || this.printings.stamp[printing] === 1) &&
      (!state.serialOnly || this.printings.serial[printing] === 1) &&
      // A merged-by-card cell stands for every printing, so hiding reprints cannot apply there.
      (state.reprints === "all" ||
        state.unit === "card" ||
        (this.printings.reprint[printing] === 1) === (state.reprints === "only"))
    // Stats come from the opened printing's summary; a selection within one stat is a union and
    // the three stats intersect.
    const statsAllowed = (printing: number) =>
      statSelected(state.cost, COST_CAP, this.costs[printing]) &&
      statSelected(state.attack, STAT_CAP, this.attacks[printing]) &&
      statSelected(state.defense, STAT_CAP, this.defenses[printing])
    // A printing's text rank: 0 for its exact number, 1 for a number prefix, else the card's
    // name/alias rank (2 prefix, 3 contains, 4 none).
    const textRankOf = (p: number, nameRank: number): number => {
      const number = this.pool.get(this.printings.number[p] ?? 0)
      if (
        (lookup !== null && this.pool.get(this.printings.lookup[p] ?? 0) === lookup) ||
        (digits !== null && number === digits)
      )
        return 0
      if (
        this.pool.get(this.printings.flat[p] ?? 0).startsWith(flat) ||
        (number !== "" &&
          typed !== null &&
          typed.suffix === "" &&
          this.pool.get(this.printings.set[p] ?? 0) === typed.set &&
          number.startsWith(typed.number)) ||
        (number !== "" && digits !== null && number.startsWith(digits))
      )
        return 1
      return nameRank
    }
    const positive = Object.keys(state.mechanics).filter((id) => state.mechanics[id] === "has")
    const negative = Object.keys(state.mechanics).filter((id) => state.mechanics[id] === "not")
    for (let card = 0; card < this.cards.id.length; card += 1) {
      if (state.errataOnly && this.cards.errata[card] !== 1) continue
      if (state.restrictionsOnly && this.cards.restricted[card] !== 1) continue
      if (
        positive.length &&
        !arrayValue(this.pool.decode(this.cards.present[card] ?? 0)).some((id) =>
          positive.includes(stringValue(id)),
        )
      )
        continue
      if (
        negative.length &&
        !negative.every((id) =>
          arrayValue(this.pool.decode(this.cards.absent[card] ?? 0)).includes(id),
        )
      )
        continue
      if (eligible && !eligible.has(this.pool.get(this.cards.id[card] ?? 0))) continue
      if (
        state.classes.length &&
        !state.classes.includes(this.pool.get(this.cards.class[card] ?? 0))
      )
        continue
      const start = this.cards.printingStart[card] ?? 0
      const end = this.cards.printingStart[card + 1] ?? start
      if (state.text !== "" && !flat) continue
      const nameRank =
        state.text === ""
          ? 0
          : Math.min(
              this.textRank(this.names, this.cards.nameStart, card, flat),
              this.textRank(this.aliases, this.cards.aliasStart, card, flat),
            )
      if (state.unit === "printing") {
        for (let p = start; p < end; p += 1) {
          if (!allowed(p) || !statsAllowed(p)) continue
          const rank = state.text === "" ? 0 : textRankOf(p, nameRank)
          if (rank <= 3) buckets[rank]?.push(p)
        }
        continue
      }
      // Merged units: the printings that pass every filter form one group per card or per art,
      // and the group's best text rank decides which of them may stand for it (#781).
      const groups = new Map<number, { readonly p: number; readonly rank: number }[]>()
      for (let p = start; p < end; p += 1) {
        if (!allowed(p) || !statsAllowed(p)) continue
        const rank = state.text === "" ? 0 : textRankOf(p, nameRank)
        if (rank > 3) continue
        const key = state.unit === "art" ? (this.printings.art[p] ?? 0) : card
        const group = groups.get(key) ?? []
        group.push({ p, rank })
        groups.set(key, group)
      }
      for (const members of groups.values()) {
        const top = Math.min(...members.map((member) => member.rank))
        const representative = pickRepresentative(
          members.filter((member) => member.rank === top).map((member) => member.p),
          (p) => this.pool.get(this.printings.rarity[p] ?? 0),
          (p) => this.order[p] ?? 0,
          { rarity: state.representative, prFirst: state.prFirst },
        )
        if (representative !== undefined) buckets[top]?.push(representative)
      }
    }
    const byNumber = (a: number, b: number) => (this.order[a] ?? 0) - (this.order[b] ?? 0)
    const matches = ranked
      ? buckets.flatMap((bucket) => bucket.sort(byNumber))
      : buckets.flat().sort(byNumber)
    const direction = state.order === "desc" ? -1 : 1
    if (state.sort === "no" && direction === -1)
      matches.sort((a, b) => (this.order[b] ?? 0) - (this.order[a] ?? 0))
    if (state.sort !== "no") {
      const summary = (p: number) =>
        this.summaries.row((this.printings.summary[p] ?? 1) - 1) as unknown as CardSummary
      const value = (p: number): number | string | null => {
        if (state.sort === "date") return this.pool.get(this.printings.releaseDate[p] ?? 0) || null
        const row = summary(p)
        if (state.sort === "name") return row.name.original.text
        if (state.sort === "cost") return row.cost
        if (state.sort === "atk") return row.attack
        if (state.sort === "def") return row.defense
        return null
      }
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
    const restrictions =
      this.optionsValue.restrictionDetails?.[this.pool.get(this.cards.id[card] ?? 0)]
    return {
      key: this.pool.get(
        state.unit === "printing"
          ? (this.printings.id[printing] ?? 0)
          : state.unit === "art"
            ? (this.printings.art[printing] ?? 0)
            : (this.cards.id[card] ?? 0),
      ),
      printingId: this.pool.get(this.printings.id[printing] ?? 0),
      summary: summary
        ? {
            ...(this.summaries.row(summary - 1) as unknown as CardSummary),
            ...(restrictions?.length ? { restrictions } : {}),
          }
        : undefined,
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

  /** Each matching card once, whatever number of its printings match. */
  count(query: SearchQuery): PackCounts {
    const cards = new Set(
      this.matches(query).map((printing) => this.cards.id[this.printings.card[printing] ?? 0]),
    )
    return { cards: cards.size }
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
      const page = this.search(
        { state: { ...DEFAULT_QUERY, unit: "card" }, edition: this.edition, limit },
        eligible,
      )
      return {
        total: page.total,
        items: page.items.flatMap((item) =>
          item.summary ? [{ summary: item.summary, field: "effect" as const, rank: 0 }] : [],
        ),
      }
    }
    if (!text) return { total: 0, items: [] }
    const flat = normalizeText(text)
    const numberQuery =
      this.sets.has(flat) || cardNoKey(text, this.sets) !== null || /^\d+$/u.test(flat)
    const state = { ...DEFAULT_QUERY, unit: "card" as const, text }
    const matches = this.matches({ state, edition: this.edition }, undefined, !numberQuery)
    const page = {
      total: matches.length,
      items: matches.slice(0, Math.max(1, Math.min(20, limit))).map((p) => this.item(p, state)),
    }
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

  private kindsOf(printing: number): DeckKinds {
    const bits = this.cards.kinds[this.printings.card[printing] ?? 0] ?? 0
    return {
      type: this.pool.get(this.printings.type[printing] ?? 0),
      token: this.printings.token[printing] === 1,
      frame: this.pool.get(this.printings.leaderFrame[printing] ?? 0),
      evolve: (bits & KIND_EVOLVE) !== 0,
      advance: (bits & KIND_ADVANCE) !== 0,
    }
  }

  /** Any selected artist credits the printing, or the uncredited choice and nobody does. */
  private artistAllowed(selected: readonly string[], printing: number): boolean {
    const credited = arrayValue(this.pool.decode(this.printings.artists[printing] ?? 0))
    return credited.length === 0
      ? Object.hasOwn(this.optionsValue.artistCounts, UNCREDITED_ARTIST) &&
          selected.includes(UNCREDITED_ARTIST)
      : selected.includes(CREDITED_ARTIST) ||
          credited.some((id) => selected.includes(stringValue(id)))
  }

  private traitsOf(card: number): string[] {
    const start = this.cards.traitStart[card] ?? 0
    const end = this.cards.traitStart[card + 1] ?? start
    const codes: string[] = []
    for (let i = start; i < end; i += 1) codes.push(this.pool.get(this.traits[i] ?? 0))
    return codes
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
          this.attacks,
          this.defenses,
          this.order,
        ].reduce((total, value) => total + value.byteLength, 0),
      stringBytes: this.pool.bytes(),
      cards: this.cards.id.length,
    }
  }
}
