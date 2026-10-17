// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import type { Region } from "../../domain/search"
import { decodeMessage } from "../format-v3/decode-message"
import { arrayValue, objectValue, stringValue } from "../format-v3/json"
import { catalogKeys, type Fragment } from "../format-v3/reader"
import { BootstrapColumns } from "./bootstrap"
import { SearchIndex } from "./index"
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
      : ["print", "product"].includes(stringValue(file["purpose"])) &&
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
