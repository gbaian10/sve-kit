import { cardNoKey, cardNoLookupKey } from "../domain/cardNo"
import { type MechanicFacts, triState } from "../domain/mechanics"
import type { NameSource } from "../domain/nameDisplay"
import { NORMALIZER_VERSION, normalizeText } from "../domain/normalize"
import type { QueryState } from "../domain/query/model"
import {
  apply,
  type FaceStats,
  type Region,
  type ResultItem,
  type SearchEntry,
  type SearchName,
  type SearchPrinting,
  suggest,
  type Suggestion,
  type TextLang,
} from "../domain/search"
import type { LoadedSnapshot } from "./client"
import type { Row } from "./format-v1/decode"
import { integerValue, type JsonValue, stringValue } from "./format-v1/json"
import { type CardIndex, createCardIndex } from "./store"

/** What a list cell or suggestion row shows for one card without loading any detail file. */
export interface CardSummary {
  readonly cardId: string
  readonly printingId: string
  readonly faceId: string
  readonly cardNo: string
  readonly region: Region
  readonly classCode: string | null
  readonly name: NameSource
  readonly cost: number | null
  readonly attack: number | null
  readonly defense: number | null
}

export interface FilterOptions {
  readonly types: readonly string[]
  readonly rarities: readonly string[]
  readonly sets: readonly { readonly code: string; readonly id: string }[]
  readonly keywords: readonly { readonly id: string; readonly name: (lang: TextLang) => string }[]
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
  /** Vocabulary codes and keyword ids the filter sheet offers. */
  readonly filterOptions: () => FilterOptions
  /** Cards with any mechanic annotation for a region, for the completeness hint. */
  readonly mechanicCoverageSummary: (region: Region) => {
    readonly annotated: number
    readonly total: number
  }
  /** The class label for the UI language (vocabulary translation), else the Japanese label. */
  readonly classLabel: (code: string, lang: TextLang) => string
  /** Any vocabulary label (`type`, `trait`, `rarity`…) for the language, else the Japanese label. */
  readonly vocabularyLabel: (kind: string, code: string, lang: TextLang) => string
  /** Name of one face in one region from the bootstrap revision (any face, not only the front). */
  readonly faceName: (faceId: string, region: Region) => NameSource | undefined
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

function nameOf(index: CardIndex, revision: Row): NameSource | undefined {
  const unit = index.textUnit(stringValue(revision["name_unit_id"]))
  if (!unit) return undefined
  const lang = stringValue(unit["lang"])
  if (!isTextLang(lang)) return undefined
  const translations: Partial<Record<TextLang, string>> = {}
  for (const entry of revision["translations"] as Row[]) {
    if (entry["field"] !== "name") continue
    const target = stringValue(entry["target_lang"])
    const text = translationText(index, stringValue(entry["translation_id"]))
    if (isTextLang(target) && text !== undefined) translations[target] = text
  }
  return { original: { lang, text: stringValue(unit["text"]) }, translations }
}

function searchName(lang: TextLang, text: string): SearchName {
  return { lang, text, normalized: normalizeText(text) }
}

function nullableInteger(value: JsonValue | undefined): number | null {
  return value === null || value === undefined ? null : integerValue(value)
}

export function createCatalog(snapshot: LoadedSnapshot): Catalog {
  const index = createCardIndex(snapshot)
  const sets = new Set(index.families.map((family) => stringValue(family["code"])))
  const rows = (table: string): Row[] =>
    snapshot.bootstrap
      .filter((fragment) => fragment.table === table)
      .flatMap((fragment) => fragment.rows)
  const classCodes = rows("vocabulary")
    .filter((row) => row["kind"] === "class" && row["active"] === true)
    .map((row) => stringValue(row["code"]))
  // Aliases of kind `card` name the card by its id without the `c:` prefix (Code has no colon).
  const aliases = new Map<string, SearchName[]>()
  for (const row of rows("search_alias")) {
    if (row["kind"] !== "card") continue
    const lang = stringValue(row["lang"])
    if (!isTextLang(lang)) continue
    const cardId = `c:${stringValue(row["code"])}`
    aliases.set(cardId, [
      ...(aliases.get(cardId) ?? []),
      searchName(lang, stringValue(row["text"])),
    ])
  }
  // Facet lookups: rarity/variant/art per printing, release dates, mechanic facts per card.
  const productDate = new Map(
    rows("product").map((row) => [stringValue(row["id"]), row["released_on"]] as const),
  )
  const releasedOn = new Map<string, string>()
  for (const link of index.printingProducts) {
    const printingId = stringValue(link["printing_id"])
    const date = link["available_on"] ?? productDate.get(stringValue(link["product_id"]))
    if (typeof date !== "string") continue
    const known = releasedOn.get(printingId)
    if (known === undefined || date < known) releasedOn.set(printingId, date)
  }
  const universe = new Set(index.keywords.map((row) => stringValue(row["id"])))
  const projections = new Map<string, Set<string>>()
  for (const row of index.mechanicProjections) {
    const key = `${stringValue(row["card_id"])}\u0000${stringValue(row["scope"])}`
    projections.set(key, (projections.get(key) ?? new Set()).add(stringValue(row["keyword_id"])))
  }
  const coverage = new Map(
    index.mechanicCoverage.map((row) => [
      `${stringValue(row["card_id"])}\u0000${stringValue(row["scope"])}`,
      row,
    ]),
  )
  const factsOf = (cardId: string, scope: "shared" | "en_override"): MechanicFacts => {
    const row = coverage.get(`${cardId}\u0000${scope}`)
    const support = index.support(cardId)
    const blocks = (support?.["region_blocks"] as Row[] | undefined) ?? []
    return {
      present: projections.get(`${cardId}\u0000${scope}`) ?? new Set(),
      coverage: row
        ? {
            completeAll: row["complete_all"] === true,
            completeMode: row["complete_mode"] === "exclude" ? "exclude" : "include",
            completeIds: (row["complete_keyword_ids"] as string[] | undefined) ?? [],
            partialMode: row["partial_mode"] === "exclude" ? "exclude" : "include",
            partialIds: (row["partial_keyword_ids"] as string[] | undefined) ?? [],
          }
        : undefined,
      enBlocked: blocks.some((block) => block["region"] === "en"),
    }
  }
  const setCodeOf = (id: string): string => {
    const family = index.family(id)
    return family ? stringValue(family["code"]) : id
  }
  const entries: SearchEntry[] = []
  const summaries = new Map<string, CardSummary>()
  index.cards.forEach((card, order) => {
    const cardId = stringValue(card["id"])
    const front = index.facesOf(cardId)[0]
    if (!front) return
    const faceId = stringValue(front["id"])
    const names: SearchName[] = []
    const seen = new Set<string>()
    const defaultPrinting: Partial<Record<Region, string>> = {}
    const faces: Partial<Record<Region, FaceStats>> = {}
    let classCode: string | null = null
    for (const view of card["regions"] as Row[]) {
      const region = stringValue(view["region"])
      if (!isRegion(region)) continue
      const printingId = view["default_printing_id"]
      if (typeof printingId === "string") defaultPrinting[region] = printingId
      const revision = index.currentRevision(faceId, region)
      if (!revision) continue
      const code = revision["class_code"]
      if (typeof code === "string") classCode = code
      const name = nameOf(index, revision)
      if (!name) continue
      faces[region] = {
        typeCode: stringValue(revision["type_code"]),
        cost: nullableInteger(revision["cost"]),
        attack: nullableInteger(revision["attack"]),
        defense: nullableInteger(revision["defense"]),
        name: name.original.text,
      }
      for (const [lang, text] of [
        [name.original.lang, name.original.text] as const,
        ...(Object.entries(name.translations) as [TextLang, string][]),
      ]) {
        const key = `${lang}\u0000${text}`
        if (seen.has(key)) continue
        seen.add(key)
        names.push(searchName(lang, text))
      }
      for (const printing of index.printingsOf(cardId)) {
        if (printing["region"] !== region) continue
        summaries.set(stringValue(printing["id"]), {
          cardId,
          printingId: stringValue(printing["id"]),
          faceId,
          cardNo: stringValue(printing["card_no"]),
          region,
          classCode: typeof code === "string" ? code : null,
          name,
          cost: nullableInteger(revision["cost"]),
          attack: nullableInteger(revision["attack"]),
          defense: nullableInteger(revision["defense"]),
        })
      }
    }
    const printings: SearchPrinting[] = index.printingsOf(cardId).flatMap((printing) => {
      const region = stringValue(printing["region"])
      if (!isRegion(region)) return []
      const cardNo = stringValue(printing["card_no"])
      const printingId = stringValue(printing["id"])
      const front = (printing["faces"] as Row[])[0]
      const rarity = printing["rarity_code"]
      const art = front?.["art_id"]
      return [
        {
          id: printingId,
          region,
          cardNo,
          lookupKey: cardNoLookupKey(cardNo, sets),
          key: cardNoKey(cardNo, sets),
          flat: normalizeText(cardNo),
          rarity: typeof rarity === "string" ? rarity : null,
          variant: stringValue(printing["variant_key"]),
          artId: typeof art === "string" ? art : null,
          releasedOn: releasedOn.get(printingId) ?? null,
        },
      ]
    })
    entries.push({
      cardId,
      classCode,
      setId: stringValue(card["home_set_id"]),
      order,
      names,
      aliases: aliases.get(cardId) ?? [],
      printings,
      defaultPrinting,
      setCode: setCodeOf(stringValue(card["home_set_id"])),
      faces,
      mechanic: (keywordId, region) => {
        // An EN override, when present, is the English card's own annotation.
        const override = region === "en" ? factsOf(cardId, "en_override") : undefined
        if (override && (override.present.size > 0 || override.coverage))
          return triState(override, keywordId, universe, region, "en_override")
        return triState(factsOf(cardId, "shared"), keywordId, universe, region, "shared")
      },
    })
  })
  const vocabularyLabel = (kind: string, code: string, lang: TextLang): string => {
    const row = index.vocabulary(kind, code)
    if (!row) return code
    for (const entry of row["translations"] as Row[]) {
      if (entry["target_lang"] !== lang) continue
      const text = translationText(index, stringValue(entry["translation_id"]))
      if (text !== undefined) return text
    }
    const unit = index.textUnit(stringValue(row["label_unit_id"]))
    return unit ? stringValue(unit["text"]) : code
  }
  const search = snapshot.config["search"]
  const normalizerMatches =
    typeof search === "object" &&
    search !== null &&
    !Array.isArray(search) &&
    search["normalizer_version"] === NORMALIZER_VERSION
  const vocabularyCodes = (kind: string): string[] =>
    rows("vocabulary")
      .filter((row) => row["kind"] === kind && row["active"] === true)
      .map((row) => stringValue(row["code"]))
  const keywordOptions = index.keywords.map((row) => {
    const id = stringValue(row["id"])
    const own = index.textUnit(stringValue(row["name_unit_id"]))
    const names = new Map<string, string>()
    if (own) names.set(stringValue(own["lang"]), stringValue(own["text"]))
    for (const entry of row["translations"] as Row[]) {
      if (entry["field"] !== "name") continue
      const text = translationText(index, stringValue(entry["translation_id"]))
      if (text !== undefined) names.set(stringValue(entry["target_lang"]), text)
    }
    return { id, name: (lang: TextLang) => names.get(lang) ?? names.get("ja") ?? id }
  })
  const filterOptions = (): FilterOptions => ({
    types: vocabularyCodes("type"),
    rarities: vocabularyCodes("rarity"),
    sets: index.families.map((row) => ({
      code: stringValue(row["code"]),
      id: stringValue(row["id"]),
    })),
    keywords: keywordOptions,
  })
  const mechanicCoverageSummary = (region: Region) => {
    const released = entries.filter((entry) => entry.defaultPrinting[region] !== undefined)
    const annotated = released.filter((entry) => {
      const shared = factsOf(entry.cardId, "shared")
      const override = region === "en" ? factsOf(entry.cardId, "en_override") : undefined
      const facts = override && (override.present.size > 0 || override.coverage) ? override : shared
      return facts.present.size > 0 || facts.coverage !== undefined
    })
    return { annotated: annotated.length, total: released.length }
  }
  return {
    index,
    entries,
    sets,
    classCodes,
    normalizerMatches,
    filterOptions,
    mechanicCoverageSummary,
    suggest: (text, edition, limit) => suggest(text, entries, { edition, sets, limit }),
    results: (state, edition) => apply(state, entries, { edition, sets }),
    summary: (printingId) => summaries.get(printingId),
    classLabel: (code, lang) => vocabularyLabel("class", code, lang),
    vocabularyLabel,
    faceName: (faceId, region) => {
      const revision = index.currentRevision(faceId, region)
      return revision ? nameOf(index, revision) : undefined
    },
  }
}

const catalogs = new WeakMap<LoadedSnapshot, Catalog>()

/** One catalog per loaded snapshot object; a reload that swaps the snapshot rebuilds it. */
export function catalogOf(snapshot: LoadedSnapshot): Catalog {
  let catalog = catalogs.get(snapshot)
  if (!catalog) {
    catalog = createCatalog(snapshot)
    catalogs.set(snapshot, catalog)
  }
  return catalog
}
