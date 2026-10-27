import type { UiLanguage } from "../i18n/languages"
import type { LoadedSnapshot, SnapshotClient } from "./client"
import type { Row } from "./format-v3/decode"
import { arrayValue, type JsonObject, objectValue, stringValue } from "./format-v3/json"

type DigitalGame = "sv1" | "svwb"
type DigitalRelation = "same_card" | "same_character" | "name_only" | "same_name"

export interface DigitalLink {
  readonly game: DigitalGame
  readonly officialId: string
  readonly relation: DigitalRelation
}

// Weakest first: when rows for one official ID disagree, the weakest claim is the one we can stand behind.
const RELATIONS: readonly DigitalRelation[] = [
  "name_only",
  "same_name",
  "same_character",
  "same_card",
]
const GAMES: readonly DigitalGame[] = ["sv1", "svwb"]

// The contract names languages by script (config.languages); the UI locale code differs for Chinese.
const CONTRACT_LANGUAGE: Readonly<Record<UiLanguage, string>> = {
  "zh-TW": "zh-Hant",
  ja: "ja",
  en: "en",
}

const isGame = (value: unknown): value is DigitalGame => GAMES.includes(value as DigitalGame)
const isRelation = (value: unknown): value is DigitalRelation =>
  RELATIONS.includes(value as DigitalRelation)

const byOfficialId = (a: string, b: string) => a.length - b.length || (a < b ? -1 : a > b ? 1 : 0)

// A card-level row says nothing about a specific face or digital phase.
function selectDigitalLinks(
  cardId: string,
  links: readonly Row[],
  digitalCards: ReadonlyMap<string, { readonly game: DigitalGame; readonly officialId: string }>,
): DigitalLink[] {
  const merged = new Map<string, DigitalLink>()
  for (const row of links) {
    if (row["card_id"] !== cardId || row["face_id"] !== null || row["digital_phase"] !== null)
      continue
    const relation = row["relation"]
    if (!isRelation(relation)) continue
    const target = digitalCards.get(stringValue(row["digital_card_id"]))
    if (!target) throw new Error("digital link references a missing digital card")
    const key = `${target.game}:${target.officialId}`
    const present = merged.get(key)
    if (!present || RELATIONS.indexOf(relation) < RELATIONS.indexOf(present.relation))
      merged.set(key, { ...target, relation })
  }
  return [...merged.values()].sort(
    (a, b) =>
      GAMES.indexOf(a.game) - GAMES.indexOf(b.game) || byOfficialId(a.officialId, b.officialId),
  )
}

const keysOf = (snapshot: LoadedSnapshot, table: string, homeSetId?: string): string[] =>
  [...snapshot.files]
    .filter(
      ([, file]) =>
        file["purpose"] === "engine" &&
        arrayValue(file["row_counts"]).some((raw) => {
          const count = arrayValue(raw)
          return count[0] === table && (homeSetId === undefined || count[2] === homeSetId)
        }),
    )
    .map(([key]) => key)

async function rowsOf(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  table: string,
  homeSetId?: string,
): Promise<Row[]> {
  const parts = (
    await Promise.all(keysOf(snapshot, table, homeSetId).map((key) => client.fragments(key)))
  ).flat()
  if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
    throw new Error("snapshot replaced")
  return parts.filter((part) => part.table === table).flatMap((part) => part.rows)
}

// The digital card table is only a few KB, so linked cards can read it whole.
export async function loadCardDigitalLinks(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  cardId: string,
  homeSetId: string,
): Promise<DigitalLink[]> {
  const links = (await rowsOf(client, snapshot, "digital_link", homeSetId)).filter(
    (row) => row["card_id"] === cardId,
  )
  if (!links.length) return []
  const digitalCards = new Map(
    (await rowsOf(client, snapshot, "digital_card")).flatMap((row) => {
      const game = row["game"]
      return isGame(game)
        ? [[stringValue(row["id"]), { game, officialId: stringValue(row["official_id"]) }] as const]
        : []
    }),
  )
  return selectDigitalLinks(cardId, links, digitalCards)
}

export function digitalLinkUrl(
  config: JsonObject,
  link: Pick<DigitalLink, "game" | "officialId">,
  uiLanguage: UiLanguage,
): string | undefined {
  const endpoint = arrayValue(config["digital_endpoints"] ?? [])
    .map((value) => objectValue(value))
    .find((entry) => entry["game"] === link.game)
  const template = endpoint?.["card_url_template"]
  const providerLanguage = objectValue(endpoint?.["language_map"] ?? {})[
    CONTRACT_LANGUAGE[uiLanguage]
  ]
  if (typeof template !== "string" || typeof providerLanguage !== "string") return undefined
  const url = template
    .replaceAll("{official_id}", () => encodeURIComponent(link.officialId))
    .replaceAll("{provider_lang}", () => encodeURIComponent(providerLanguage))
  return URL.canParse(url) && new URL(url).protocol === "https:" ? url : undefined
}
