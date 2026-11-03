import type {
  ConstructionIndex,
  FormatRules,
  IndexCard,
  Restriction,
  Zone,
} from "../domain/deck/construction"
import { fetchBytes, type Fetcher } from "./cdn"
import { fail } from "./format-v3/errors"
import {
  arrayValue,
  integerValue,
  type JsonValue,
  objectValue,
  parseJson,
  stringValue,
} from "./format-v3/json"

const FORMAT = 1
const ZONES: readonly Zone[] = ["none", "leader", "main", "evolve"]
const SCOPES = ["main", "evolve", "all"] as const
const KINDS = ["copy_limit", "choice_group"] as const

/** The construction index next to a snapshot: `construction/<manifest hex>/<region>.json`. */
export function constructionIndexUrl(base: string, manifestHash: string, region: string): string {
  return `${base}/construction/${manifestHash.replace(/^sha256:/u, "")}/${region}.json`
}

export async function loadConstructionIndex(
  fetcher: Fetcher,
  base: string,
  manifestHash: string,
  region: string,
): Promise<ConstructionIndex> {
  const bytes = await fetchBytes(fetcher, constructionIndexUrl(base, manifestHash, region))
  return decodeConstructionIndex(parseJson(bytes), manifestHash, region)
}

const oneOf = <T extends string>(allowed: readonly T[], value: string, path: string): T => {
  if (!(allowed as readonly string[]).includes(value)) fail("shape", `unexpected ${value}`, [path])
  return value as T
}
const dateOrNull = (value: JsonValue | undefined, path: string) =>
  value === null ? null : stringValue(value, [path])
const intOrNull = (value: JsonValue | undefined, path: string) =>
  value === null ? null : integerValue(value, [path])

/** Validates the wire shape (docs/schema/export/construction-index.md) and its snapshot binding. */
export function decodeConstructionIndex(
  value: JsonValue,
  manifestHash: string,
  region: string,
): ConstructionIndex {
  const root = objectValue(value)
  if (root["construction_index_format"] !== FORMAT) fail("shape", "unknown index format", [])
  if (root["region"] !== region) fail("shape", "index of another region", ["region"])
  const snapshot = objectValue(root["snapshot"], ["snapshot"])
  if (snapshot["manifest_sha256"] !== manifestHash)
    fail("shape", "index of another snapshot", ["snapshot"])
  const zoneNames = arrayValue(root["zones"]).map((item) => stringValue(item, ["zones"]))
  if (zoneNames.join() !== ZONES.join()) fail("shape", "unexpected zones", ["zones"])
  const classes = arrayValue(root["classes"]).map((item) => stringValue(item, ["classes"]))
  const titles = arrayValue(root["titles"]).map((item) => stringValue(item, ["titles"]))
  const names = integerValue(root["names"], ["names"])
  const nameCode = (item: JsonValue | undefined, path: string) => {
    const code = integerValue(item, [path])
    if (code < 0 || code >= names) fail("shape", "unknown rules name", [path])
    return code
  }

  const columns = objectValue(root["cards"], ["cards"])
  const column = (key: string) => arrayValue(columns[key], ["cards", key])
  const ids = column("id").map((item) => stringValue(item, ["cards", "id"]))
  const [name, classColumn, titleBits, zones] = ["name", "class", "titles", "zone"].map(column)
  if ([name, classColumn, titleBits, zones].some((values) => values?.length !== ids.length))
    fail("shape", "card columns differ in length", ["cards"])
  type Mutable = { -readonly [K in keyof IndexCard]: IndexCard[K] }
  const cards: Mutable[] = ids.map((_id, i) => {
    const classIndex = integerValue(classColumn?.[i], ["cards", "class"])
    const bits = integerValue(titleBits?.[i], ["cards", "titles"])
    return {
      name: nameCode(name?.[i], "cards.name"),
      classCode: classIndex === -1 ? null : (classes[classIndex] ?? fail("shape", "class", [])),
      titles: titles.filter((_title, bit) => (bits & (1 << bit)) !== 0),
      zone: ZONES[integerValue(zones?.[i], ["cards", "zone"])] ?? fail("shape", "zone", []),
      startAmulet: false,
    }
  })
  const at = (item: JsonValue | undefined, path: string): Mutable =>
    cards[integerValue(item, [path])] ?? fail("shape", "card position", [path])
  for (const row of arrayValue(root["copies"], ["copies"])) {
    const [position, section, limit] = arrayValue(row, ["copies"])
    const card = at(position, "copies")
    const deck = oneOf(["main", "evolve"] as const, stringValue(section, ["copies"]), "copies")
    card.copies = { ...card.copies, [deck]: integerValue(limit, ["copies"]) }
  }
  for (const row of arrayValue(root["singleton_except"], ["singleton_except"])) {
    const [position, exempt] = arrayValue(row, ["singleton_except"])
    at(position, "singleton_except").singletonExcept = new Set(
      arrayValue(exempt, ["singleton_except"]).map((item) => nameCode(item, "singleton_except")),
    )
  }
  for (const position of arrayValue(root["start_amulet"], ["start_amulet"]))
    at(position, "start_amulet").startAmulet = true

  const byId = new Map(ids.map((id, i) => [id, cards[i] as IndexCard]))
  const titleRules = new Map(
    Object.entries(objectValue(root["title_rules"], ["title_rules"])).map(([title, rule]) => [
      title,
      stringValue(rule, ["title_rules"]),
    ]),
  )
  const formats = new Map<string, FormatRules>(
    Object.entries(objectValue(root["formats"], ["formats"])).map(([format, value]) => {
      const rules = objectValue(value, ["formats", format])
      const coverage = arrayValue(rules["coverage"], ["coverage"]).map((range) => {
        const [from, until] = arrayValue(range, ["coverage"])
        return [stringValue(from, ["coverage"]), dateOrNull(until, "coverage")] as const
      })
      const restrictions = arrayValue(rules["restrictions"], ["restrictions"]).map(
        (item): Restriction => {
          const row = objectValue(item, ["restrictions"])
          return {
            id: stringValue(row["id"], ["restrictions", "id"]),
            kind: oneOf(KINDS, stringValue(row["kind"], ["restrictions"]), "kind"),
            from: stringValue(row["from"], ["restrictions", "from"]),
            until: dateOrNull(row["until"], "until"),
            max: intOrNull(row["max"], "max"),
            maxGroups: intOrNull(row["max_groups"], "max_groups"),
            members: arrayValue(row["members"], ["members"]).map((member) => {
              const [code, scope, option] = arrayValue(member, ["members"])
              return {
                name: nameCode(code, "members"),
                scope: oneOf(SCOPES, stringValue(scope, ["members"]), "scope"),
                option: integerValue(option, ["members"]),
              }
            }),
          }
        },
      )
      return [format, { coverage, restrictions }]
    }),
  )
  return { card: (id) => byId.get(id), titleRules, formats }
}
