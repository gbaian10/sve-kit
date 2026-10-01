import type { CardTextVocabulary } from "../domain/cardText"
import type { NameSource } from "../domain/nameDisplay"
import type { RouteLookups } from "../domain/route"
import type { Region, TextLang } from "../domain/search"
import {
  type ResolvedFaceText,
  resolveFaceText,
  type TranslationCandidate,
  type TranslationOrigin,
} from "../domain/textLanguage"
import type { UiLanguage } from "../i18n/languages"
import type { Catalog } from "./catalog"
import type { SnapshotClient } from "./client"
import { type GlobalDetail, globalDetailOf, setDetailOf } from "./detail"
import type { Row } from "./format-v1/decode"
import { integerValue, type JsonValue, stringValue } from "./format-v1/json"

export interface CardFaceView {
  readonly faceId: string
  readonly side: "front" | "back"
  readonly name: NameSource
  /** Null when the effect text is empty (vanilla follower). */
  readonly effect: ResolvedFaceText | null
  readonly classCode: string | null
  readonly typeCode: string
  readonly traits: readonly string[]
  readonly cost: number | null
  readonly attack: number | null
  readonly defense: number | null
}

export interface KeywordInfo {
  readonly id: string
  readonly name: (lang: TextLang) => string
  /** The rule definition (source language only), fetched when first asked for. */
  readonly definition: () => Promise<string | undefined>
}

export interface CardView {
  readonly cardId: string
  readonly printingId: string
  readonly cardNo: string
  readonly region: Region
  readonly setId: string
  readonly setCode: string
  readonly faces: readonly CardFaceView[]
  /** The default printing of each region the card is released in. */
  readonly editions: Partial<Record<Region, string>>
  /** Mapping state of the shown region towards the other one (snapshot-format §8). */
  readonly mappingState: string
  readonly vocabulary: CardTextVocabulary
  /** Name/tooltip of a text icon in one language; the table is in memory once the view exists. */
  readonly symbolLocalization: (symbolId: string, lang: TextLang) => Row | undefined
  readonly keyword: (id: string) => KeywordInfo | undefined
  readonly vocabularyLabel: Catalog["vocabularyLabel"]
}

const ORIGINS: readonly TranslationOrigin[] = [
  "official_sve",
  "official_svwb",
  "official_sv1",
  "project",
  "machine",
  "community",
]
const isOrigin = (value: JsonValue | undefined): value is TranslationOrigin =>
  typeof value === "string" && (ORIGINS as readonly string[]).includes(value)
const isLang = (value: JsonValue | undefined): value is TextLang =>
  value === "ja" || value === "en" || value === "zh-Hant"

function nullableInteger(value: JsonValue | undefined): number | null {
  return value === null || value === undefined ? null : integerValue(value)
}

/** Effect translations of one revision as candidates for the language matrix. */
async function effectTranslations(
  revision: Row,
  global: GlobalDetail,
): Promise<TranslationCandidate[]> {
  const entries = (revision["translations"] as Row[]).filter((entry) => entry["field"] === "effect")
  const candidates = await Promise.all(
    entries.map(async (entry): Promise<TranslationCandidate | undefined> => {
      const translation = await global.translation(stringValue(entry["translation_id"]))
      if (!translation) return undefined
      const unit = await global.textUnit(stringValue(translation["text_unit_id"]))
      const origin = translation["origin"]
      const status = translation["status"]
      const basis = entry["basis"]
      if (!unit || !isLang(unit["lang"]) || !isOrigin(origin)) return undefined
      return {
        lang: unit["lang"],
        text: stringValue(unit["text"]),
        origin,
        status: status === "draft" ? "draft" : status === "stale" ? "stale" : "reviewed",
        basis:
          basis === "official_counterpart"
            ? "official_counterpart"
            : basis === "shared_jp"
              ? "shared_jp"
              : "own_source",
      }
    }),
  )
  return candidates.filter((item): item is TranslationCandidate => item !== undefined)
}

/**
 * Everything the card page's first screen needs for one printing. Only the home set's revision
 * file, the buckets holding this card's texts and translations, and the icon table are fetched.
 */
