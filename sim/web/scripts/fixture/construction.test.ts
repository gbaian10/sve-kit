import { expect, it } from "vitest"

import { arrayValue, type JsonObject, objectValue, parseJson } from "../../src/data/format-v3/json"
import { constructionIndexFiles } from "./construction"

it("keeps fixture coverage within known dates and excludes partial or announced gaps", () => {
  const profile = "rules-profile:jp:standard"
  const rows: Readonly<Record<string, readonly JsonObject[]>> = {
    rules_profile: [{ id: profile, region: "jp", format_code: "standard" }],
    restriction: [
      {
        profile_id: profile,
        state: "announced",
        effective_from: "2026-11-04",
        effective_until: "2026-11-05",
      },
    ],
  }
  const manifest: JsonObject = {
    regions: ["jp"],
    data_version: "preview-20261105T000000Z-0001",
    data_as_of: "2026-11-05",
    restriction_coverage: [
      { profile_id: profile, from_date: "2026-11-01", until_date: null, state: "complete" },
      { profile_id: profile, from_date: "2026-11-02", until_date: "2026-11-03", state: "partial" },
    ],
  }
  const files = constructionIndexFiles(
    (table) => rows[table] ?? [],
    manifest,
    `sha256:${"ab".repeat(32)}`,
  )
  const raw = [...files.values()][0]
  expect(raw).toBeDefined()
  const index = objectValue(parseJson(raw ?? ""))
  const standard = objectValue(objectValue(index["formats"])["standard"])
  expect(arrayValue(standard["coverage"])).toEqual([
    ["2026-11-01", "2026-11-02"],
    ["2026-11-03", "2026-11-04"],
    ["2026-11-05", "2026-11-06"],
  ])
})
