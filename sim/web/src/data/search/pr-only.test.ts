// @vitest-environment node
import { expect, it } from "vitest"

import { DEFAULT_QUERY } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import { type JsonObject, stringValue } from "../format-v3/json"
import type { Fragment } from "../format-v3/reader"
import { BootstrapColumns } from "./bootstrap"
import { SearchIndex } from "./index"

function fragment(table: string, rows: JsonObject[], owner = "pr"): Fragment {
  return {
    table,
    rows,
    file: table,
    identity: `${table}/${owner}`,
    value: { purpose: "print", owner: { kind: "home_set", id: owner }, base: null },
  }
}
const printing = (
  id: string,
  region: Region,
  card_no: string,
  extra: JsonObject = {},
): JsonObject => ({
  id,
  region,
  card_no,
  rarity_code: "pr",
  stamp: null,
  serial: false,
  ...extra,
})
const cases = [
  {
    id: "all-pr",
    prints: [
      printing("all:1", "jp", "PR-001", { stamp: {}, serial: true }),
      printing("all:2", "jp", "PR-002"),
    ],
  },
  {
    id: "reprinted",
    prints: [
      printing("re:pr", "jp", "PR-003"),
      printing("re:bp", "jp", "BP01-001", { home: "bp" }),
    ],
  },
  {
    id: "bilingual",
    prints: [printing("bi:jp", "jp", "PR-438"), printing("bi:en", "en", "PR-464EN")],
  },
  {
    id: "en-booster",
    prints: [
      printing("en:pr", "jp", "PR-005"),
      printing("en:bp", "en", "BP01-002EN", { home: "bp" }),
    ],
  },
  {
    id: "jp-booster",
    prints: [
      printing("jp:bp", "jp", "BP01-003", { home: "bp" }),
      printing("jp:pr", "en", "PR-006EN"),
    ],
  },
  {
    id: "unlisted",
    prints: [
      printing("unlisted:jp", "jp", "", { card_no_state: "none", catalog_state: "unlisted" }),
    ],
  },
  {
    id: "mixed-unlisted",
    prints: [
      printing("mixed:jp", "jp", "PR-007"),
      printing("mixed:en", "en", "", { card_no_state: "none", catalog_state: "unlisted" }),
    ],
  },
  { id: "other-promo", prints: [printing("other:jp", "jp", "BSF2024-001", { home: "other" })] },
  { id: "empty", prints: [] },
  { id: "leader", type: "leader", prints: [printing("leader:jp", "jp", "PR-405")] },
  {
    id: "token",
    type: "spell",
    special: ["token"],
    prints: [printing("token:jp", "jp", "PR-155")],
  },
  { id: "ep", type: "ep", prints: [printing("ep:jp", "jp", "PR-417")] },
]
function columns(edition?: Region) {
  const columns = new BootstrapColumns()
  columns.ingest([
    fragment("product_family", [
      {
        name_unit_id: "name",
        translations: [],
        id: "pr",
        code: "pr",
        public_code: "PR",
        kind: "promo",
      },
      {
        name_unit_id: "name",
        translations: [],
        id: "bp",
        code: "bp01",
        public_code: "BP01",
        kind: "booster",
      },
      {
        name_unit_id: "name",
        translations: [],
        id: "other",
        code: "bsf2024",
        public_code: "BSF2024",
        kind: "promo",
      },
    ]),
    fragment("text_unit", [{ id: "name", lang: "ja", text: "Synthetic name" }]),
    ...cases.flatMap(({ id, prints, type, special }) => [
      fragment(
        "card",
        [
          {
            id,
            home_set_id: "pr",
            regions: ["jp", "en"].map((region) => ({
              region,
              default_printing_id: prints.find((p) => p["region"] === region)?.["id"] ?? null,
            })),
          },
        ],
        id,
      ),
      fragment(
        "face",
        [
          {
            id: `f:${id}`,
            card_id: id,
            ordinal: 0,
            current: ["jp", "en"].map((region) => ({ region, revision_id: `r:${id}` })),
            wording: [],
          },
        ],
        id,
      ),
      fragment(
        "face_revision",
        [
          {
            id: `r:${id}`,
            face_id: `f:${id}`,
            name_unit_id: "name",
            translations: [],
            type_code: type ?? "follower",
            traits: [],
            special_kinds: special ?? [],
            cost: 1,
            attack: 1,
            defense: 1,
          },
        ],
        id,
      ),
      ...prints
        .filter((p) => !edition || p["region"] === edition)
        .map((p) =>
          fragment(
            "printing",
            [{ ...p, card_id: id }],
            typeof p["home"] === "string" ? p["home"] : "pr",
          ),
        ),
    ]),
  ])
  columns.seal()
  return columns
}

it.each(["jp", "en"] as const)(
  "requires every printing across both regions to be PR in %s",
  (edition) => {
    const full = columns()
    const created = new SearchIndex(full, edition)
    const completed = new SearchIndex(columns(edition), edition)
    completed.completePrintings(full)
    for (const index of [
      created,
      new SearchIndex(created.encode(), edition),
      completed,
      new SearchIndex(completed.encode(), edition),
    ]) {
      const search = (change: Partial<typeof DEFAULT_QUERY> = {}) =>
        index.search({ edition, state: { ...DEFAULT_QUERY, prOnly: true, ...change }, limit: 100 })
      const expected =
        edition === "jp" ? ["all-pr", "bilingual", "leader", "token", "ep"] : ["bilingual"]
      for (const unit of ["printing", "art", "card"] as const) {
        expect(
          [...new Set(search({ unit }).items.map((item) => item.summary?.cardId))].sort(),
        ).toEqual([...expected].sort())
      }
      if (edition === "jp") {
        expect(
          search({ stampOnly: true, serialOnly: true }).items.map((item) => item.printingId),
        ).toEqual(["all:1"])
        expect(search({ unlistedOnly: true }).items).toEqual([])
        expect(
          search({ locations: ["main"] }).items.some((item) => item.summary?.cardId === "leader"),
        ).toBe(false)
      }
      expect(search({ prOnly: false }).total).toBeGreaterThan(search().total)
    }
  },
)

it.each(["jp", "en"] as const)(
  "disqualifies local PR candidates lazily and persists the %s completion flag",
  (edition) => {
    const local = columns(edition)
    const index = new SearchIndex(local, edition)
    const restoredLocal = new SearchIndex(index.encode(), edition)
    expect(restoredLocal.prOnlyReady).toBe(false)
    const full = columns()
    const fullIndex = full.index()
    restoredLocal.completePrOnly(
      fullIndex.cards
        .flatMap((card) => fullIndex.printingsOf(stringValue(card["id"])))
        .filter((printing) => printing["region"] !== edition),
    )
    expect(restoredLocal.prOnlyReady).toBe(true)
    const restored = new SearchIndex(restoredLocal.encode(), edition)
    expect(restored.prOnlyReady).toBe(true)
    const request = {
      edition,
      state: { ...DEFAULT_QUERY, prOnly: true, unit: "card" as const },
      limit: 100,
    }
    expect(restored.search(request)).toEqual(new SearchIndex(full, edition).search(request))
    restored.completePrintings(local)
    expect(restored.search(request)).toEqual(new SearchIndex(full, edition).search(request))
  },
)
