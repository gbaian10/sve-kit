import type { CardTextVocabulary, KeywordName, SymbolSpelling } from "../domain/cardText"
import type { RouteNamespace } from "../domain/route"
import type { TextLang } from "../domain/search"
import type { LoadedSnapshot, SnapshotClient } from "./client"
import type { Row } from "./format-v1/decode"
import { integerValue, stringValue } from "./format-v1/json"
import { bucketOf, createLocator, GLOBAL_OWNER, homeSetOwner } from "./locator"
import type { CardIndex } from "./store"

const LANGS: readonly TextLang[] = ["ja", "en", "zh-Hant"]
const isTextLang = (value: unknown): value is TextLang =>
  typeof value === "string" && (LANGS as readonly string[]).includes(value)

/** The global detail file: effect texts, their translations, text icons, route tables. */
export interface GlobalDetail {
  readonly textUnit: (id: string) => Row | undefined
  readonly translation: (id: string) => Row | undefined
  readonly vocabulary: CardTextVocabulary
  /** Name/tooltip/copy pattern of one text icon in one language. */
  readonly symbolLocalization: (symbolId: string, lang: TextLang) => Row | undefined
  readonly alias: (
    namespace: RouteNamespace,
    key: string,
  ) => { readonly namespace: RouteNamespace; readonly key: string } | undefined
  readonly override: (routeKey: string) => string | undefined
}

/** One home set's detail file: the full current revisions (effect unit, sections, translations). */
export interface SetDetail {
  readonly revision: (id: string) => Row | undefined
}

function byId(rows: readonly Row[]): Map<string, Row> {
  return new Map(rows.map((row) => [stringValue(row["id"]), row]))
}

function keywordNames(index: CardIndex, textUnit: (id: string) => Row | undefined): KeywordName[] {
  const out: KeywordName[] = []
  for (const row of index.keywords) {
    const keywordId = stringValue(row["id"])
    const own = textUnit(stringValue(row["name_unit_id"]))
    if (own && isTextLang(own["lang"]))
      out.push({ keywordId, lang: own["lang"], name: stringValue(own["text"]) })
    for (const entry of row["translations"] as Row[]) {
      if (entry["field"] !== "name") continue
      const translation = index.translation(stringValue(entry["translation_id"]))
      const unit = translation ? textUnit(stringValue(translation["text_unit_id"])) : undefined
      if (unit && isTextLang(unit["lang"]))
        out.push({ keywordId, lang: unit["lang"], name: stringValue(unit["text"]) })
    }
  }
  return out
}

function spellingsOf(symbols: readonly Row[]): SymbolSpelling[] {
  const out: SymbolSpelling[] = []
  for (const symbol of symbols) {
    const schema = symbol["parameter_schema"] as Row
    const variables = (schema["parameters"] as Row[]).flatMap(
      (parameter) => (parameter["variables"] as string[] | undefined) ?? [],
    )
    for (const spelling of symbol["spellings"] as Row[]) {
      if (!isTextLang(spelling["lang"])) continue
      const parse = stringValue(spelling["parse_kind"])
      out.push({
        symbolId: stringValue(symbol["id"]),
        code: stringValue(symbol["code"]),
        lang: spelling["lang"],
        prefix: stringValue(spelling["literal_prefix"]),
        suffix: stringValue(spelling["literal_suffix"]),
        parse: parse === "uint" ? "uint" : parse === "variable" ? "variable" : "literal",
        variables,
      })
    }
  }
  return out
}

async function loadGlobal(client: SnapshotClient, index: CardIndex): Promise<GlobalDetail> {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  const locate = createLocator(snapshot.files)
  const key = locate({ table: "text_symbol", owner: GLOBAL_OWNER, bucket: 0, partition: "detail" })
  const fragments = key === undefined ? [] : await client.fragments(key)
  const rows = (table: string) =>
    fragments.filter((fragment) => fragment.table === table).flatMap((fragment) => fragment.rows)
  const units = byId(rows("text_unit"))
  const translations = byId(rows("translation"))
  const symbols = rows("text_symbol")
  const symbolMap = byId(symbols)
  const aliases = new Map(
    rows("card_route_alias").map((row) => [
      `${stringValue(row["namespace"])}\u0000${stringValue(row["old_key"])}`,
      row,
    ]),
  )
  const overrides = new Map(
    rows("route_override").map((row) => [
      stringValue(row["route_key"]),
      stringValue(row["printing_id"]),
    ]),
  )
  const textUnit = (id: string) => index.textUnit(id) ?? units.get(id)
  return {
    textUnit,
    translation: (id) => index.translation(id) ?? translations.get(id),
    vocabulary: { spellings: spellingsOf(symbols), keywords: keywordNames(index, textUnit) },
    symbolLocalization: (symbolId, lang) =>
      (symbolMap.get(symbolId)?.["localizations"] as Row[] | undefined)?.find(
        (row) => row["lang"] === lang,
      ),
    alias: (namespace, key) => {
      const row = aliases.get(`${namespace}\u0000${key}`)
      if (!row) return undefined
      const target = stringValue(row["target_namespace"])
      return {
        namespace: target === "provisional" ? "provisional" : "official",
        key: stringValue(row["target_key"]),
      }
    },
    override: (routeKey) => overrides.get(routeKey),
  }
}

async function loadSet(client: SnapshotClient, setId: string): Promise<SetDetail> {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  const locate = createLocator(snapshot.files)
  const bucketCount = integerValue((snapshot.manifest["partitioning"] as Row)["bucket_count"])
  // Revisions of one set share a file per bucket; with one bucket that is one file.
  const keys = new Set<string>()
  for (let bucket = 0; bucket < bucketCount; bucket += 1) {
    const key = locate({
      table: "face_revision",
      owner: homeSetOwner(setId),
      bucket,
      partition: "detail",
    })
    if (key !== undefined) keys.add(key)
  }
  const fragments = (await Promise.all([...keys].map((key) => client.fragments(key)))).flat()
  const revisions = byId(
    fragments.filter((fragment) => fragment.table === "face_revision").flatMap((f) => f.rows),
  )
  return { revision: (id) => revisions.get(id) }
}

const globals = new WeakMap<LoadedSnapshot, Promise<GlobalDetail>>()
const sets = new WeakMap<LoadedSnapshot, Map<string, Promise<SetDetail>>>()

/** Loaded once per snapshot object; a failed load is forgotten so a retry can succeed. */
export function globalDetailOf(client: SnapshotClient, index: CardIndex): Promise<GlobalDetail> {
  const snapshot = client.snapshot()
  if (!snapshot) return Promise.reject(new Error("snapshot not loaded"))
  let pending = globals.get(snapshot)
  if (!pending) {
    pending = loadGlobal(client, index)
    globals.set(snapshot, pending)
    pending.catch(() => globals.delete(snapshot))
  }
  return pending
}

export function setDetailOf(client: SnapshotClient, setId: string): Promise<SetDetail> {
  const snapshot = client.snapshot()
  if (!snapshot) return Promise.reject(new Error("snapshot not loaded"))
  let perSet = sets.get(snapshot)
  if (!perSet) {
    perSet = new Map()
    sets.set(snapshot, perSet)
  }
  let pending = perSet.get(setId)
  if (!pending) {
    pending = loadSet(client, setId)
    perSet.set(setId, pending)
    pending.catch(() => perSet.delete(setId))
  }
  return pending
}

export { bucketOf }
