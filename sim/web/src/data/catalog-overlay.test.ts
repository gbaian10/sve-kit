// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { memoryCache } from "../test-utils/cache"
import { catalogOf, createCatalog, preparePrintingOverlay } from "./catalog"
import { createSnapshotClient, type LoadedSnapshot } from "./client"
import type { Row } from "./format-v3/decode"
import { canonicalText, stringValue } from "./format-v3/json"
import type { Fragment } from "./format-v3/reader"

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })
async function fixture() {
  const client = createSnapshotClient("/cdn", {
    cacheStorage: memoryCache(),
    fetch: (url) => {
      const bytes = built.files.get(url.slice("/cdn/".length))
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  await client.load()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("fixture did not load")
  const present = new Set(snapshot.bootstrap.map((part) => part.identity))
  const parts = (
    await Promise.all(
      [...snapshot.files]
        .filter(([, file]) => file["purpose"] === "print" && file["region"] === "jp")
        .map(([key]) => client.fragments(key)),
    )
  )
    .flat()
    .filter((part) => !present.has(part.identity))
  return { client, snapshot, parts }
}
function append(snapshot: LoadedSnapshot, parts: readonly Fragment[]): LoadedSnapshot {
  return { ...snapshot, bootstrap: [...snapshot.bootstrap, ...parts] }
}
function byId(rows: readonly Row[]): Row[] {
  return [...rows].sort((a, b) => stringValue(a["id"]).localeCompare(stringValue(b["id"])))
}
function overlay(snapshot: LoadedSnapshot, parts: readonly Fragment[]) {
  const next = append(snapshot, parts)
  preparePrintingOverlay(next, snapshot, parts)
  return { snapshot: next, catalog: catalogOf(next) }
}

// Keep malformed fragments internally consistent so scope checks cannot hide identity errors.
function identity(part: Fragment): Fragment {
  return {
    ...part,
    identity: canonicalText([
      part.file,
      part.table,
      part.value["owner"] ?? null,
      part.value["region"] ?? null,
      part.value["purpose"] ?? null,
      part.value["segment"] ?? null,
      part.value["partition"] ?? null,
    ]),
  }
}

describe("printing catalog overlays", () => {
  it("matches a complete rebuild for broad lookups, including repeated known and missing IDs", async () => {
    const { snapshot, parts } = await fixture()
    const base = catalogOf(snapshot)
    const { snapshot: next, catalog } = overlay(snapshot, parts)
    const rebuilt = createCatalog(next)
    for (let pass = 0; pass < 2; pass++) {
      for (const card of rebuilt.index.cards) {
        const cardId = stringValue(card["id"])
        expect(byId(catalog.index.printingsOf(cardId))).toEqual(
          byId(rebuilt.index.printingsOf(cardId)),
        )
        for (const row of rebuilt.index.printingsOf(cardId)) {
          const id = stringValue(row["id"])
          expect(catalog.index.printing(id)).toEqual(row)
          expect(catalog.index.printingByCardNo("jp", stringValue(row["card_no"]))).toEqual(row)
          expect(catalog.summary(id)).toEqual(rebuilt.summary(id))
        }
      }
      expect(catalog.index.printing("p:missing")).toBeUndefined()
      expect(catalog.summary("p:missing")).toBeUndefined()
      expect(catalog.index.printingByCardNo("jp", "MISSING-001")).toBeUndefined()
    }
    expect(catalog.entries).toBe(base.entries)
    expect(catalog.index.cards).toBe(base.index.cards)
    expect(base.summary("p:bp01-002-alt")?.intId).toBeUndefined()
    expect(catalog.summary("p:bp01-002-alt")?.intId).toBe(1102)
  })

  it.each(["card_id", "region", "card_no"])(
    "rejects a completed row with a different %s",
    async (field) => {
      const { snapshot, parts } = await fixture()
      const changed = parts.map((part) =>
        part.table !== "printing"
          ? part
          : {
              ...part,
              rows: part.rows.map((row) => ({ ...row, [field]: "wrong" })),
            },
      )
      const { catalog } = overlay(snapshot, changed)
      expect(() => catalog.index.printing("p:bp01-002-alt")).toThrow(/catalog identity/u)
    },
  )

  it("rejects a fragment whose owner differs from its manifest", async () => {
    const { snapshot, parts } = await fixture()
    const changed = parts.map((part) =>
      identity({
        ...part,
        value: { ...part.value, owner: { kind: "home_set", id: "set:wrong" } },
      }),
    )
    expect(() => overlay(snapshot, changed)).toThrow(/print bootstrap/u)
  })

  it.each(["manifest", "region"])("rejects a changed snapshot %s scope", async (scope) => {
    const { snapshot, parts } = await fixture()
    const next = append(snapshot, parts)
    expect(() => {
      preparePrintingOverlay(
        {
          ...next,
          ...(scope === "manifest" ? { manifestHash: "wrong" } : { region: "en" as const }),
        },
        snapshot,
        parts,
      )
    }).toThrow(/snapshot scope/u)
  })

  it.each(["file_sha256", "region", "purpose", "partition", "base"])(
    "rejects out-of-scope fragment %s",
    async (field) => {
      const { snapshot, parts } = await fixture()
      const changed = parts.map((part) =>
        identity({
          ...part,
          value: { ...part.value, [field]: "wrong" },
        }),
      )
      expect(() => overlay(snapshot, changed)).toThrow(/print bootstrap/u)
    },
  )

  it("rejects shared print fragments, which carry no regional printing rows", async () => {
    const { client, snapshot } = await fixture()
    const shared = (
      await Promise.all(
        [...snapshot.files]
          .filter(([, file]) => file["purpose"] === "print" && file["region"] === null)
          .map(([key]) => client.fragments(key)),
      )
    ).flat()
    expect(shared.length).toBeGreaterThan(0)
    for (const part of shared) expect(() => overlay(snapshot, [part])).toThrow(/print bootstrap/u)
  })

  it("rejects revision/history additions and unreported bootstrap replacements", async () => {
    const { snapshot, parts } = await fixture()
    const revisions = parts.map((part) => identity({ ...part, table: "face_revision" }))
    expect(() => overlay(snapshot, revisions)).toThrow(/print bootstrap/u)
    expect(() => {
      preparePrintingOverlay({ ...snapshot, bootstrap: [] }, snapshot, [])
    }).toThrow(/only append/u)
  })

  it("retains the first overlay's fields through a second owner and an empty overlay", async () => {
    const { snapshot, parts } = await fixture()
    const firstParts = parts.filter(
      (part) => part.value["owner"] && canonicalText(part.value["owner"]).includes("set:bp01"),
    )
    const secondParts = parts.filter((part) => !firstParts.includes(part))
    expect(firstParts.length).toBeGreaterThan(0)
    expect(secondParts.length).toBeGreaterThan(0)
    const first = overlay(snapshot, firstParts)
    const second = overlay(first.snapshot, secondParts)
    const third = overlay(second.snapshot, [])
    const rebuilt = createCatalog(second.snapshot)
    for (const card of rebuilt.index.cards)
      for (const row of rebuilt.index.printingsOf(stringValue(card["id"]))) {
        const id = stringValue(row["id"])
        expect(third.catalog.index.printing(id)).toEqual(row)
        expect(third.catalog.summary(id)).toEqual(rebuilt.summary(id))
      }
    expect(first.catalog.summary("p:bp01-002-alt")?.intId).toBe(1102)
    expect(second.catalog.summary("p:bp01-002-alt")).toEqual(
      first.catalog.summary("p:bp01-002-alt"),
    )
    expect(third.snapshot).not.toBe(second.snapshot)
    expect(third.catalog).not.toBe(second.catalog)
  })
})
