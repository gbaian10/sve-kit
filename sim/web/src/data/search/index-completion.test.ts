// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { DEFAULT_QUERY } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import { decodeMessage } from "../format-v3/decode-message"
import { arrayValue, objectValue, stringValue } from "../format-v3/json"
import { catalogKeys, type Fragment } from "../format-v3/reader"
import { BootstrapColumns } from "./bootstrap"
import { SearchIndex, SearchQueryError } from "./index"
import { compareSearchIndexes } from "./test-utils/search-equivalence"

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array()) })

function stages(edition: Region) {
  const manifest = decodeMessage({
    kind: "manifest",
    bytes: built.files.get(built.manifestPath) ?? new Uint8Array(),
  })
  if (manifest.kind !== "manifest") throw new Error("manifest missing")
  const basic: Fragment[] = [],
    printing: Fragment[] = []
  const startup = new Set(catalogKeys(manifest.files, edition))
  for (const file of manifest.files.values()) {
    const target = startup.has(stringValue(file["key"]))
      ? basic
      : ["print", "product", "engine"].includes(stringValue(file["purpose"])) &&
          (file["region"] === null || file["region"] === edition)
        ? printing
        : undefined
    if (!target) continue
    const result = decodeMessage({
      kind: "file",
      file,
      version: "4.0.0",
      bytes: built.files.get(stringValue(file["path"])) ?? new Uint8Array(),
    })
    if (result.kind === "fragments") target.push(...result.fragments)
  }
  return { basic, printing }
}

function columns(fragments: readonly Fragment[]) {
  const result = new BootstrapColumns()
  result.ingest(fragments)
  result.seal()
  return result
}

const PARENTS = ["main", "evolve", "advance", "leader", "token", "ep", "sep"] as const

// Every printing lands in exactly one parent location, whatever special kinds it combines.
function expectExclusiveLocations(index: SearchIndex, edition: Region) {
  const query = (locations: readonly string[]) =>
    index.search({ edition, state: { ...DEFAULT_QUERY, locations }, limit: 1000 })
  const total = query([]).total
  const members = new Map(PARENTS.map((parent) => [parent, query([parent]).items]))
  for (const [parent, items] of members) {
    expect(items.length, parent).toBe(index.options.locationCounts[parent] ?? 0)
    for (const other of PARENTS) {
      if (other === parent) continue
      const ids = new Set(members.get(other)?.map((item) => item.printingId))
      expect(items.filter((item) => ids.has(item.printingId)).length, `${parent} ∩ ${other}`).toBe(
        0,
      )
    }
  }
  expect(PARENTS.reduce((sum, parent) => sum + (members.get(parent)?.length ?? 0), 0)).toBe(total)
  expect(query([...PARENTS]).total).toBe(total)
  // Sub-values partition their parent the same way.
  const children = (parent: string) =>
    Object.entries(index.options.locationCounts)
      .filter(([token]) => token.startsWith(`${parent}:`))
      .reduce((sum, [, count]) => sum + count, 0)
  expect(children("token")).toBe(index.options.locationCounts["token"] ?? 0)
  expect(children("leader")).toBe(index.options.locationCounts["leader"] ?? 0)
  return members
}

