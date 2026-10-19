// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

import type { Catalog, LoadedSnapshot, SnapshotClient } from "../data"
import { pinPrintings } from "./printing-snapshot"

const file = (key: string, purpose: string, region: string, counts: readonly unknown[][]) =>
  [key, { key, purpose, role: "text", region, row_counts: counts }] as const

const part = (fileKey: string, identity: string) => ({
  file: fileKey,
  table: "printing",
  value: {},
  rows: [],
  identity,
})

describe("pinPrintings", () => {
  it("merges the print bands of every owner of the cards' printings into a pinned snapshot", async () => {
    const snapshot = {
      manifestHash: "sha256:one",
      bootstrap: [part("catalog/jp/family-BP05/0", "printing:BP05:bootstrap")],
      files: new Map([
        file("print/jp/family-BP05/0", "print", "jp", [
          ["printing", "home_set", "BP05", "bootstrap", "family-BP05:band-0", 3],
        ]),
        file("print/jp/family-SP01/0", "print", "jp", [
          ["printing", "home_set", "SP01", "bootstrap", "family-SP01:band-0", 1],
        ]),
        file("print/en/family-SP01/0", "print", "en", [
          ["printing", "home_set", "SP01", "bootstrap", "family-SP01:band-0", 1],
        ]),
        file("print/jp/family-BP20/0", "print", "jp", [
          ["printing", "home_set", "BP20", "bootstrap", "family-BP20:band-0", 1],
        ]),
      ]),
    } as unknown as LoadedSnapshot
    const reads: string[] = []
    const client = {
      base: "/cdn",
      snapshot: () => snapshot,
      fragments: vi.fn((key: string) => {
        reads.push(key)
        return Promise.resolve([
          part(
            key,
            key === "print/jp/family-BP05/0" ? "printing:BP05:bootstrap" : `printing:${key}`,
          ),
        ])
      }),
    } as unknown as SnapshotClient
    const printings = {
      index: {
        printingsOf: (cardId: string) =>
          cardId === "c:base"
            ? [
                { id: "p:1", region: "jp", home_set_id: "BP05" },
                { id: "p:2", region: "jp", home_set_id: "SP01" },
                { id: "p:3", region: "en", home_set_id: "SP01" },
              ]
            : [{ id: "p:4", region: "jp", home_set_id: "BP20" }],
      },
    } as unknown as Catalog
    const pinned = await pinPrintings(client, snapshot, printings, ["c:base", "c:evolve"], ["jp"])
    expect(reads.sort()).toEqual([
      "print/jp/family-BP05/0",
      "print/jp/family-BP20/0",
      "print/jp/family-SP01/0",
    ])
    expect(pinned.snapshot()?.bootstrap.map((entry) => entry.identity)).toEqual([
      "printing:BP05:bootstrap",
      "printing:print/jp/family-SP01/0",
      "printing:print/jp/family-BP20/0",
    ])
    // The original client keeps its own snapshot; only the pinned view sees the merged rows.
    expect(client.snapshot()).toBe(snapshot)
  })
})
