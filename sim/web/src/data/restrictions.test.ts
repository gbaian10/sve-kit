// @vitest-environment node
import { expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { DEFAULT_QUERY } from "../domain/query/model"
import type { Row } from "./format-v3/decode"
import { decodeMessage } from "./format-v3/decode-message"
import { type JsonObject, stringValue } from "./format-v3/json"
import { catalogKeys } from "./format-v3/reader"
import { selectRestrictions } from "./restrictions"
import { BootstrapColumns } from "./search/bootstrap"
import { SearchEngine } from "./search/engine"
import { SearchIndex } from "./search/index"
import type { SearchEvent } from "./search/messages"

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array()) })
const bootstrap = new BootstrapColumns()
const basicKeys = new Set(
  catalogKeys(
    new Map(
      (built.manifest["files"] as JsonObject[]).map((file) => [stringValue(file["key"]), file]),
    ),
    "jp",
  ),
)
for (const file of built.manifest["files"] as JsonObject[]) {
  if (["config", "programs"].includes(stringValue(file["role"]))) continue
  const bytes = built.files.get(stringValue(file["path"]))
  if (!bytes) throw new Error("missing fixture bytes")
  const decoded = decodeMessage({ kind: "file", bytes, file, version: "4.0.0" })
  if (decoded.kind === "fragments")
    bootstrap.ingest(
      decoded.fragments.filter(
        (fragment) =>
          basicKeys.has(stringValue(file["key"])) ||
          ["rules_profile", "restriction", "rules_name", "face_rules_name"].includes(
            fragment.table,
          ),
      ),
    )
}
bootstrap.seal()
const lookup = (
  overrides: Record<string, readonly Row[]> = {},
  manifest = built.manifest,
  date = "2026-09-29",
  edition: "jp" | "en" = "jp",
) =>
  selectRestrictions(
    manifest,
    (table) => overrides[table] ?? bootstrap.rows(table),
    bootstrap.index(),
    edition,
    date,
  )

it("filters every printing of the known restriction, with identical encoded/restored results", () => {
  const selection = lookup()
  expect(selection.status).toMatchObject({
    state: "complete",
    onDate: "2026-09-29",
    synthetic: true,
  })
  expect([...selection.cards]).toEqual(["c:bp01-021"])
  const index = new SearchIndex(bootstrap, "jp", new Set(), selection)
  const query = {
    state: { ...DEFAULT_QUERY, restrictionsOnly: true },
    edition: "jp" as const,
    limit: 100,
  }
  const expected = ["p:bp01-021", "p:bp01-021-sp"]
  expect(index.search(query).items.map((row) => row.printingId)).toEqual(expected)
  expect(new SearchIndex(index.encode(), "jp").search(query)).toEqual(index.search(query))
  index.completePrintings(bootstrap)
  expect(index.options.restrictions).toEqual(selection.status)
  expect(index.search(query).items.map((row) => row.printingId)).toEqual(expected)
})

it("keeps missing, overlapping, expired and other-region coverage unknown", () => {
  expect(lookup({}, { ...built.manifest, restriction_coverage: [] }).status.state).toBe("unknown")
  const windows = built.manifest["restriction_coverage"] as JsonObject[]
  expect(
    lookup({}, { ...built.manifest, restriction_coverage: [...windows, ...windows] }).status.state,
  ).toBe("unknown")
  expect(lookup({}, built.manifest, "2026-09-30").cards.size).toBe(0)
  expect(lookup({}, built.manifest, "2026-09-29", "en").status.state).toBe("unknown")
})

it("honors lift boundaries, future announcements and unresolved current observations", () => {
  const restriction = bootstrap.rows("restriction").find((row) => row["id"] === "restr:bp01-021")
  if (!restriction) throw new Error("missing synthetic restriction")
  expect(
    lookup({ restriction: [{ ...restriction, effective_until: "2026-09-29" }] }).cards.size,
  ).toBe(0)
  expect(
    lookup({ restriction: [{ ...restriction, effective_from: "2026-10-01" }] }).cards.size,
  ).toBe(0)
  expect(lookup({ restriction: [{ ...restriction, state: "withdrawn" }] }).cards.size).toBe(0)
  expect(lookup({ restriction: [{ ...restriction, state: "announced" }] }).status.state).toBe(
    "partial",
  )
  expect(lookup({ face_rules_name: [] }).status.state).toBe("partial")
  expect(
    lookup({
      restriction: [
        {
          ...restriction,
          members: (restriction["members"] as JsonObject[]).map((row) => ({
            ...row,
            deck_scope: "evolve",
          })),
        },
      ],
    }).cards.size,
  ).toBe(0)
})

it("loads rule bindings in the real worker path and preserves the filter through background completion", async () => {
  const events: SearchEvent[] = []
  const engine = new SearchEngine((event) => events.push(event), {
    fetch: (url) => {
      const bytes = built.files.get(url.replace("https://fixture.invalid/", ""))
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  await engine.handle({
    kind: "load",
    generation: 1,
    base: "https://fixture.invalid",
    edition: "jp",
    entry: "index",
  })
  expect(events.some((event) => event.kind === "ready")).toBe(true)
  await engine.handle({
    kind: "search",
    generation: 1,
    channelId: 1,
    queryId: 1,
    query: { state: { ...DEFAULT_QUERY, restrictionsOnly: true }, edition: "jp", limit: 100 },
  })
  const result = events.findLast((event) => event.kind === "result")
  expect(result?.kind === "result" && result.page.items.map((row) => row.printingId)).toEqual([
    "p:bp01-021",
    "p:bp01-021-sp",
  ])
})

it("includes other cards with the same construction name and counts a two-faced physical card once", () => {
  const binding = bootstrap
    .rows("face_rules_name")
    .find((row) => row["rules_name_id"] === "rn:jp:f:bp01-021")
  if (!binding) throw new Error("missing synthetic name binding")
  const selection = lookup({
    face_rules_name: [
      binding,
      { ...binding, face_id: "f:bp01-001" },
      ...bootstrap
        .index()
        .facesOf("c:bp01-060")
        .map((face) => ({ ...binding, face_id: face["id"] })),
    ],
  })
  expect([...selection.cards].sort()).toEqual(["c:bp01-001", "c:bp01-021", "c:bp01-060"])
})

it.each([null, "missing"])(
  "reports partial coverage when a scoped member has unknown deck role: %s",
  (role) => {
    const index = bootstrap.index()
    const card = index.card("c:bp01-021")
    if (!card) throw new Error("missing synthetic card")
    const restriction = bootstrap.rows("restriction").find((row) => row["id"] === "restr:bp01-021")
    if (!restriction) throw new Error("missing synthetic restriction")
    const changed = {
      ...index,
      card: (id: string) =>
        id === "c:bp01-021"
          ? {
              ...card,
              regions:
                role === "missing"
                  ? []
                  : (card["regions"] as JsonObject[]).map((region) => ({
                      ...region,
                      deck_role: null,
                    })),
            }
          : index.card(id),
    }
    const selection = selectRestrictions(
      built.manifest,
      (table) =>
        table === "restriction"
          ? [
              {
                ...restriction,
                members: (restriction["members"] as JsonObject[]).map((member) => ({
                  ...member,
                  deck_scope: "main",
                })),
              },
            ]
          : bootstrap.rows(table),
      changed,
      "jp",
      "2026-09-29",
    )
    expect(selection.status.state).toBe("partial")
    expect(selection.cards.size).toBe(0)
  },
)
