import { expect, it, vi } from "vitest"

import type { LoadedSnapshot, SnapshotClient } from "../../data"
import { loadCardQa } from "./qa-data"

const file = (key: string, table: string, region: string | null, band: number) => ({
  key,
  region,
  purpose: "detail",
  segment: `shared:band-${String(band)}`,
  row_counts: [[table, "global", null, "detail", `shared:band-${String(band)}`, 1]],
})

function snapshotOf(windows: object[], qaCardIds: string[], index?: object) {
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
      { id: "qa:1:v1", qa_id: "qa:1", cards: ["c:a", "c:b"] },
      { id: "qa:2:v1", qa_id: "qa:2", cards: ["c:a"] },
      // The card was removed from the current revision, so only the old one names it.
      { id: "qa:2:v2", qa_id: "qa:2", cards: ["c:b"] },
      { id: "qa:3:v1", qa_id: "qa:3", cards: ["c:a", "c:b"] },
      { id: "qa:e:v1", qa_id: "qa:e", cards: ["c:a"] },
    ],
  }
  // Independent SHA-256 bucket vectors for the fixture IDs, with the fixed wire widths.
  const versionBands: Record<string, number> = {
    "qa:1:v1": 10,
    "qa:2:v1": 11,
    "qa:2:v2": 11,
    "qa:3:v1": 7,
    "qa:e:v1": 6,
  }
  const questionBands: Record<string, number> = { "qa:1": 7, "qa:2": 7, "qa:3": 1, "qa:e": 1 }
  const files = new Map<string, object>()
  const fragments: Record<string, object[]> = {}
  for (let band = 0; band < 16; band++) {
    const key = `versions/${String(band)}`
    files.set(key, file(key, "qa_version", null, band))
    fragments[key] = [
      {
        table: "qa_version",
        rows: rows["versions"]?.filter((row) => versionBands[(row as { id: string }).id] === band),
      },
    ]
  }
  for (const region of ["jp", "en"])
    for (let band = 0; band < 8; band++) {
      const key = `qa/${region}/${String(band)}`
      files.set(key, file(key, "qa", region, band))
      fragments[key] = [
        {
          table: "qa",
          rows: rows[`qa/${region}`]?.filter(
            (row) => questionBands[(row as { id: string }).id] === band,
          ),
        },
      ]
    }
  const requests: string[] = []
  const snapshot = {
    manifestHash: "m",
    files: files as LoadedSnapshot["files"],
    manifest: {
      qa_card_ids: qaCardIds,
      source_windows: windows,
      ...(index ? { qa_card_index: index } : {}),
    },
  } as unknown as LoadedSnapshot
  const client = {
    snapshot: vi.fn(() => snapshot),
    fragments: vi.fn((key: string) => {
      requests.push(key)
      return Promise.resolve(fragments[key] ?? [])
    }),
  } as unknown as SnapshotClient
  return { client, snapshot, requests }
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

const index = {
  jp: { "c:a": ["qa:1:v1", "qa:3:v1"], "c:b": ["qa:1:v1", "qa:2:v2", "qa:3:v1"] },
  en: { "c:a": ["qa:e:v1"] },
}

it("uses the edition's index to fetch only the card's current versions and questions", async () => {
  const { client, snapshot, requests } = snapshotOf(
    [complete("jp"), complete("en")],
    ["c:a", "c:b"],
    index,
  )
  const result = await loadCardQa(client, snapshot, "c:a", "jp", "2026-10-24")
  expect(result).toMatchObject({
    coverage: "present",
    entries: [
      { qaId: "qa:1", versionId: "qa:1:v1", number: "Q9" },
      { qaId: "qa:3", versionId: "qa:3:v1", number: "Q11" },
    ],
  })
  expect(new Set(requests)).toEqual(new Set(["versions/10", "versions/7", "qa/jp/7", "qa/jp/1"]))
  requests.length = 0
  const english = await loadCardQa(client, snapshot, "c:a", "en", "2026-10-24")
  expect(english).toMatchObject({ coverage: "present", entries: [{ qaId: "qa:e", number: "Q1" }] })
  expect(requests).toEqual(["versions/6", "qa/en/1"])
})

