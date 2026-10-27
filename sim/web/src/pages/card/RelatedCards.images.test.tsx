import { screen, waitFor, within } from "@testing-library/react"
import { afterEach, beforeEach, expect, it } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import {
  cardPrintings,
  catalogOf,
  type LoadedSnapshot,
  type SnapshotClient,
  stringValue,
} from "../../data"
import { createSnapshotClient } from "../../data/client"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { memoryCache } from "../../test-utils/cache"
import { type PrintingEntry, RelatedCards } from "./RelatedCards"

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })

function serve(): SnapshotClient {
  return createSnapshotClient("https://images.test", {
    cacheStorage: memoryCache(),
    fetch: (url) => {
      const bytes = built.files.get(url.slice("https://images.test/".length))
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
}

/** A card printed in both regions, with every printing's summary from the merged catalog. */
async function bothRegions(client: SnapshotClient, en: LoadedSnapshot) {
  const jp = client.snapshot()
  if (!jp) throw new Error("snapshot not loaded")
  const catalog = catalogOf(jp)
  // Each region's catalog lists its own printings; a card in both is in both catalogs.
  const other = catalogOf(en)
  const cardId = catalog.index.cards
    .map((card) => stringValue(card["id"]))
    .find(
      (id) =>
        catalog.index.printingsOf(id).some((row) => row["region"] === "jp") &&
        other.index.printingsOf(id).some((row) => row["region"] === "en"),
    )
  if (!cardId) throw new Error("no synthetic card printed in both regions")
  const complete = await cardPrintings(client, en, cardId, ["jp", "en"])
  const entries: PrintingEntry[] = complete.index.printingsOf(cardId).flatMap((row) => {
    const summary = complete.summary(stringValue(row["id"]))
    return summary
      ? [{ summary, rarity: undefined, link: { to: `/cards/${summary.cardNo}`, state: null } }]
      : []
  })
  return { catalog, cardId, entries }
}

beforeEach(() => {
  prefsStore.set({ ...DEFAULT_PREFS, uiLanguage: "zh-TW" })
  Object.defineProperty(HTMLImageElement.prototype, "decode", {
    configurable: true,
    value: () => Promise.resolve(),
  })
})
afterEach(() => prefsStore.set(DEFAULT_PREFS))

it("resolves the thumbnails of both regions' printings once the other edition's catalog is given", async () => {
  const client = serve()
  await client.load()
  if (!client.catalog) throw new Error("client cannot load the other catalog")
  const en = await client.catalog("en", new AbortController().signal)
  const { catalog, cardId, entries } = await bothRegions(client, en)
  const regions = new Set(entries.map((entry) => entry.summary.region))
  expect([...regions].sort()).toEqual(["en", "jp"])
  const first = entries[0]
  if (!first) throw new Error("no printings")
  await renderInRouter(
    <RelatedCards
      client={client}
      catalog={catalog}
      edition="jp"
      cardId={cardId}
      printingId={first.summary.printingId}
      revisionIds={[]}
      printings={entries}
      otherSnapshot={en}
      linkTo={(card) => ({ to: `/cards/${card.cardNo}`, state: null })}
    />,
  )
  const list = await screen.findByRole("list", { name: "其他版本" })
  expect(within(list).getAllByRole("link")).toHaveLength(entries.length)
  // Every thumbnail of either region gets its image: none is left at the placeholder.
  await waitFor(
    () => {
      expect(list.querySelectorAll("img")).toHaveLength(entries.length)
    },
    { timeout: 10_000 },
  )
  for (const entry of entries) {
    const slot = list.querySelector(`[data-image-printing="${entry.summary.printingId}"]`)
    expect(slot?.querySelector("img")).not.toBeNull()
  }
})

it("leaves the other region's thumbnails unresolved without that edition's catalog", async () => {
  const client = serve()
  await client.load()
  if (!client.catalog) throw new Error("client cannot load the other catalog")
  const en = await client.catalog("en", new AbortController().signal)
  const { catalog, cardId, entries } = await bothRegions(client, en)
  const first = entries.find((entry) => entry.summary.region === "jp")
  if (!first) throw new Error("no JP printing")
  await renderInRouter(
    <RelatedCards
      client={client}
      catalog={catalog}
      edition="jp"
      cardId={cardId}
      printingId={first.summary.printingId}
      revisionIds={[]}
      printings={entries}
      linkTo={(card) => ({ to: `/cards/${card.cardNo}`, state: null })}
    />,
  )
  const list = await screen.findByRole("list", { name: "其他版本" })
  const jpCount = entries.filter((entry) => entry.summary.region === "jp").length
  await waitFor(
    () => {
      expect(list.querySelectorAll("img")).toHaveLength(jpCount)
    },
    { timeout: 10_000 },
  )
  for (const entry of entries.filter((item) => item.summary.region === "en")) {
    const slot = list.querySelector(`[data-image-printing="${entry.summary.printingId}"]`)
    expect(slot?.querySelector("img")).toBeNull()
  }
})
