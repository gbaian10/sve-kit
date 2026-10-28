// @vitest-environment node
import { expect, it } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY, type QueryState } from "../../domain/query/model"
import type { Fetcher } from "../cdn"
import { createSnapshotClient } from "../client"
import { arrayValue, type JsonObject, objectValue } from "../format-v3/json"
import { BootstrapColumns } from "./bootstrap"
import { SearchIndex } from "./index"

const built = await buildSnapshot({
  encodeImage: ({ seed }) => Promise.resolve(new TextEncoder().encode(String(seed))),
})
const base = "https://cdn.test"
const fetcher: Fetcher = (url) => {
  const data = built.files.get(url.slice(base.length + 1))
  return Promise.resolve(
    data ? new Response(data.slice().buffer) : new Response(null, { status: 404 }),
  )
}

/** The printing row with one art per face (null clears the art), and new scalar values. */
function withArt(
  row: JsonObject,
  arts: readonly (string | null)[],
  changes: JsonObject,
): JsonObject {
  return {
    ...row,
    ...changes,
    faces: arrayValue(row["faces"]).map((face, ordinal) => ({
      ...objectValue(face),
      art_id: arts[ordinal] ?? arts[0] ?? null,
    })),
  }
}

// The maintainer's example re-created on the fixture: the mage has LG and SL on one art, UR on
// another and a PR of the first art in the promo set; the pair of followers has no art at all; the
// double-faced card gets a reprint sharing its front art while the back differs, and one with
// another front; the Token gains a BR reprint.
async function modelledIndex(): Promise<SearchIndex> {
  const client = createSnapshotClient(base, { fetch: fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const columns = new BootstrapColumns()
  // A printing's home set is its fragment's owner, so reprints in other sets get their own fragment.
  const elsewhere = new Map<string, JsonObject[]>()
  const reprint = (owner: string, row: JsonObject): JsonObject[] => {
    elsewhere.set(owner, [...(elsewhere.get(owner) ?? []), row])
    return []
  }
  columns.ingest(
    snapshot.bootstrap.flatMap((fragment) => {
      if (fragment.table !== "printing") return [fragment]
      const rows = fragment.rows.flatMap((row): JsonObject[] => {
        switch (row["id"]) {
          case "p:bp01-021":
            return [
              withArt(row, ["art:rose:a"], { rarity_code: "lg" }),
              withArt(row, ["art:rose:b"], {
                id: "p:bp01-021-ur",
                card_no: "BP01-U01",
                int_id: 1221,
                rarity_code: "ur",
              }),
              ...reprint(
                "set:pr",
                withArt(row, ["art:rose:a"], {
                  id: "p:bp01-021-pr",
                  card_no: "PR-021",
                  int_id: 3021,
                  rarity_code: "pr",
                }),
              ),
            ]
          case "p:bp01-021-sp":
            return [withArt(row, ["art:rose:a"], { card_no: "BP01-SL01", rarity_code: "sl" })]
          case "p:bp01-002":
          case "p:bp01-002-alt":
            return [withArt(row, [null], {})]
          case "p:bp01-030":
            return [
              withArt(row, ["art:twin:front", "art:twin:back"], { rarity_code: "gr" }),
              ...reprint(
                "set:sd01",
                withArt(row, ["art:twin:front", "art:twin:back-2"], {
                  id: "p:bp01-030-reprint",
                  card_no: "SD01-030",
                  int_id: 2030,
                  rarity_code: "lg",
                }),
              ),
              withArt(row, ["art:twin:front-2", "art:twin:back"], {
                id: "p:bp01-030-alt",
                card_no: "BP01-SL30",
                int_id: 1230,
                rarity_code: "sl",
              }),
            ]
          case "p:token-001":
            return [
              { ...row, rarity_code: null },
              {
                ...row,
                id: "p:token-001-br",
                card_no: "BP01-T02",
                int_id: 1902,
                rarity_code: "br",
              },
            ]
          default:
            return [row]
        }
      })
      return [
        { ...fragment, rows },
        ...[...elsewhere].map(([owner, rows]) => ({
          ...fragment,
          value: { ...fragment.value, owner: { kind: "home_set", id: owner } },
          rows,
        })),
      ]
    }),
  )
  columns.seal()
  return new SearchIndex(columns, "jp")
}

const created = await modelledIndex()
const indexes = [created, new SearchIndex(created.encode(), "jp")]

type Changes = Partial<QueryState>
const cells = (index: SearchIndex, cardId: string, changes: Changes) =>
  index
    .search({ edition: "jp", limit: 1000, state: { ...DEFAULT_QUERY, unit: "card", ...changes } })
    .items.filter((item) => item.summary?.cardId === cardId)
    .map((item) => [item.key, item.printingId])
const representatives = (index: SearchIndex, cardId: string, changes: Changes) =>
  cells(index, cardId, changes).map(([, printingId]) => printingId)

it.each(indexes.map((index, stage) => [stage, index] as const))(
  "merges by art or by card and shows the chosen rarity (stage %i)",
  (_stage, index) => {
    const mage = "c:bp01-021"
    expect(representatives(index, mage, { unit: "printing" })).toEqual([
      "p:bp01-021",
      "p:bp01-021-sp",
      "p:bp01-021-ur",
      "p:bp01-021-pr",
    ])
    // Same art, lowest: LG and UR; highest: SL and UR; the PR shares the first art.
    expect(cells(index, mage, { unit: "art" })).toEqual([
      ["art:rose:a", "p:bp01-021"],
      ["art:rose:b", "p:bp01-021-ur"],
    ])
    expect(representatives(index, mage, { unit: "art", representative: "high" })).toEqual([
      "p:bp01-021-sp",
      "p:bp01-021-ur",
    ])
    expect(representatives(index, mage, { unit: "art", prFirst: true })).toEqual([
      "p:bp01-021-ur",
      "p:bp01-021-pr",
    ])
    expect(cells(index, mage, { unit: "card" })).toEqual([[mage, "p:bp01-021"]])
    expect(representatives(index, mage, { unit: "card", representative: "high" })).toEqual([
      "p:bp01-021-ur",
    ])
    expect(representatives(index, mage, { unit: "card", prFirst: true })).toEqual(["p:bp01-021-pr"])
    expect(
      representatives(index, mage, { unit: "card", representative: "high", prFirst: true }),
    ).toEqual(["p:bp01-021-pr"])
  },
)

it.each(indexes.map((index, stage) => [stage, index] as const))(
  "picks the representative only among the printings the filters keep (stage %i)",
  (_stage, index) => {
    const mage = "c:bp01-021"
    expect(representatives(index, mage, { unit: "card", rarities: ["ur"] })).toEqual([
      "p:bp01-021-ur",
    ])
    // BP01-SL01 is the fixture's signed premium printing. A tier-only selection excludes it, so
    // the non-premium UR printing is the only candidate left for representative selection.
    expect(representatives(index, mage, { unit: "card", rarities: ["sl", "ur"] })).toEqual([
      "p:bp01-021-ur",
    ])
    // Without its PR in the result, "prefer PR" falls back to the rarity choice.
    expect(representatives(index, mage, { unit: "card", sets: ["bp01"], prFirst: true })).toEqual([
      "p:bp01-021",
    ])
    expect(representatives(index, mage, { unit: "card", sets: ["pr"] })).toEqual(["p:bp01-021-pr"])
    expect(cells(index, mage, { unit: "art", rarities: ["sl", "pr"] })).toEqual([
      ["art:rose:a", "p:bp01-021-pr"],
    ])
    // Text: an exact number keeps that printing; a name hits every printing, so the rule applies.
    expect(
      representatives(index, mage, { unit: "card", text: "BP01-021", representative: "high" }),
    ).toEqual(["p:bp01-021"])
    expect(
      representatives(index, mage, { unit: "card", text: "bp01", representative: "high" }),
    ).toEqual(["p:bp01-021-ur"])
    expect(
      representatives(index, mage, { unit: "card", text: "試作の魔導士", prFirst: true }),
    ).toEqual(["p:bp01-021-pr"])
    expect(
      cells(index, mage, { unit: "art", text: "試作の魔導士", representative: "high" }),
    ).toEqual([
      ["art:rose:a", "p:bp01-021-sp"],
      ["art:rose:b", "p:bp01-021-ur"],
    ])
  },
)

it.each(indexes.map((index, stage) => [stage, index] as const))(
  "keeps printings without an art apart and judges a double-faced card by its front (stage %i)",
  (_stage, index) => {
    expect(cells(index, "c:bp01-002", { unit: "art" })).toEqual([
      ["p:bp01-002", "p:bp01-002"],
      ["p:bp01-002-alt", "p:bp01-002-alt"],
    ])
    expect(representatives(index, "c:bp01-002", { unit: "card" })).toEqual(["p:bp01-002"])
    const twin = "c:bp01-030"
    expect(cells(index, twin, { unit: "art" })).toEqual([
      ["art:twin:front", "p:bp01-030"],
      ["art:twin:front-2", "p:bp01-030-alt"],
    ])
    // Equal release dates put SD01 before BP01 under the default descending set sort.
    expect(representatives(index, twin, { unit: "art", representative: "high" })).toEqual([
      "p:bp01-030-reprint",
      "p:bp01-030-alt",
    ])
    expect(
      index
        .search({
          edition: "jp",
          limit: 1000,
          state: parseQuery(new URLSearchParams("unit=art&rep=high&sort=no:asc")),
        })
        .items.filter((item) => item.summary?.cardId === twin)
        .map((item) => item.printingId),
    ).toEqual(["p:bp01-030-alt", "p:bp01-030-reprint"])
    expect(representatives(index, twin, { unit: "card", representative: "high" })).toEqual([
      "p:bp01-030-alt",
    ])
  },
)

it.each(indexes.map((index, stage) => [stage, index] as const))(
  "ranks a printing without a rarity below BR (stage %i)",
  (_stage, index) => {
    const token = "c:token-001"
    expect(representatives(index, token, { unit: "card" })).toEqual(["p:token-001"])
    expect(representatives(index, token, { unit: "card", representative: "high" })).toEqual([
      "p:token-001-br",
    ])
    expect(representatives(index, token, { unit: "card", prFirst: true })).toEqual(["p:token-001"])
  },
)

it("counts cards once whatever the unit and orders merged cells by their representative", () => {
  const index = created
  const search = (changes: Changes) =>
    index.search({ edition: "jp", limit: 1000, state: { ...DEFAULT_QUERY, ...changes } })
  const printings = search({ unit: "printing" })
  const arts = search({ unit: "art" })
  const cards = search({ unit: "card" })
  expect(cards.total).toBeLessThan(arts.total)
  expect(arts.total).toBeLessThan(printings.total)
  expect(index.count({ edition: "jp", state: { ...DEFAULT_QUERY, unit: "art" } }).cards).toBe(
    cards.total,
  )
  expect(new Set(arts.items.map((item) => item.key)).size).toBe(arts.total)
  // The mage's UR sorts at its own number, after the followers printed between 021 and U01.
  const keys = search({ unit: "card", representative: "high" }).items.map((item) => item.key)
  const printingOrder = printings.items.map((item) => item.printingId)
  expect(keys.indexOf("c:bp01-021")).toBe(
    search({ unit: "card", representative: "high" }).items.findIndex(
      (item) => item.printingId === "p:bp01-021-ur",
    ),
  )
  expect(printingOrder.indexOf("p:bp01-021-ur")).toBeGreaterThan(
    printingOrder.indexOf("p:bp01-021"),
  )
})
