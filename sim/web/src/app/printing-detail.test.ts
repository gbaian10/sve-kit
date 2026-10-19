// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

import type { LoadedSnapshot, SnapshotClient } from "../data"
import { loadPrintedTextState } from "./printing-detail"

const file = (
  key: string,
  purpose: string,
  region: string,
  counts: readonly (readonly unknown[])[],
) => [key, { key, purpose, role: "text", region, row_counts: counts }] as const

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
      file("print-detail/jp/family-SCS01/0", "print-detail", "jp", [
        ["printing", "home_set", "SCS01", "detail", "print:family-SCS01:band-0", 2],
      ]),
      file("print-detail/en/family-SCS01/0", "print-detail", "en", [
        ["printing", "home_set", "SCS01", "detail", "print:family-SCS01:band-0", 1],
      ]),
      file("print-detail/jp/family-BP05/0", "print-detail", "jp", [
        ["printing", "home_set", "BP05", "detail", "print:family-BP05:band-0", 1],
      ]),
      file("print/jp/family-SCS01/0", "print", "jp", [
        ["printing", "home_set", "SCS01", "bootstrap", "family-SCS01:band-0", 2],
      ]),
    ]),
  } as unknown as LoadedSnapshot
  const reads: string[] = []
  const client = {
    snapshot: () => (replaced ? { manifestHash: "sha256:two" } : snapshot),
    fragments: vi.fn((key: string) => {
      reads.push(key)
      return Promise.resolve([
        fragment(key, "printing", [
          {
            id: "p:blank",
            // No printed text units at all: only the state says the page printed nothing.
            faces: [
              { face_id: "f:blank", printed_effect_unit_id: null, printed_text_state: "unknown" },
              { face_id: "f:back", printed_effect_unit_id: null, printed_text_state: "derived" },
            ],
          },
          { id: "p:other", faces: [{ face_id: "f:other", printed_text_state: "confirmed" }] },
        ]),
      ])
    }),
  } as unknown as SnapshotClient
  return { client, snapshot, reads }
}

describe("loadPrintedTextState", () => {
  it("reads the face's state from the region's print-detail band of the home set", async () => {
    const { client, snapshot, reads } = origin()
    await expect(
      loadPrintedTextState(client, snapshot, "p:blank", "f:blank", "SCS01", "jp"),
    ).resolves.toBe("unknown")
    expect(reads).toEqual(["print-detail/jp/family-SCS01/0"])
    await expect(
      loadPrintedTextState(client, snapshot, "p:blank", "f:back", "SCS01", "jp"),
    ).resolves.toBe("derived")
    await expect(
      loadPrintedTextState(client, snapshot, "p:missing", "f:blank", "SCS01", "jp"),
    ).resolves.toBeUndefined()
  })

  it("knows nothing without a snapshot", async () => {
    const { client } = origin()
    await expect(
      loadPrintedTextState(client, null, "p:blank", "f:blank", "SCS01", "jp"),
    ).resolves.toBeUndefined()
  })

  it("fails when the snapshot was replaced while reading", async () => {
    const { client, snapshot } = origin(true)
    await expect(
      loadPrintedTextState(client, snapshot, "p:blank", "f:blank", "SCS01", "jp"),
    ).rejects.toThrow("snapshot replaced")
  })
})