describe("deck locations stay exclusive across combined special kinds", () => {
  it.each(["jp", "en"] as const)(
    "resolve by priority at build, after restore and after completion for %s",
    (edition) => {
      const { basic, printing } = stages(edition)
      // Give the fixture every combination the data could carry: Token over evolve, evolve over
      // advance, and a Token or evolve mark on a leader, EP or spell.
      const seen = new Map<string, number>()
      const kindsFor = (type: string, region: string): string[] => {
        const nth = (seen.get(`${type}/${region}`) ?? 0) + 1
        seen.set(`${type}/${region}`, nth)
        if (type === "leader") return nth % 2 ? ["token"] : ["evolve"]
        if (type === "ep" || type === "sep") return ["evolve", "advance"]
        if (type === "follower") return ["evolve", "token"]
        if (type === "spell") return nth % 2 ? ["evolve", "advance"] : ["advance"]
        return []
      }
      const combined = basic.map((fragment) =>
        fragment.table === "face_revision"
          ? {
              ...fragment,
              rows: fragment.rows.map((row) => ({
                ...row,
                special_kinds: kindsFor(stringValue(row["type_code"]), stringValue(row["region"])),
              })),
            }
          : fragment,
      )
      const source = columns(combined)
      const index = new SearchIndex(source, edition)
      const built = expectExclusiveLocations(index, edition)
      const { locationCounts: counts, typeCounts: types } = index.options
      // Token wins over evolve: every follower is a Token and none is an evolve card; evolve wins
      // over advance for the odd spells; EP and SEP keep their type whatever kinds they carry.
      expect(counts["token:follower"]).toBe(types["follower"])
      expect((counts["evolve"] ?? 0) + (counts["advance"] ?? 0)).toBe(types["spell"])
      expect(counts["evolve"]).toBeGreaterThan(0)
      expect(counts["advance"]).toBeGreaterThan(0)
      expect((counts["token:leader"] ?? 0) + (counts["leader"] ?? 0)).toBe(types["leader"])
      expect(counts["token:leader"]).toBeGreaterThan(0)
      expect(counts["ep"] ?? 0).toBe(types["ep"] ?? 0)
      expectExclusiveLocations(new SearchIndex(index.encode(), edition), edition)
      source.ingest(printing)
      source.seal()
      index.completePrintings(source)
      const completed = expectExclusiveLocations(index, edition)
      expect([...completed.values()].map((items) => items.length)).toEqual(
        [...built.values()].map((items) => items.length),
      )
      expectExclusiveLocations(new SearchIndex(index.encode(), edition), edition)
    },
  )
})

describe("printing completion without replacing the search index", () => {
  it.each(["jp", "en"] as const)("matches a full build and restored image for %s", (edition) => {
    const { basic, printing } = stages(edition)
    const source = columns(basic)
    const index = new SearchIndex(source, edition)
    const expectedBasic = new SearchIndex(columns(basic), edition)
    expect(compareSearchIndexes(index, expectedBasic, edition).printings).toBeGreaterThan(0)
    const restoredInitial = new SearchIndex(index.encode(), edition)
    source.ingest(printing)
    source.seal()
    index.completePrintings(source)
    restoredInitial.completePrintings(source)
    const expected = new SearchIndex(columns([...basic, ...printing]), edition)
    const completed = compareSearchIndexes(index, expected, edition)
    expect(completed.comparisons).toBeGreaterThan(100)
    expect(compareSearchIndexes(restoredInitial, expected, edition).digest).toBe(completed.digest)
    expect(
      compareSearchIndexes(new SearchIndex(index.encode(), edition), expected, edition).digest,
    ).toBe(completed.digest)
  })

  it.each(["jp", "en"] as const)(
    "matches a full build without a default printing for %s",
    (edition) => {
      const stagesForEdition = stages(edition)
      const basic = stagesForEdition.basic.map((fragment) =>
        fragment.table !== "card"
          ? fragment
          : {
              ...fragment,
              rows: fragment.rows.map((row) => ({
                ...row,
                regions: arrayValue(row["regions"]).map((value) => {
                  const view = { ...objectValue(value) }
                  if (view["region"] === edition) delete view["default_printing_id"]
                  return view
                }),
              })),
            },
      )
      const source = columns(basic)
      const index = new SearchIndex(source, edition)
      const restoredInitial = new SearchIndex(index.encode(), edition)
      source.ingest(stagesForEdition.printing)
      source.seal()
      index.completePrintings(source)
      restoredInitial.completePrintings(source)
      const expected = new SearchIndex(columns([...basic, ...stagesForEdition.printing]), edition)
      const completed = compareSearchIndexes(index, expected, edition)
      expect(compareSearchIndexes(restoredInitial, expected, edition).digest).toBe(completed.digest)
      expect(
        compareSearchIndexes(new SearchIndex(index.encode(), edition), expected, edition).digest,
      ).toBe(completed.digest)
    },
  )

  it("preserves the initial index when a closure changes locator membership", () => {
    const { basic, printing } = stages("jp")
    const source = columns(basic)
    const index = new SearchIndex(source, "jp")
    const before = new SearchIndex(index.encode(), "jp")
    let added = false
    source.ingest(
      printing.map((fragment) => {
        const row = fragment.rows.find((row) => row["region"] === "jp")
        if (fragment.table !== "printing" || !row || added) return fragment
        added = true
        return { ...fragment, rows: [...fragment.rows, { ...row, id: "p:synthetic-unlisted" }] }
      }),
    )
    expect(added).toBe(true)
    source.seal()
    expect(() => {
      index.completePrintings(source)
    }).toThrow("printing closure changes catalog membership")
    expect(compareSearchIndexes(index, before, "jp").printings).toBeGreaterThan(0)
  })
})

