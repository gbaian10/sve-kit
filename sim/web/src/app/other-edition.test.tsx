import assert from "node:assert/strict"

import { act, renderHook, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { CARDS } from "../../scripts/fixture/cards"
import { AccountMenuPanel } from "../components/nav/AccountMenu"
import {
  arrayValue,
  cardPrintings,
  catalogOf,
  createSnapshotClient,
  editionPrinting,
  locateCard,
  objectValue,
  type SnapshotClient,
  stringValue,
} from "../data"
import { DEFAULT_QUERY } from "../domain/query/model"
import { DEFAULT_PREFS, prefsStore } from "../settings"
import { renderInRouter } from "../test-utils"
import { memoryCache } from "../test-utils/cache"
import { useOtherEdition } from "./other-edition"

const state = vi.hoisted(() => ({ client: null as SnapshotClient | null }))
vi.mock("../data", async (original) => ({
  ...(await original<typeof import("../data")>()),
  snapshotClient: () => state.client,
}))
const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })
const enPaths = new Set(
  arrayValue(built.manifest["files"])
    .map((raw) => objectValue(raw))
    .filter((file) => file["region"] === "en")
    .map((file) => stringValue(file["path"])),
)
function harness() {
  const requests: string[] = []
  let release: (() => void) | undefined
  let fail = false
  let hold: Promise<void> | undefined
  const client = createSnapshotClient("https://edition.test", {
    cacheStorage: memoryCache(),
    fetch: async (url, init) => {
      const path = url.slice("https://edition.test/".length)
      requests.push(path)
      if (enPaths.has(path)) {
        await hold
        if (init?.signal?.aborted) throw new Error("cancelled")
        if (fail) return new Response(null, { status: 503 })
      }
      const bytes = built.files.get(path)
      return bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 })
    },
  })
  state.client = client
  return {
    client,
    requests,
    block: () => {
      hold = new Promise((resolve) => {
        release = resolve
      })
    },
    release: () => {
      release?.()
      hold = undefined
    },
    fail: (next: boolean) => {
      fail = next
    },
  }
}
afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

it("requests no other-region data when off, waits for selected readiness, and keeps search regional", async () => {
  const h = harness()
  await h.client.load()
  const selected = h.client.snapshot()
  assert(selected)
  const { result, rerender } = renderHook(
    ({ enabled, ready }) => useOtherEdition(h.client, "jp", enabled, ready, selected.manifestHash),
    { initialProps: { enabled: false, ready: true } },
  )
  await act(async () => {
    await Promise.resolve()
  })
  expect(h.requests.filter((path) => enPaths.has(path))).toEqual([])
  rerender({ enabled: true, ready: false })
  await act(async () => {
    await Promise.resolve()
  })
  expect(h.requests.filter((path) => enPaths.has(path))).toEqual([])
  h.block()
  rerender({ enabled: true, ready: true })
  await waitFor(() => {
    expect(h.requests.some((path) => enPaths.has(path))).toBe(true)
  })
  expect(h.client.status().state).toBe("ready")
  expect(h.client.snapshot()).toBe(selected)
  expect(result.current.catalog).toBeUndefined()
  act(() => {
    h.release()
  })
  await waitFor(() => {
    expect(result.current.catalog).toBeDefined()
  })
  expect(h.client.snapshot()).toBe(selected)
  expect(
    catalogOf(selected)
      .results(DEFAULT_QUERY, "jp")
      .every((item) => catalogOf(selected).index.printing(item.printingId)?.["region"] === "jp"),
  ).toBe(true)
  expect(
    result.current.catalog?.index.printingsOf("c:bp01-002").some((row) => row["region"] === "en"),
  ).toBe(true)
})

it("keeps a downloaded comparison while the selected edition rechecks its data", async () => {
  const h = harness()
  const { result, rerender } = renderHook(
    ({ ready }) => useOtherEdition(h.client, "jp", true, ready, "sha256:selected"),
    { initialProps: { ready: true } },
  )
  await waitFor(() => {
    expect(result.current.catalog).toBeDefined()
  })
  const requested = h.requests.length
  rerender({ ready: false })
  rerender({ ready: true })
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20))
  })
  expect(h.requests).toHaveLength(requested)
  expect(result.current.catalog).toBeDefined()
})

it("ignores a late comparison after disabling and retries a failed download", async () => {
  const h = harness()
  await h.client.load()
  h.block()
  const { result, rerender } = renderHook(
    ({ enabled }) => useOtherEdition(h.client, "jp", enabled, true),
    { initialProps: { enabled: true } },
  )
  await waitFor(() => {
    expect(h.requests.some((path) => enPaths.has(path))).toBe(true)
  })
  rerender({ enabled: false })
  act(() => {
    h.release()
  })
  expect(result.current.catalog).toBeUndefined()
  h.fail(true)
  rerender({ enabled: true })
  await waitFor(() => {
    expect(result.current.error).toBeDefined()
  })
  expect(h.client.status().state).toBe("ready")
  h.fail(false)
  act(() => {
    result.current.retry()
  })
  await waitFor(() => {
    expect(result.current.catalog).toBeDefined()
  })
})

