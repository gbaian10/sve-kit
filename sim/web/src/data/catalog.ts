import { cardNoKey, cardNoLookupKey } from "../domain/cardNo"
import { type BadgeLocation, deckLocationBadges } from "../domain/deckLocation"
import type { NameSource } from "../domain/nameDisplay"
import { NORMALIZER_VERSION, normalizeText } from "../domain/normalize"
import type { QueryState } from "../domain/query/model"
import type { RarityBadge } from "../domain/rarityBadges"
import {
  apply,
  type Region,
  type ResultItem,
  type SearchEntry,
  type SearchName,
  type SearchPrinting,
  suggest,
  type Suggestion,
  type TextLang,
} from "../domain/search"
import { CatalogBudget } from "./catalog-cursor"
import { catalogWork } from "./catalog-timing"
import type { LoadedSnapshot } from "./client"
import { drain } from "./format-v3/cursor"
import type { Row } from "./format-v3/decode"
import {
  arrayValue,
  integerValue,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "./format-v3/json"
import { mergedRowsCursor, validateCatalogCursor } from "./format-v3/reader"
import { type CardIndex, cardIndexCursor } from "./store"

/** What a list cell or suggestion row shows for one card without loading any detail file. */
export interface CardSummary {
  readonly cardId: string
  readonly printingId: string
  readonly faceId: string
  readonly cardNo: string
  readonly region: Region
  readonly route?: { readonly cardOwner: string; readonly printingOwner: string }
  readonly intId?: number
  readonly cardNoState?: string
  readonly classCode: string | null
  readonly name: NameSource
  readonly cost: number | null
  readonly attack: number | null
  readonly defense: number | null
  readonly typeCode?: string | null
  readonly deckLocations?: readonly BadgeLocation[]
  readonly rarityCode?: string | null
  readonly premium?: boolean | null
  readonly rarity?: NameSource
  readonly rarityBadge?: RarityBadge | null
  readonly wordingPending?: boolean
}

export interface Catalog {
  readonly index: CardIndex
  readonly entries: readonly SearchEntry[]
  /** Set codes of the snapshot, for card-number parsing. */
  readonly sets: ReadonlySet<string>
  /** Active class codes in vocabulary order. */
  readonly classCodes: readonly string[]
  readonly suggest: (text: string, edition: Region, limit: number) => Suggestion[]
  readonly results: (state: QueryState, edition: Region) => ResultItem[]
  /** The summary of a printing's front face, or undefined for an unknown printing id. */
  readonly summary: (printingId: string) => CardSummary | undefined
  /** The class label for the UI language (vocabulary translation), else the Japanese label. */
  readonly classLabel: (code: string, lang: TextLang) => string
  /** False when the snapshot's alias normalizer differs from ours, so alias hits may be missed. */
  readonly normalizerMatches: boolean
}

const LANGS: readonly TextLang[] = ["ja", "en", "zh-Hant"]

function isTextLang(value: string): value is TextLang {
  return (LANGS as readonly string[]).includes(value)
}

function isRegion(value: string): value is Region {
  return value === "jp" || value === "en"
}

/** Text of the translation a FieldTranslation points at, when the bootstrap ships it. */
function translationText(index: CardIndex, translationId: string): string | undefined {
  const translation = index.translation(translationId)
  if (!translation) return undefined
  const unit = index.textUnit(stringValue(translation["text_unit_id"]))
  return unit ? stringValue(unit["text"]) : undefined
}

export function nameOf(
  index: CardIndex,
  revision: Row,
  field: "name" | "label" = "name",
  basis: Row = revision,
): NameSource | undefined {
  const unit = index.textUnit(stringValue(revision[`${field}_unit_id`]))
  if (!unit) return undefined
  const lang = stringValue(unit["lang"])
  if (!isTextLang(lang)) return undefined
  const translations: Partial<Record<TextLang, string>> = {}
  const translationQuality: NonNullable<NameSource["translationQuality"]> = {}
  for (const entry of basis["translations"] as Row[]) {
    if (entry["field"] !== field) continue
    const target = stringValue(entry["target_lang"])
    const text = translationText(index, stringValue(entry["translation_id"]))
    if (isTextLang(target) && text !== undefined) {
      translations[target] = text
      const translation = index.translation(stringValue(entry["translation_id"]))
      translationQuality[target] = {
        lowConfidence: translation?.["low_confidence"] === true,
        jpSource: entry["basis"] === "jp_source",
      }
    }
  }
  return { original: { lang, text: stringValue(unit["text"]) }, translations, translationQuality }
}

function searchName(lang: TextLang, text: string): SearchName {
  return { lang, text, normalized: normalizeText(text) }
}

function nullableInteger(value: JsonValue | undefined): number | null {
  return value === null || value === undefined ? null : integerValue(value)
}

/** The exporter and Python reader validate this object; the browser trusts verified data. */
export function badgeOf(row: JsonObject): RarityBadge | null {
  return (row["rarity_badge"] as RarityBadge | null | undefined) ?? null
}

/** Official rarity controls sorting; the appearance only describes this printing's badge. */
function printingRarity(row: JsonObject) {
  return {
    code: typeof row["rarity_code"] === "string" ? row["rarity_code"] : null,
    raw: typeof row["rarity_raw"] === "string" ? row["rarity_raw"] : "",
    premium: typeof row["premium"] === "boolean" ? row["premium"] : null,
    badge: badgeOf(row),
  }
}

/** The Worker consumes one card at a time, without constructing another region's summaries. */
export function* catalogCards(
  index: CardIndex,
  cards: Iterable<Row>,
  aliasRows: readonly Row[],
  sets: ReadonlySet<string>,
  edition?: Region,
): Generator<{ readonly entry: SearchEntry; readonly summaries: readonly CardSummary[] }> {
  for (const card of catalogCardCursor(index, cards, aliasRows, sets, edition))
    if (card !== undefined) yield card
}

function* catalogCardCursor(
  index: CardIndex,
  cards: Iterable<Row>,
  aliasRows: readonly Row[],
  sets: ReadonlySet<string>,
  edition?: Region,
): Generator<
  undefined | { readonly entry: SearchEntry; readonly summaries: readonly CardSummary[] }
> {
  let visited = 0
  // Aliases of kind `card` name the card by its id without the `c:` prefix (Code has no colon).
  const aliases = new Map<string, SearchName[]>()
  for (const row of aliasRows) {
    if ((++visited & 15) === 0) yield
    if (row["kind"] !== "card") continue
    const lang = stringValue(row["lang"])
    if (!isTextLang(lang)) continue
    const cardId = `c:${stringValue(row["code"])}`
    aliases.set(cardId, [
      ...(aliases.get(cardId) ?? []),
      searchName(lang, stringValue(row["text"])),
    ])
  }
  let order = -1
  for (const card of cards) {
    if ((++visited & 15) === 0) yield
    order += 1
    const summaries: CardSummary[] = []
    const cardId = stringValue(card["id"])
    const front = index.facesOf(cardId)[0]
    if (!front) continue
    const faceId = stringValue(front["id"])
    const names: SearchName[] = []
    const seen = new Set<string>()
    const defaultPrinting: Partial<Record<Region, string>> = {}
    let classCode: string | null = null
    for (const view of card["regions"] as Row[]) {
      const region = stringValue(view["region"])
      if (!isRegion(region)) continue
      const printingId = view["default_printing_id"]
      if (typeof printingId === "string") defaultPrinting[region] = printingId
      const revision = index.displayRevision(faceId, region)
      const pending = index.wording(faceId, region)
      if (!revision && !pending) continue
      const code = revision?.["class_code"]
      if (typeof code === "string") classCode = code
      const printing = index.printing(typeof printingId === "string" ? printingId : "")
      const nameRow = revision ?? (pending?.["common_name"] as Row | null | undefined)
      const name = nameRow
        ? nameOf(index, nameRow)
        : printing
          ? {
              original: {
                lang: region === "jp" ? ("ja" as const) : ("en" as const),
                text: stringValue(printing["card_no"]),
              },
              translations: {},
            }
          : undefined
      if (!name) continue
      for (const [lang, text] of [
        [name.original.lang, name.original.text] as const,
        ...(Object.entries(name.translations) as [TextLang, string][]),
      ]) {
        const key = `${lang}\u0000${text}`
        if (seen.has(key)) continue
        seen.add(key)
        names.push(searchName(lang, text))
      }
      if (edition !== undefined && region !== edition) continue
      for (const printing of index.printingsOf(cardId)) {
        if ((++visited & 15) === 0) yield
        if (printing["region"] !== region) continue
        const printedRarity = printingRarity(printing)
        const rarityCode = printedRarity.code
        const rarity =
          typeof rarityCode === "string" ? index.vocabulary("rarity", rarityCode) : undefined
        const rarityName = rarity ? nameOf(index, rarity, "label") : undefined
        const own = index.displayRevision(faceId, region, stringValue(printing["id"]))
        const ownName = own ? nameOf(index, own, "name", revision ?? own) : undefined
        summaries.push({
          cardId,
          ...(rarityName ? { rarity: rarityName } : {}),
          rarityBadge: printedRarity.badge,
          rarityCode:
            typeof rarityCode === "string" ? rarityCode : printedRarity.raw === "-" ? "-" : null,
          premium: printedRarity.premium,
          printingId: stringValue(printing["id"]),
          faceId,
          cardNo: stringValue(printing["card_no"]),
          region,
          route: {
            cardOwner: stringValue(card["home_set_id"]),
            printingOwner: stringValue(printing["home_set_id"]),
          },
          ...(typeof printing["int_id"] === "number" ? { intId: printing["int_id"] } : {}),
          ...(typeof printing["card_no_state"] === "string"
            ? { cardNoState: printing["card_no_state"] }
            : {}),
          classCode: typeof own?.["class_code"] === "string" ? own["class_code"] : null,
          name: ownName ?? name,
          cost: nullableInteger(own?.["cost"]),
          attack: nullableInteger(own?.["attack"]),
          defense: nullableInteger(own?.["defense"]),
          typeCode: typeof own?.["type_code"] === "string" ? own["type_code"] : null,
          deckLocations: deckLocationBadges(
            typeof own?.["type_code"] === "string" ? own["type_code"] : "",
            arrayValue(own?.["special_kinds"] ?? []),
          ),
          wordingPending:
            pending !== undefined && (pending["display"] as Row)["basis"] !== "current",
        })
      }
    }
    const printings: SearchPrinting[] = []
    for (const printing of index.printingsOf(cardId)) {
      if ((++visited & 15) === 0) yield
      const region = stringValue(printing["region"])
      if (!isRegion(region) || (edition !== undefined && region !== edition)) continue
      const cardNo = stringValue(printing["card_no"])
      printings.push({
        id: stringValue(printing["id"]),
        region,
        cardNo,
        lookupKey: cardNoLookupKey(cardNo, sets),
        key: cardNoKey(cardNo, sets),
        flat: normalizeText(cardNo),
      })
    }
    const entry: SearchEntry = {
      cardId,
      classCode,
      setId: stringValue(card["home_set_id"]),
      order,
      names,
      aliases: aliases.get(cardId) ?? [],
      printings,
      defaultPrinting,
    }
    if (edition !== undefined) {
      if (printings.length === 0) continue
      const preferred = defaultPrinting[edition] ?? printings[0]?.id
      if (preferred !== undefined) defaultPrinting[edition] = preferred
      const representative = summaries.find((summary) => summary.printingId === preferred)
      yield {
        entry: { ...entry, classCode: representative ? representative.classCode : entry.classCode },
        summaries,
      }
    } else yield { entry, summaries }
  }
  yield
}

function* catalogCursor(snapshot: LoadedSnapshot): Generator<void, Catalog> {
  let visited = 0
  const index = yield* cardIndexCursor(snapshot)
  const sets = new Set<string>()
  for (const family of index.families) {
    sets.add(stringValue(family["code"]))
    if ((++visited & 31) === 0) yield
  }
  const classCodes: string[] = []
  for (const row of yield* mergedRowsCursor(snapshot.bootstrap, "vocabulary")) {
    if (row["kind"] === "class" && row["active"] === true) classCodes.push(stringValue(row["code"]))
    if ((++visited & 31) === 0) yield
  }
  const aliasRows = yield* mergedRowsCursor(snapshot.bootstrap, "search_alias")
  const entries: SearchEntry[] = []
  const summaries = new Map<string, CardSummary>()
  for (const card of catalogCardCursor(index, index.cards, aliasRows, sets)) {
    if (card === undefined) {
      yield
      continue
    }
    if ((++visited & 31) === 0) yield
    entries.push(card.entry)
    for (const summary of card.summaries) {
      summaries.set(summary.printingId, summary)
      if ((++visited & 31) === 0) yield
    }
  }
  const classLabel = (code: string, lang: TextLang): string => {
    const row = index.vocabulary("class", code)
    if (!row) return code
    for (const entry of row["translations"] as Row[]) {
      if (entry["target_lang"] !== lang) continue
      const text = translationText(index, stringValue(entry["translation_id"]))
      if (text !== undefined) return text
    }
    const unit = index.textUnit(stringValue(row["label_unit_id"]))
    return unit ? stringValue(unit["text"]) : code
  }
  const errataCards = new Map(
    ["jp", "en"].map((region) => [
      region,
      new Set(
        arrayValue(objectValue(snapshot.manifest["errata_card_ids"])[region] ?? []).map((id) =>
          stringValue(id),
        ),
      ),
    ]),
  )
  const search = snapshot.config["search"]
  const normalizerMatches =
    typeof search === "object" &&
    search !== null &&
    !Array.isArray(search) &&
    search["normalizer_version"] === NORMALIZER_VERSION
  yield
  return {
    index,
    entries,
    sets,
    classCodes,
    normalizerMatches,
    suggest: (text, edition, limit) => suggest(text, entries, { edition, sets, limit }),
    results: (state, edition) =>
      apply(
        state,
        state.errataOnly
          ? entries.filter((entry) => errataCards.get(edition)?.has(entry.cardId) === true)
          : entries,
        { edition, sets },
      ),
    summary: (printingId) => summaries.get(printingId),
    classLabel,
  }
}

const catalogs = new WeakMap<LoadedSnapshot, Catalog>()
const prepared = new WeakSet<LoadedSnapshot>()

export function createCatalog(snapshot: LoadedSnapshot): Catalog {
  return drain(catalogCursor(snapshot))
}

/** One catalog per loaded snapshot object; a reload that swaps the snapshot rebuilds it. */
export function catalogOf(snapshot: LoadedSnapshot): Catalog {
  let catalog = catalogs.get(snapshot)
  if (!catalog) {
    catalog = catalogWork("catalogOf", () => createCatalog(snapshot))
    catalogs.set(snapshot, catalog)
  }
  return catalog
}

/** Digest-checked local records reuse their prior validation; construction still stays private. */
export async function prepareCatalog(
  snapshot: LoadedSnapshot,
  check: () => void,
  validate = true,
): Promise<void> {
  check()
  if (prepared.has(snapshot)) return
  const budget = new CatalogBudget(check)
  try {
    if (validate) await budget.drain(validateCatalogCursor(snapshot.bootstrap), "validate")
    const catalog =
      catalogs.get(snapshot) ?? (await budget.drain(catalogCursor(snapshot), "catalogOf"))
    check()
    catalogs.set(snapshot, catalog)
    prepared.add(snapshot)
  } finally {
    budget.close()
  }
}