it("applies keyword unions before grouping and excludes unknown from negative queries", () => {
  const { basic, printing } = stages("jp")
  const source = columns(basic)
  const created = new SearchIndex(source, "jp")
  source.ingest(printing)
  source.seal()
  created.completePrintings(source)
  created.completeMechanics(source)
  for (const index of [created, new SearchIndex(created.encode(), "jp")]) {
    expect(index.options.keywordCoverage).toBe("partial")
    const find = (
      mechanics: Record<string, "has" | "not">,
      unit: "printing" | "art" | "card" = "card",
      extra = {},
    ) =>
      index.search({
        edition: "jp",
        state: { ...DEFAULT_QUERY, unit, mechanics, ...extra },
        limit: 100,
      })
    const ward = find({ "kw:ward": "has" }).items.map((item) => item.key)
    expect(ward).toEqual(["c:bp01-011", "c:bp01-031", "c:bp01-051"])
    const union = find({ "kw:ward": "has", "kw:evolve": "has" }).items.map((item) => item.key)
    expect(union).toEqual(["c:bp01-011", "c:bp01-030", "c:bp01-031", "c:bp01-051", "c:bp01-081"])
    const without = find({ "kw:ward": "not" }).items.map((item) => item.key)
    expect(without).toContain("c:bp01-003")
    expect(without).not.toContain("c:bp01-040")
    expect(without).not.toContain("c:bp01-002")
    expect(without).not.toContain("c:bp01-011")
    const byName = find({ "kw:fanfare": "has" }, "card", {
      text: "試作の魔導士",
      representative: "high",
    })
    expect(byName.items[0]?.printingId).toBe("p:bp01-021-sp")
    const filtered = find({ "kw:fanfare": "has" }, "card", {
      text: "BP01-021",
      representative: "high",
      rarities: ["legend"],
    })
    expect(filtered.items[0]?.printingId).toBe("p:bp01-021")
    for (const unit of ["printing", "art", "card"] as const) {
      const page = find({ "kw:fanfare": "has" }, unit, { text: "BP01-021", representative: "high" })
      expect(page.total).toBe(unit === "card" ? 1 : 2)
      if (unit === "card") expect(page.items[0]?.printingId).toBe("p:bp01-021")
      expect(page.items.map((item) => item.summary?.cardId)).toEqual(
        Array.from({ length: page.total }, () => "c:bp01-021"),
      )
      expect(find({ "kw:ward": "has" }, unit, { classes: ["witch"] }).total).toBe(0)
    }
  }
  const en = stages("en")
  const englishSource = columns([...en.basic, ...en.printing])
  const english = new SearchIndex(englishSource, "en")
  english.completeMechanics(englishSource)
  expect(english.options.keywordCoverage).toBe("partial")
  for (const state of ["has", "not"] as const) {
    const result = english.search({
      edition: "en",
      state: { ...DEFAULT_QUERY, unit: "card", mechanics: { "kw:ward": state } },
      limit: 100,
    })
    expect(result.items.map((item) => item.key)).not.toContain("c:bp01-031")
    if (state === "has") expect(result.items.map((item) => item.key)).toContain("c:bp01-051")
  }
})

it("rejects mechanic queries when every EN shared annotation is blocked", () => {
  const { basic, printing } = stages("en")
  const source = columns(
    [...basic, ...printing].map((fragment) =>
      fragment.table === "card_engine_support"
        ? {
            ...fragment,
            rows: fragment.rows.map((row) => ({
              ...row,
              region_blocks: [{ region: "en", reasons: ["unconfirmed_mapping"] }],
            })),
          }
        : fragment,
    ),
  )
  const index = new SearchIndex(source, "en")
  index.completeMechanics(source)
  expect(index.options.keywordCoverage).toBe("unknown")
  for (const state of ["has", "not"] as const) {
    try {
      index.search({
        edition: "en",
        state: { ...DEFAULT_QUERY, mechanics: { "kw:ward": state } },
        limit: 100,
      })
      expect.fail("unknown coverage must reject the query")
    } catch (error) {
      expect(error).toBeInstanceOf(SearchQueryError)
      expect((error as SearchQueryError).failure).toMatchObject({
        kind: "unsupported-query",
        facets: ["mechanics"],
      })
    }
  }
})