it("selects a shared art by identity, otherwise the region default, and detects a missing region", async () => {
  const h = harness()
  const source = await locateCard(h.client, { namespace: "official", key: "BP01EN-002" })
  assert(h.client.catalog)
  const snapshot = await h.client.catalog("jp", new AbortController().signal)
  const target = await cardPrintings(h.client, snapshot, source.printing.cardId)
  const row = source.catalog.index.printing(source.printing.id)
  assert(row)
  const candidates = target.index
    .printingsOf(source.printing.cardId)
    .filter((row) => row["region"] === "jp")
  expect(candidates.length).toBeGreaterThan(1)
  const alternate = candidates.find((row) => row["id"] === "p:bp01-002-alt")
  assert(alternate?.["faces"])
  const shared = { ...row, faces: alternate["faces"] }
  expect(editionPrinting(shared, target, "jp")?.["id"]).toBe(alternate["id"])
  const defaultId = arrayValue(target.index.card(source.printing.cardId)?.["regions"])
    .map((raw) => objectValue(raw))
    .find((view) => view["region"] === "jp")?.["default_printing_id"]
  expect(editionPrinting(row, target, "jp")?.["id"]).toBe(defaultId)
  expect(editionPrinting({ ...row, card_id: "c:uncollected" }, target, "en")).toBeUndefined()
})

it("keeps the card when switching with comparison off, shows loading, and recovers from failure", async () => {
  const h = harness()
  prefsStore.set({ showOtherEdition: false })
  const { router } = await renderInRouter(<AccountMenuPanel />, {
    initialEntries: ["/cards/BP01-002"],
  })
  h.block()
  await userEvent.click(screen.getByRole("radio", { name: "英版" }))
  expect(await screen.findByRole("status")).toHaveTextContent("正在載入另一版本資料")
  h.fail(true)
  act(() => {
    h.release()
  })
  expect(await screen.findByRole("alert")).toHaveTextContent("另一版本資料載入失敗")
  expect(router.state.location.pathname).toBe("/cards/BP01-002")
  expect(prefsStore.get().cardEdition).toBe("jp")
  expect(h.client.snapshot()?.region).toBe("jp")
  h.fail(false)
  await userEvent.click(screen.getByRole("button", { name: "重試" }))
  await waitFor(() => {
    expect(router.state.location.pathname).toBe("/cards/BP01EN-002")
  })
  expect(router.state.location.state).toMatchObject({
    route: { cardId: "c:bp01-002", region: "en" },
  })
})

it("opens the chosen edition's list from a card URL that names no card", async () => {
  harness()
  const { router } = await renderInRouter(<AccountMenuPanel />, {
    initialEntries: ["/cards/BP99-999"],
  })
  await userEvent.click(screen.getByRole("radio", { name: "英版" }))
  await waitFor(() => {
    expect(router.state.location.pathname + router.state.location.search).toBe("/cards?ed=en")
  })
  expect(prefsStore.get().cardEdition).toBe("en")
  expect(screen.queryByRole("alert")).not.toBeInTheDocument()
})

it("opens another-region direct links even with comparison off", async () => {
  const h = harness()
  prefsStore.set({ showOtherEdition: false })
  const result = await locateCard(h.client, { namespace: "official", key: "BP01EN-002" })
  expect(result.printing.region).toBe("en")
})

it.each([
  ["zh-TW", "英版", "英版尚未收錄這張卡", "卡片版本"],
  ["ja", "英語版", "このカードは英語版に未収録です", "カードの版"],
  ["en", "English", "This card is not yet available in the English edition", "Card edition"],
] as const)(
  "returns to the target list with a missing-edition message in %s",
  async (language, option, message, editionLabel) => {
    harness()
    const missing = CARDS.find((card) =>
      card.printings.every((printing) => printing.region === "jp"),
    )?.printings[0]
    assert(missing)
    const { router } = await renderInRouter(<AccountMenuPanel />, {
      initialEntries: [`/cards/${missing.cardNo}`],
      language,
    })
    await userEvent.click(
      within(screen.getByRole("radiogroup", { name: editionLabel })).getByRole("radio", {
        name: option,
      }),
    )
    await waitFor(() => {
      expect(router.state.location.pathname + router.state.location.search).toBe("/cards?ed=en")
    })
    expect(await screen.findByText(message)).toBeInTheDocument()
  },
)

it("preloads only the other catalog when the selected search lives in a Worker", async () => {
  const h = harness()
  const jpPaths = new Set(
    arrayValue(built.manifest["files"])
      .map((raw) => objectValue(raw))
      .filter((file) => file["purpose"] === "catalog" && file["region"] === "jp")
      .map((file) => stringValue(file["path"])),
  )
  const { result } = renderHook(() => useOtherEdition(h.client, "jp", true, true))
  await waitFor(() => {
    expect(result.current.catalog).toBeDefined()
  })
  expect(h.requests.filter((path) => jpPaths.has(path))).toEqual([])
  expect(h.client.snapshot()).toBeNull()
  expect(h.requests.some((path) => enPaths.has(path))).toBe(true)
})

it("cancels an edition switch when navigating away and clears its loading indicator", async () => {
  const h = harness()
  h.block()
  const { router } = await renderInRouter(<AccountMenuPanel />, {
    initialEntries: ["/cards/BP01-002"],
  })
  await userEvent.click(screen.getByRole("radio", { name: "英版" }))
  expect(await screen.findByRole("status")).toBeInTheDocument()
  await act(async () => {
    await router.navigate("/cards/BP01-003")
  })
  expect(screen.queryByText("正在載入另一版本資料…")).not.toBeInTheDocument()
  act(() => {
    h.release()
  })
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
  expect(router.state.location.pathname).toBe("/cards/BP01-003")
})
