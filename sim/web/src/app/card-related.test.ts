// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

import type { LoadedSnapshot, SnapshotClient } from "../data"
import { loadCardRelated } from "./card-related"

const file = (key: string, purpose: string, counts: readonly (readonly unknown[])[]) =>
  [key, { key, purpose, role: "text", region: null, row_counts: counts }] as const

const fragment = (fileKey: string, table: string, rows: readonly Record<string, unknown>[]) => ({
  file: fileKey,
  table,
  value: {},
  rows: rows.map((row) => ({ ...row })),
  identity: `${table}:${fileKey}`,
})

function origin(replaced = false) {
  const snapshot = {
    manifestHash: "sha256:one",
    files: new Map([
      file("engine/shared/family-BP05/0", "engine", [
        ["card_engine_support", "home_set", "BP05", "bootstrap", "family-BP05:band-0", 1],
        ["card_related", "home_set", "BP05", "detail", "family-BP05:band-0", 2],
        ["card_mentioned_by", "home_set", "BP05", "detail", "family-BP05:band-0", 1],
      ]),
      file("engine/shared/family-BP05/1", "engine", [
        ["card_mentioned_by", "home_set", "BP05", "detail", "family-BP05:band-1", 1],
      ]),
      file("engine/shared/family-BP06/0", "engine", [
        ["card_related", "home_set", "BP06", "detail", "family-BP06:band-0", 1],
      ]),
      file("detail/jp/family-BP05/0", "detail", [
        ["face_revision", "home_set", "BP05", "detail", "family-BP05:band-0", 1],
      ]),
    ]),
  } as unknown as LoadedSnapshot
  const reads: string[] = []
  const client = {
    snapshot: () => (replaced ? { manifestHash: "sha256:two" } : snapshot),
    fragments: vi.fn((key: string) => {
      reads.push(key)
      return Promise.resolve(
        key === "engine/shared/family-BP05/0"
          ? [
              fragment(key, "card_engine_support", [{ card_id: "c:base" }]),
              fragment(key, "card_related", [
                { id: "r1", from_card_id: "c:base", to_card_id: "c:evolve" },
                { id: "r2", from_card_id: "c:other", to_card_id: "c:base" },
              ]),
              fragment(key, "card_mentioned_by", [
                { card_id: "c:base", region: "jp", from_card_id: "c:cross-family" },
              ]),
            ]
          : [
              fragment(key, "card_mentioned_by", [
                { card_id: "c:other", region: "en", from_card_id: "c:token" },
              ]),
            ],
      )
    }),
  } as unknown as SnapshotClient
  return { client, snapshot, reads }
}

describe("loadCardRelated", () => {
  it("reads only the engine bands of the card's family and returns every relation row in them", async () => {
    const { client, snapshot, reads } = origin()
    const rows = await loadCardRelated(client, snapshot, "BP05")
    expect(reads.sort()).toEqual(["engine/shared/family-BP05/0", "engine/shared/family-BP05/1"])
    expect(rows.related.map((row) => row["id"])).toEqual(["r1", "r2"])
    expect(rows.mentionedBy).toEqual([
      { card_id: "c:base", region: "jp", from_card_id: "c:cross-family" },
      { card_id: "c:other", region: "en", from_card_id: "c:token" },
    ])
  })

  it("fails when the snapshot was replaced while reading", async () => {
    const { client, snapshot } = origin(true)
    await expect(loadCardRelated(client, snapshot, "BP05")).rejects.toThrow("snapshot replaced")
  })
})
