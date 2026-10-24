import { describe, expect, it } from "vitest"

import { artCredits } from "./artists"
import type { LoadedSnapshot, SnapshotClient } from "./client"
import type { Fragment } from "./format-v3/reader"

const file = (tables: string[], owner: string | null) => ({
  purpose: "print",
  row_counts: tables.map((table) => [table, "home_set", owner, "detail", "seg", 1]),
})
const fragment = (table: string, rows: Record<string, unknown>[]) =>
  ({ table, rows, identity: table, value: {} }) as unknown as Fragment

describe("art credits", () => {
  it("reads the art rows of the card's home set and the shared names, for one edition only", async () => {
    const files = new Map([
      ["print/BP01", file(["art"], "BP01")],
      ["print/BP02", file(["art"], "BP02")],
      ["print/shared", file(["artist"], null)],
    ])
    const rows: Record<string, Fragment[]> = {
      "print/BP01": [
        fragment("art", [
          {
            id: "a:1",
            artists: [
              { artist_id: "y:jp", role: "illustration_jp" },
              { artist_id: "y:en", role: "illustration_en" },
            ],
          },
        ]),
      ],
      "print/shared": [
        fragment("artist", [
          { id: "y:jp", display_name: "久方綜司" },
          { id: "y:en", display_name: "Soji Hisakata" },
        ]),
      ],
    }
    const requested: string[] = []
    const client = {
      fragments: (key: string) => {
        requested.push(key)
        return Promise.resolve(rows[key] ?? [])
      },
    } as unknown as SnapshotClient
    const snapshot = { files } as unknown as LoadedSnapshot
    const credits = await artCredits(client, snapshot, "BP01", "jp", ["a:1", "a:none"])
    expect([...credits]).toEqual([["a:1", [{ id: "y:jp", name: "久方綜司" }]]])
    expect(requested.sort()).toEqual(["print/BP01", "print/shared"])
    expect([...(await artCredits(client, snapshot, "BP01", "en", ["a:1"]))]).toEqual([
      ["a:1", [{ id: "y:en", name: "Soji Hisakata" }]],
    ])
  })
})
