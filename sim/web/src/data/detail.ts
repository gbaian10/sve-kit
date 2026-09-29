import type { CardTextVocabulary, KeywordName, SymbolSpelling } from "../domain/cardText"
import type { RouteNamespace } from "../domain/route"
import type { TextLang } from "../domain/search"
import type { LoadedSnapshot, SnapshotClient } from "./client"
import type { Row } from "./format-v1/decode"
import { integerValue, type JsonValue, stringValue } from "./format-v1/json"
import { bucketOf, createLocator, GLOBAL_OWNER, homeSetOwner } from "./locator"
import type { CardIndex } from "./store"

const LANGS: readonly TextLang[] = ["ja", "en", "zh-Hant"]
const isTextLang = (value: unknown): value is TextLang =>
  typeof value === "string" && (LANGS as readonly string[]).includes(value)

/**
 * Global detail rows, fetched by primary key: each id hashes to one bucket (transport §5), so a
 * card needs only the files its own texts and translations live in, never the whole library.
 */
export interface GlobalDetail {
  readonly textUnit: (id: string) => Promise<Row | undefined>
  readonly translation: (id: string) => Promise<Row | undefined>
  readonly alias: (
    namespace: RouteNamespace,
    key: string,
  ) => Promise<{ readonly namespace: RouteNamespace; readonly key: string } | undefined>
  readonly override: (routeKey: string) => Promise<string | undefined>
  /** Every text icon (a small table, but it may span buckets); keyword names come from the bootstrap. */
  readonly vocabulary: () => Promise<CardTextVocabulary>
  readonly symbolLocalization: (symbolId: string, lang: TextLang) => Promise<Row | undefined>
}

/** One home set's detail file: the full current revisions (effect unit, sections, translations). */
export interface SetDetail {
  readonly revision: (id: string) => Row | undefined
}

function keywordNames(index: CardIndex): KeywordName[] {
  const out: KeywordName[] = []
  for (const row of index.keywords) {
    const keywordId = stringValue(row["id"])
    const own = index.textUnit(stringValue(row["name_unit_id"]))
    if (own && isTextLang(own["lang"]))
      out.push({ keywordId, lang: own["lang"], name: stringValue(own["text"]) })
    for (const entry of row["translations"] as Row[]) {
      if (entry["field"] !== "name") continue
      const translation = index.translation(stringValue(entry["translation_id"]))
      const unit = translation
        ? index.textUnit(stringValue(translation["text_unit_id"]))
        : undefined
      if (unit && isTextLang(unit["lang"]))
        out.push({ keywordId, lang: unit["lang"], name: stringValue(unit["text"]) })
    }
  }
  return out
}

function spellingsOf(symbols: readonly Row[]): SymbolSpelling[] {
  const out: SymbolSpelling[] = []
  for (const symbol of symbols) {
    const parameters = (symbol["parameter_schema"] as Row)["parameters"] as Row[]
    for (const spelling of symbol["spellings"] as Row[]) {
      if (!isTextLang(spelling["lang"])) continue
      const parse = stringValue(spelling["parse_kind"])
      const parameter = parameters.find((item) => item["name"] === spelling["parameter_name"])
      const uint = parameter?.["uint"] as Row | undefined
      out.push({
        symbolId: stringValue(symbol["id"]),
        code: stringValue(symbol["code"]),
        lang: spelling["lang"],
        prefix: stringValue(spelling["literal_prefix"]),
        suffix: stringValue(spelling["literal_suffix"]),
        parse: parse === "uint" ? "uint" : parse === "variable" ? "variable" : "literal",
        variables: (parameter?.["variables"] as string[] | undefined) ?? [],
        ...(uint === undefined
          ? {}
          : { minimum: integerValue(uint["minimum"]), maximum: integerValue(uint["maximum"]) }),
      })
    }
  }
  return out
}

function createGlobal(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  index: CardIndex,
): GlobalDetail {
  const locate = createLocator(snapshot.files)
  const bucketCount = integerValue((snapshot.manifest["partitioning"] as Row)["bucket_count"])
  const rowsOf = async (table: string, primaryKey: readonly JsonValue[]): Promise<Row[]> => {
    const key = locate({
      table,
      owner: GLOBAL_OWNER,
      bucket: bucketOf(primaryKey, bucketCount),
      partition: "detail",
    })
    if (key === undefined) return []
    return (await client.fragments(key))
      .filter((fragment) => fragment.table === table)
      .flatMap((fragment) => fragment.rows)
  }
  const byId = async (table: string, id: string): Promise<Row | undefined> =>
    (await rowsOf(table, [id])).find((row) => row["id"] === id)
  // The icon table is read whole; every bucket that has a fragment of it is one file.
  let symbols: Promise<Row[]> | undefined
  const allSymbols = (): Promise<Row[]> => {
    symbols ??= (async () => {
      const keys = new Set<string>()
      for (let bucket = 0; bucket < bucketCount; bucket += 1) {
        const key = locate({
          table: "text_symbol",
          owner: GLOBAL_OWNER,
          bucket,
          partition: "detail",
        })
        if (key !== undefined) keys.add(key)
      }
      const fragments = (await Promise.all([...keys].map((key) => client.fragments(key)))).flat()
      return fragments.filter((fragment) => fragment.table === "text_symbol").flatMap((f) => f.rows)
    })()
    symbols.catch(() => {
      symbols = undefined
    })
    return symbols
  }
  return {
    textUnit: async (id) => index.textUnit(id) ?? (await byId("text_unit", id)),
    translation: async (id) => index.translation(id) ?? (await byId("translation", id)),
    alias: async (namespace, key) => {
      const row = (await rowsOf("card_route_alias", [namespace, key])).find(
        (item) => item["namespace"] === namespace && item["old_key"] === key,
      )
      if (!row) return undefined
      const target = stringValue(row["target_namespace"])
      return {
        namespace: target === "provisional" ? "provisional" : "official",
        key: stringValue(row["target_key"]),
      }
    },
    override: async (routeKey) => {
      const row = (await rowsOf("route_override", [routeKey])).find(
        (item) => item["route_key"] === routeKey,
      )
      return row ? stringValue(row["printing_id"]) : undefined
    },
    vocabulary: async () => ({
      spellings: spellingsOf(await allSymbols()),
      keywords: keywordNames(index),
    }),
    symbolLocalization: async (symbolId, lang) =>
      (
        (await allSymbols()).find((row) => row["id"] === symbolId)?.["localizations"] as
          Row[] | undefined
      )?.find((row) => row["lang"] === lang),
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
  const revisions = new Map(
    fragments
      .filter((fragment) => fragment.table === "face_revision")
      .flatMap((f) => f.rows)
      .map((row) => [stringValue(row["id"]), row]),
  )
  return { revision: (id) => revisions.get(id) }
}

const globals = new WeakMap<LoadedSnapshot, GlobalDetail>()
const sets = new WeakMap<LoadedSnapshot, Map<string, Promise<SetDetail>>>()

/** One accessor per snapshot object; nothing downloads until a lookup asks for it. */
export function globalDetailOf(client: SnapshotClient, index: CardIndex): GlobalDetail {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  let global = globals.get(snapshot)
  if (!global) {
    global = createGlobal(client, snapshot, index)
    globals.set(snapshot, global)
  }
  return global
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
