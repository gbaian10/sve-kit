import { beforeAll, expect, it } from "vitest"

import { buildSnapshot, type BuiltSnapshot } from "../../scripts/fixture/build"
import { memoryCache } from "../test-utils/cache"
import { createSnapshotClient } from "./client"
import { loadCardErrata } from "./errata"
import { arrayValue, objectValue } from "./format-v3/json"

let built: BuiltSnapshot
beforeAll(async () => {
  built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })
})

async function source() {
  const client = createSnapshotClient("https://cdn.test", {
    region: "jp",
    cacheStorage: memoryCache(),
    fetch: (url) => {
      const bytes = built.files.get(url.slice("https://cdn.test/".length))
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  await client.load()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const fragments = (
    await Promise.all(
      [...snapshot.files]
        .filter(([, file]) =>
          arrayValue(file["row_counts"]).some((count) => arrayValue(count)[0] === "errata"),
        )
        .map(([key]) => client.fragments(key)),
    )
  ).flat()
  const notice = fragments
    .flatMap((fragment) => (fragment.table === "errata" ? fragment.rows : []))
    .find((row) => row["id"] === "errata:bp01-020")
  if (!notice) throw new Error("missing notice")
  const version = objectValue(arrayValue(notice["versions"])[0])
  const change = objectValue(arrayValue(version["changes"])[0])
  const load = () =>
    loadCardErrata(client, snapshot, "c:bp01-020", "p:bp01-020", "f:bp01-020", "jp")
  return { client, snapshot, version, change, load }
}

it("keeps a listed printing unknown and exposes its card number", async () => {
  const { load } = await source()
  const [notice] = await load()
  expect(notice?.printed.state).toBe("unknown")
  expect(notice?.printed.before).toBeDefined()
  expect(notice?.printings).toEqual([{ cardNo: "BP01-020", confirmed: false }])
})

it("reuses the resolved data for repeated interactions in the same snapshot", async () => {
  const { load } = await source()
  const first = load()
  expect(load()).toBe(first)
  await first
  expect(load()).toBe(first)
})

it("does not return a cached result for a replaced snapshot", async () => {
  const { client, snapshot, load } = await source()
  await load()
  await expect(
    loadCardErrata(
      client,
      { ...snapshot, manifestHash: "replaced" },
      "c:bp01-020",
      "p:bp01-020",
      "f:bp01-020",
      "jp",
    ),
  ).rejects.toThrow("snapshot replaced")
})

it("requires the selected printing's confirmed scope for derived_from_errata", async () => {
  const { version, load } = await source()
  objectValue(arrayValue(version["printings"])[0])["scope"] = "confirmed_applies"
  expect((await load())[0]?.printed.state).toBe("derived_from_errata")
})

it("preserves excerpts when no exact corrected passage matches", async () => {
  const { change, load } = await source()
  change["after"] = "Unmatched synthetic passage."
  const [notice] = await load()
  expect(notice?.printed).toEqual({ state: "unknown", reason: "fragment_missing" })
  expect(notice?.changes[0]?.["after"]).toBe("Unmatched synthetic passage.")
})

it("refuses to infer an order for multiple announcement versions", async () => {
  const { version, load, client, snapshot } = await source()
  for (const [key, file] of snapshot.files) {
    if (!arrayValue(file["row_counts"]).some((count) => arrayValue(count)[0] === "errata")) continue
    for (const fragment of await client.fragments(key)) {
      if (fragment.table !== "errata") continue
      const notice = fragment.rows.find((row) => row["id"] === "errata:bp01-020")
      if (notice) notice["versions"] = [version, { ...version, id: "another-version", revision: 2 }]
    }
  }
  const notices = await load()
  expect(notices).toHaveLength(2)
  expect(notices.every((notice) => notice.printed.reason === "multiple_notices")).toBe(true)
})
