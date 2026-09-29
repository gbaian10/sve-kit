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
  readonly definition: (lang: TextLang) => string | undefined
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
  readonly symbolLocalization: GlobalDetail["symbolLocalization"]
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
function effectTranslations(revision: Row, global: GlobalDetail): TranslationCandidate[] {
  const out: TranslationCandidate[] = []
  for (const entry of revision["translations"] as Row[]) {
    if (entry["field"] !== "effect") continue
    const translation = global.translation(stringValue(entry["translation_id"]))
    if (!translation) continue
    const unit = global.textUnit(stringValue(translation["text_unit_id"]))
    const origin = translation["origin"]
    const status = translation["status"]
    const basis = entry["basis"]
    if (!unit || !isLang(unit["lang"]) || !isOrigin(origin)) continue
    out.push({
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
    })
  }
  return out
}

/**
 * Everything the card page's first screen needs for one printing: the global detail (texts,
 * icons, routes) and the home set's detail (full revisions) are fetched once each per snapshot.
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
  const [global, set] = await Promise.all([
    globalDetailOf(client, index),
    setDetailOf(client, setId),
  ])
  const faces: CardFaceView[] = []
  for (const face of index.facesOf(cardId)) {
    const faceId = stringValue(face["id"])
    const base = index.currentRevision(faceId, region)
    const name = catalog.faceName(faceId, region)
    if (!base || !name) continue
    const revision = set.revision(stringValue(base["id"])) ?? base
    const unit = global.textUnit(stringValue(revision["effect_unit_id"] ?? ""))
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
            translations: effectTranslations(revision, global),
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
  const keywordInfo = (id: string): KeywordInfo | undefined => {
    const row = index.keyword(id)
    if (!row) return undefined
    const names = global.vocabulary.keywords.filter((item) => item.keywordId === id)
    const definitionId = row["definition_unit_id"]
    const definition = typeof definitionId === "string" ? global.textUnit(definitionId) : undefined
    return {
      id,
      name: (lang) => (names.find((item) => item.lang === lang) ?? names[0])?.name ?? id,
      // Definitions ship in the source language only; the UI shows it with a lang attribute.
      definition: () => (definition ? stringValue(definition["text"]) : undefined),
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
    vocabulary: global.vocabulary,
    symbolLocalization: global.symbolLocalization,
    keyword: keywordInfo,
    vocabularyLabel: catalog.vocabularyLabel,
  }
}

/** What the route resolver needs, from the bootstrap index and the global route tables. */
export function createRouteLookups(catalog: Catalog, global: GlobalDetail): RouteLookups {
  return {
    printingByCardNo: (cardNo) => {
      const row = catalog.index.printingByAnyCardNo(cardNo)
      return row ? stringValue(row["id"]) : undefined
    },
    printingByIntId: (intId) => {
      const row = catalog.index.printingByIntId(intId)
      return row ? stringValue(row["id"]) : undefined
    },
    alias: global.alias,
    override: global.override,
    nameOf: (printingId) => catalog.summary(printingId)?.name.original.text,
  }
}
