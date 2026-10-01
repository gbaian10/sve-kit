import { cardNoKey, cardNoLookupKey } from "../domain/cardNo"
import type { NameSource } from "../domain/nameDisplay"
import { NORMALIZER_VERSION, normalizeText } from "../domain/normalize"
import type { QueryState } from "../domain/query/model"
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
      const name = revision
        ? nameOf(index, revision)
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
          cost: nullableInteger(revision?.["cost"]),
          attack: nullableInteger(revision?.["attack"]),
          defense: nullableInteger(revision?.["defense"]),
        })
      }
    }
    const printings: SearchPrinting[] = index.printingsOf(cardId).flatMap((printing) => {
      const region = stringValue(printing["region"])
      if (!isRegion(region)) return []
      const cardNo = stringValue(printing["card_no"])
      return [
        {
          id: stringValue(printing["id"]),
          region,
          cardNo,
          lookupKey: cardNoLookupKey(cardNo, sets),
          key: cardNoKey(cardNo, sets),
          flat: normalizeText(cardNo),
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
    })
  })
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
  const search = snapshot.config["search"]
  const normalizerMatches =
    typeof search === "object" &&
    search !== null &&
    !Array.isArray(search) &&
    search["normalizer_version"] === NORMALIZER_VERSION
  return {
    index,
    entries,
    sets,
    classCodes,
    normalizerMatches,
    suggest: (text, edition, limit) => suggest(text, entries, { edition, sets, limit }),
    results: (state, edition) => apply(state, entries, { edition, sets }),
    summary: (printingId) => summaries.get(printingId),
    classLabel,
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
