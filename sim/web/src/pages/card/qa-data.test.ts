import { expect, it } from "vitest"

import type { LoadedSnapshot, SnapshotClient } from "../../data"
import { loadCardQa } from "./qa-data"

const file = (table: string, region: string | null) => ({
  region,
  row_counts: [[table, "global", null, "detail", "shared:band-0", 1]],
})

function snapshotOf(windows: object[], qaCardIds: string[]) {
  const rows: Record<string, object[]> = {
    "qa/jp": [
      {
        id: "qa:2",
        region: "jp",
        official_number: "Q10",
        source_url: "https://qa/10",
        current_version_id: "qa:2:v2",
      },
      {
        id: "qa:1",
        region: "jp",
        official_number: "Q9",
        source_url: "https://qa/9",
        current_version_id: "qa:1:v1",
      },
      {
        id: "qa:3",
        region: "jp",
        official_number: "Q11",
        source_url: "https://qa/11",
        current_version_id: "qa:3:v1",
      },
    ],
    "qa/en": [
      {
        id: "qa:e",
        region: "en",
        official_number: "Q1",
        source_url: null,
        current_version_id: "qa:e:v1",
      },
    ],
    versions: [
      { id: "qa:1:v1", cards: ["c:a"] },
      { id: "qa:2:v1", cards: ["c:a"] },
      // The card was removed from the current revision, so only the old one names it.
      { id: "qa:2:v2", cards: ["c:b"] },
      { id: "qa:3:v1", cards: ["c:a", "c:b"] },
      { id: "qa:e:v1", cards: ["c:a"] },
    ],
  }
  const files = new Map([
    ["qa/jp", file("qa", "jp")],
    ["qa/en", file("qa", "en")],
    ["versions", file("qa_version", null)],
  ]) as unknown as LoadedSnapshot["files"]
  const snapshot = {
    manifestHash: "m",
    files,
    manifest: { qa_card_ids: qaCardIds, source_windows: windows },
  } as unknown as LoadedSnapshot
  const client = {
    snapshot: () => snapshot,
    fragments: (key: string) =>
      Promise.resolve([{ table: key === "versions" ? "qa_version" : "qa", rows: rows[key] ?? [] }]),
  } as unknown as SnapshotClient
  return { client, snapshot }
}

const complete = (region: string) => ({
  kind: "qa",
  region,
  scope_key: "region:*",
  from_date: "2026-01-01",
  until_date: null,
  as_of: "2026-10-24",
  state: "complete",
})

it("reports present, absent and unknown apart; an empty list is never absence", async () => {
  const { client, snapshot } = snapshotOf([complete("jp")], ["c:a", "c:b"])
  const today = "2026-10-24"
  // Current versions only, this edition only, in official number order.
  const present = await loadCardQa(client, snapshot, "c:a", "jp", today)
  expect(present.coverage === "present" && present.entries.map((e) => e.number)).toEqual([
    "Q9",
    "Q11",
  ])
  // A card with no current association is absent only inside the complete jp window.
  expect((await loadCardQa(client, snapshot, "c:c", "jp", today)).coverage).toBe("absent")
  // The en edition has Q&A for c:a but no window covers it for other cards: unknown, not absent.
  expect((await loadCardQa(client, snapshot, "c:c", "en", today)).coverage).toBe("unknown")
  expect((await loadCardQa(client, snapshot, "c:a", "en", today)).coverage).toBe("present")
  // A window that ended before today or is partial proves nothing.
  const stale = snapshotOf([{ ...complete("jp"), until_date: "2026-06-01" }], [])
  expect((await loadCardQa(stale.client, stale.snapshot, "c:c", "jp", today)).coverage).toBe(
    "unknown",
  )
  const partial = snapshotOf([{ ...complete("jp"), state: "partial" }], [])
  expect((await loadCardQa(partial.client, partial.snapshot, "c:c", "jp", today)).coverage).toBe(
    "unknown",
  )
  for (const windows of [
    [{ ...complete("jp"), as_of: "2026-10-23" }],
    [{ ...complete("jp"), until_date: today }],
    [{ ...complete("jp"), from_date: "2026-10-25" }],
    [{ ...complete("jp"), scope_key: "product:bp01" }],
    [{ ...complete("jp"), kind: "errata" }],
    [complete("jp"), { ...complete("jp"), state: "partial" }],
    [complete("jp"), { ...complete("jp"), scope_key: "product:bp01", state: "partial" }],
  ]) {
    const uncertain = snapshotOf(windows, [])
    expect(
      (await loadCardQa(uncertain.client, uncertain.snapshot, "c:c", "jp", today)).coverage,
    ).toBe("unknown")
  }
  const finite = snapshotOf([{ ...complete("jp"), until_date: "2026-10-25" }], [])
  expect((await loadCardQa(finite.client, finite.snapshot, "c:c", "jp", today)).coverage).toBe(
    "absent",
  )
})