it("deduplicates files for multiple questions in the same shard", async () => {
  const { client, snapshot, requests } = snapshotOf([complete("jp")], ["c:a", "c:b"], index)
  await loadCardQa(client, snapshot, "c:b", "jp", "2026-10-24")
  expect(requests).toHaveLength(new Set(requests).size)
  expect(requests).toHaveLength(5)
  expect(requests.filter((key) => key === "qa/jp/7")).toHaveLength(1)
})

it("proves absence with no file reads only inside a complete source window", async () => {
  const { client, snapshot, requests } = snapshotOf([complete("jp")], ["c:a", "c:b"], index)
  expect(await loadCardQa(client, snapshot, "c:c", "jp", "2026-10-24")).toEqual({
    coverage: "absent",
  })
  expect(requests).toEqual([])
})

it.each([undefined, index])(
  "keeps the scan path when an index or complete window is unavailable: %j",
  async (value) => {
    const { client, snapshot, requests } = snapshotOf(value ? [] : [complete("jp")], ["c:a"], value)
    expect(await loadCardQa(client, snapshot, "c:a", "jp", "2026-10-24")).toMatchObject({
      coverage: "present",
    })
    expect(requests).toHaveLength(24)
  },
)

it("treats an earlier overlapping partial window as incomplete and does not combine complete windows", async () => {
  for (const windows of [
    [
      complete("jp"),
      {
        ...complete("jp"),
        state: "partial",
        scope_key: "product:bp01",
        until_date: "2026-02-01",
        as_of: "2026-01-31",
      },
    ],
    [
      { ...complete("jp"), as_of: "2026-10-23" },
      { ...complete("jp"), from_date: "2026-10-25" },
    ],
  ]) {
    const { client, snapshot, requests } = snapshotOf(windows, ["c:a"], index)
    expect(await loadCardQa(client, snapshot, "c:c", "jp", "2026-10-24")).toEqual({
      coverage: "unknown",
    })
    await loadCardQa(client, snapshot, "c:a", "jp", "2026-10-24")
    expect(requests).toHaveLength(24)
  }
})

it.each([
  {
    name: "a partial window ending exactly before the complete window",
    windows: [
      complete("jp"),
      {
        ...complete("jp"),
        state: "partial",
        from_date: "2025-01-01",
        until_date: "2026-01-01",
        as_of: "2025-12-31",
      },
    ],
    reads: 4,
    absence: "absent",
  },
  {
    name: "a partial window after the complete window's evidence",
    windows: [
      complete("jp"),
      { ...complete("jp"), state: "partial", from_date: "2026-10-25", as_of: "2026-10-30" },
    ],
    reads: 4,
    absence: "absent",
  },
  {
    name: "the exclusive until date",
    windows: [{ ...complete("jp"), until_date: "2026-10-24" }],
    reads: 24,
    absence: "unknown",
  },
  {
    name: "a query beyond as_of",
    windows: [{ ...complete("jp"), as_of: "2026-10-23" }],
    reads: 24,
    absence: "unknown",
  },
])("keeps indexed window semantics at $name", async ({ windows, reads, absence }) => {
  const { client, snapshot, requests } = snapshotOf(windows, ["c:a"], index)
  expect(await loadCardQa(client, snapshot, "c:a", "jp", "2026-10-24")).toMatchObject({
    coverage: "present",
  })
  expect(requests).toHaveLength(reads)
  expect(await loadCardQa(client, snapshot, "c:c", "jp", "2026-10-24")).toEqual({
    coverage: absence,
  })
})

it("does not turn a missing target, download failure or snapshot replacement into absence", async () => {
  const broken = snapshotOf([complete("jp")], ["c:a"], { jp: { "c:a": ["nonexistent"] } })
  await expect(
    loadCardQa(broken.client, broken.snapshot, "c:a", "jp", "2026-10-24"),
  ).rejects.toThrow()
  const failing = snapshotOf([complete("jp")], ["c:a"], index)
  vi.mocked(failing.client.fragments).mockRejectedValue(new Error("download failed"))
  await expect(
    loadCardQa(failing.client, failing.snapshot, "c:a", "jp", "2026-10-24"),
  ).rejects.toThrow("download failed")
  const replaced = snapshotOf([complete("jp")], ["c:a"], index)
  vi.mocked(replaced.client.snapshot).mockReturnValue({ ...replaced.snapshot, manifestHash: "new" })
  await expect(
    loadCardQa(replaced.client, replaced.snapshot, "c:c", "jp", "2026-10-24"),
  ).rejects.toThrow("snapshot replaced")
})
