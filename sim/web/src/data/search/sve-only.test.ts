// @vitest-environment node
import { expect, it } from "vitest"

import { DEFAULT_QUERY } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import type { JsonObject } from "../format-v3/json"
import type { Fragment } from "../format-v3/reader"
import { BootstrapColumns } from "./bootstrap"
import { SearchIndex } from "./index"

function fragment(table: string, rows: JsonObject[]): Fragment {
  return {
    table,
    rows,
    file: table,
    identity: table,
    value: { purpose: "print", owner: { kind: "home_set", id: "bp01" }, base: null },
  }
}

const cases = [
  { card: "unlinked", arts: ["matched", "unknown"] },
  { card: "linked", arts: ["matched", "exclusive", "unknown", "missing", "partial", "exclusive"] },
  { card: "single-game", arts: ["partial"] },
  { card: "unresolved-target", arts: ["exclusive"] },
  { card: "character-only", arts: ["matched"] },
]

function bootstrap(catalogOnly = false): BootstrapColumns {
  const columns = new BootstrapColumns()
  columns.ingest([
    fragment("product_family", [
      {
        id: "bp01",
        code: "bp01",
        public_code: "BP01",
        kind: "booster",
        name_unit_id: "name",
        translations: [],
      },
    ]),
    fragment("text_unit", [{ id: "name", lang: "ja", text: "Synthetic card" }]),
    ...cases.flatMap(({ card, arts }) => [
      fragment("card", [
        {
          id: card,
          home_set_id: "bp01",
          regions: ["jp", "en"].map((region) => ({
            region,
            default_printing_id: `${card}:${region}:0`,
          })),
        },
      ]),
      fragment("face", [
        {
          id: `f:${card}`,
          card_id: card,
          ordinal: 0,
          current: ["jp", "en"].map((region) => ({ region, revision_id: `r:${card}` })),
          wording: [],
        },
      ]),
      fragment("face_revision", [
        {
          id: `r:${card}`,
          face_id: `f:${card}`,
          name_unit_id: "name",
          translations: [],
          type_code: "follower",
          traits: [],
          special_kinds: [],
          cost: 1,
          attack: 1,
          defense: 1,
        },
      ]),
      fragment(
        "printing",
        ["jp", "en"].flatMap((region) =>
          arts.map((art, i) => ({
            id: `${card}:${region}:${String(i)}`,
            card_id: card,
            region,
            card_no: `BP01-${card}-${String(i)}${region === "en" ? "EN" : ""}`,
            rarity_code: "gr",
            ...(catalogOnly ? {} : { faces: [{ face_id: `f:${card}`, art_id: `${card}:${art}` }] }),
          })),
        ),
      ),
    ]),
    ...(catalogOnly
      ? []
      : [
          fragment("digital_card", [
            { id: "d:1", game: "sv1" },
            { id: "d:2", game: "svwb" },
          ]),
          fragment("digital_link", [
            ...["d:1", "d:2"].map((digital_card_id) => ({
              card_id: "linked",
              digital_card_id,
              relation: "same_card",
            })),
            { card_id: "single-game", digital_card_id: "d:1", relation: "same_card" },
            { card_id: "unresolved-target", digital_card_id: "missing", relation: "same_card" },
            { card_id: "character-only", digital_card_id: "d:1", relation: "same_character" },
          ]),
          fragment(
            "digital_art_coverage",
            cases.flatMap(({ card }) =>
              [
                { art_id: "matched", game: "sv1", state: "matched" },
                { art_id: "matched", game: "svwb", state: "not_found" },
                { art_id: "exclusive", game: "sv1", state: "not_found" },
                { art_id: "exclusive", game: "svwb", state: "not_found" },
                { art_id: "unknown", game: "sv1", state: "not_found" },
                { art_id: "unknown", game: "svwb", state: "unknown" },
                { art_id: "partial", game: "sv1", state: "not_found" },
              ].map((row) => ({ ...row, art_id: `${card}:${row.art_id}` })),
            ),
          ),
        ]),
  ])
  columns.seal()
  return columns
}

const full = bootstrap()
const catalog = bootstrap(true)
it.each(["jp", "en"] as const)(
  "filters SVE-exclusive %s printings before grouping and persists the result",
  (edition: Region) => {
    const created = new SearchIndex(full, edition)
    const completed = new SearchIndex(catalog, edition)
    completed.completePrintings(full)
    completed.completeMechanics(full)
    completed.completeSveOnly(full)
    expect(completed.sveOnlyReady).toBe(true)
    const eligible = ["unlinked", "linked", "single-game", "character-only"]
    const printings = eligible.flatMap((card) => {
      const positions = card === "unlinked" ? [0, 1] : card === "linked" ? [1, 5] : [0]
      return positions.map((i) => `${card}:${edition}:${String(i)}`)
    })
    for (const index of [
      created,
      new SearchIndex(created.encode(), edition),
      completed,
      new SearchIndex(completed.encode(), edition),
    ]) {
      for (const unit of ["printing", "art", "card"] as const) {
        const query = {
          edition,
          language: "en" as const,
          offset: 0,
          limit: 100,
          state: { ...DEFAULT_QUERY, unit, sveOnly: true },
        }
        const result = index.search(query)
        expect(result.items.map((item) => item.summary?.cardId).sort()).toEqual(
          (unit === "card"
            ? eligible
            : unit === "art"
              ? ["unlinked", "unlinked", "linked", "single-game", "character-only"]
              : ["unlinked", "unlinked", "linked", "linked", "single-game", "character-only"]
          ).sort(),
        )
        for (const item of result.items) expect(printings).toContain(item.summary?.printingId)
        if (unit === "printing") {
          expect(result.items.map((item) => item.summary?.printingId).sort()).toEqual(
            printings.toSorted(),
          )
          expect(index.count(query).cards).toBe(eligible.length)
        }
      }
    }
    expect(new SearchIndex(completed.encode(), edition).sveOnlyReady).toBe(true)
  },
)