export async function loadCardView(
  client: SnapshotClient,
  catalog: Catalog,
  printingId: string,
  uiLanguage: UiLanguage,
): Promise<CardView | undefined> {
  const { index } = catalog
  const printing = index.printing(printingId)
  if (!printing) return undefined
  const cardId = stringValue(printing["card_id"])
  const card = index.card(cardId)
  if (!card) return undefined
  const region = stringValue(printing["region"]) === "en" ? "en" : "jp"
  const setId = stringValue(card["home_set_id"])
  const global = globalDetailOf(client, index)
  const [set, vocabulary] = await Promise.all([setDetailOf(client, setId), global.vocabulary()])
  const faces: CardFaceView[] = []
  for (const face of index.facesOf(cardId)) {
    const faceId = stringValue(face["id"])
    const base = index.currentRevision(faceId, region)
    const name = catalog.faceName(faceId, region)
    if (!base || !name) continue
    const revision = set.revision(stringValue(base["id"])) ?? base
    const unitId = revision["effect_unit_id"]
    const unit = typeof unitId === "string" ? await global.textUnit(unitId) : undefined
    const text = unit ? stringValue(unit["text"]) : ""
    const original =
      unit && isLang(unit["lang"]) && text !== "" ? { lang: unit["lang"], text } : undefined
    const classCode = revision["class_code"]
    faces.push({
      faceId,
      side: face["side"] === "back" ? "back" : "front",
      name,
      effect: original
        ? resolveFaceText({
            edition: region,
            uiLanguage,
            original,
            fallback: undefined,
            translations: await effectTranslations(revision, global),
          })
        : null,
      classCode: typeof classCode === "string" ? classCode : null,
      typeCode: stringValue(revision["type_code"]),
      traits: (revision["traits"] as string[] | undefined) ?? [],
      cost: nullableInteger(revision["cost"]),
      attack: nullableInteger(revision["attack"]),
      defense: nullableInteger(revision["defense"]),
    })
  }
  const editions: Partial<Record<Region, string>> = {}
  let mappingState = "unmapped"
  for (const view of card["regions"] as Row[]) {
    const viewRegion = view["region"]
    const defaultPrinting = view["default_printing_id"]
    if ((viewRegion === "jp" || viewRegion === "en") && typeof defaultPrinting === "string")
      editions[viewRegion] = defaultPrinting
    if (viewRegion === region) mappingState = stringValue(view["mapping_state"])
  }
  const family = index.family(setId)
  // Localizations were fetched with the vocabulary; read them back synchronously for rendering.
  const localizations = new Map<string, Row | undefined>()
  await Promise.all(
    vocabulary.spellings.flatMap((spelling) =>
      (["ja", "en", "zh-Hant"] as const).map(async (lang) => {
        const key = `${spelling.symbolId}\u0000${lang}`
        if (!localizations.has(key))
          localizations.set(key, await global.symbolLocalization(spelling.symbolId, lang))
      }),
    ),
  )
  const keywordInfo = (id: string): KeywordInfo | undefined => {
    const row = index.keyword(id)
    if (!row) return undefined
    const names = vocabulary.keywords.filter((item) => item.keywordId === id)
    const definitionId = row["definition_unit_id"]
    return {
      id,
      name: (lang) => (names.find((item) => item.lang === lang) ?? names[0])?.name ?? id,
      definition: async () => {
        if (typeof definitionId !== "string") return undefined
        const unit = await global.textUnit(definitionId)
        return unit ? stringValue(unit["text"]) : undefined
      },
    }
  }
  return {
    cardId,
    printingId,
    cardNo: stringValue(printing["card_no"]),
    region,
    setId,
    setCode: family ? stringValue(family["public_code"]) : setId,
    faces,
    editions,
    mappingState,
    vocabulary,
    symbolLocalization: (symbolId, lang) => localizations.get(`${symbolId}\u0000${lang}`),
    keyword: keywordInfo,
    vocabularyLabel: catalog.vocabularyLabel,
  }
}

/**
 * What the route resolver needs for one URL: the bootstrap index plus the alias and override rows
 * of that key (each one bucket), fetched before resolving.
 */
export async function createRouteLookups(
  catalog: Catalog,
  global: GlobalDetail,
  key: { readonly namespace: "official" | "provisional"; readonly value: string },
): Promise<RouteLookups> {
  const [alias, override] = await Promise.all([
    global.alias(key.namespace, key.value),
    key.namespace === "official" ? global.override(key.value) : Promise.resolve(undefined),
  ])
  return {
    printingByCardNo: (cardNo) => {
      const row = catalog.index.printingByAnyCardNo(cardNo)
      return row ? stringValue(row["id"]) : undefined
    },
    printingByIntId: (intId) => {
      const row = catalog.index.printingByIntId(intId)
      return row ? stringValue(row["id"]) : undefined
    },
    alias: (namespace, value) =>
      namespace === key.namespace && value === key.value ? alias : undefined,
    override: (routeKey) => (routeKey === key.value ? override : undefined),
    nameOf: (printingId) => catalog.summary(printingId)?.name.original.text,
  }
}
