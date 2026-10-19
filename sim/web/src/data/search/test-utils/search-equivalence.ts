import { createHash } from "node:crypto"

import { DEFAULT_QUERY, type QueryState } from "../../../domain/query/model"
import type { Region } from "../../../domain/search"
import { canonicalText, type JsonValue } from "../../format-v3/json"
import type { SearchIndex } from "../index"

type SearchApi = Pick<
  SearchIndex,
  | "search"
  | "count"
  | "suggest"
  | "recent"
  | "imageTargets"
  | "cardForFace"
  | "neighbours"
  | "options"
  | "releases"
>

/** Failures report hashes so a mismatch never prints card wording. */
export function compareSearchIndexes(actual: SearchApi, expected: SearchApi, edition: Region) {
  let comparisons = 0
  const digest = createHash("sha256")
  const hash = (value: unknown) =>
    createHash("sha256")
      .update(canonicalText(JSON.parse(JSON.stringify([value])) as JsonValue))
      .digest("hex")
  const equal = (kind: string, first: unknown, second: unknown) => {
    comparisons++
    const a = hash(first),
      b = hash(second)
    if (a !== b)
      throw new Error(`Search equivalence failed: ${kind} #${String(comparisons)} (${a} != ${b})`)
    digest.update(b)
  }
  const query = (state: QueryState, offset = 0) => ({ edition, state, offset, limit: 100 })
  equal("options", actual.options, expected.options)
  equal("releases", actual.releases, expected.releases)
  let printings = 0
  const texts = ["", "BP01", "1", "no-match-synthetic-query"]
  for (const unit of ["card", "printing", "art"] as const) {
    const state = { ...DEFAULT_QUERY, unit }
    let total = 0
    for (let offset = 0; offset === 0 || offset < total; offset += 100) {
      const want = expected.search(query(state, offset))
      total = want.total
      equal("full page", actual.search(query(state, offset)), want)
      if (unit !== "printing") continue
      printings = total
      for (const item of want.items.slice(0, 3)) {
        if (texts.length < 10 && item.summary) texts.push(item.summary.name.original.text)
      }
      const ids = want.items.map((item) => item.printingId)
      equal("image targets", actual.imageTargets(ids), expected.imageTargets(ids))
      for (let start = 0; start < ids.length; start += 20)
        equal(
          "recent",
          actual.recent(ids.slice(start, start + 20)),
          expected.recent(ids.slice(start, start + 20)),
        )
      for (const target of expected.imageTargets(ids))
        equal("face owner", actual.cardForFace(target.faceId), expected.cardForFace(target.faceId))
    }
  }
  const changes: Partial<QueryState>[] = [
    {},
    { altArtOnly: true },
    { classes: ["synthetic-missing-class"] },
    ...expected.options.classes.map((option) => ({ classes: [option.code] })),
    ...expected.options.sets.map((option) => ({ sets: [option.code] })),
    ...Object.keys(expected.options.typeCounts).map((type) => ({ types: [type] })),
    ...Object.keys(expected.options.locationCounts).map((token) => ({ locations: [token] })),
    { locations: ["main", "token"] },
    ...Object.keys(expected.options.traitCounts).map((trait) => ({ traits: [trait] })),
    { traits: Object.keys(expected.options.traitCounts).slice(0, 2) },
    { traits: ["synthetic-missing-trait"] },
    ...Object.keys(expected.options.rarityCounts).map((rarity) => ({ rarities: [rarity] })),
    ...texts.map((text) => ({ text })),
  ]
  for (const stat of ["cost", "attack", "defense"] as const)
    for (const values of [[0], [1], [9], [10], [0, 10]]) changes.push({ [stat]: values })
  for (const sort of ["no", "name", "cost", "atk", "def", "date"] as const)
    changes.push({ sort }, { sort, order: "desc" })
  for (const set of expected.options.sets.slice(0, 3))
    for (const rarity of Object.keys(expected.options.rarityCounts))
      changes.push({ sets: [set.code], rarities: [rarity], altArtOnly: true })
  for (const unit of ["card", "printing", "art"] as const) {
    for (const change of changes) {
      const state = { ...DEFAULT_QUERY, unit, ...change }
      equal("filtered page", actual.search(query(state)), expected.search(query(state)))
      equal("filtered count", actual.count(query(state)), expected.count(query(state)))
    }
    const state = { ...DEFAULT_QUERY, unit }
    const first = expected.search(query(state)).items[0]
    if (first)
      equal(
        "neighbours",
        actual.neighbours(query(state), first.key, 60),
        expected.neighbours(query(state), first.key, 60),
      )
  }
  for (const text of texts) equal("suggest", actual.suggest(text, 20), expected.suggest(text, 20))
  equal(
    "missing recent",
    actual.recent(["synthetic-unknown-printing"]),
    expected.recent(["synthetic-unknown-printing"]),
  )
  return { edition, printings, comparisons, digest: digest.digest("hex") }
}
