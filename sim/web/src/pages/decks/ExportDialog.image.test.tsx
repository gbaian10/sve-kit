import "fake-indexeddb/auto"

import { act, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, afterEach, beforeAll, beforeEach, expect, it, onTestFinished, vi } from "vitest"

import { routes } from "../../app/routes"
import { createDeck } from "../../domain/deck/model"
import type { Deck } from "../../domain/deck/types"
import { decksStore, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"
import { stubFixtureFetch } from "../../test-utils/fixture-fetch"
import { DeckImageMissing } from "./deck-image"

// jsdom has no canvas: drawing and encoding are held at their boundary so the test decides when
// (and in which order) they finish.
const control = vi.hoisted(() => ({
  pending: [] as {
    resolve: (canvas: { width: number; height: number }) => void
    reject: (error: unknown) => void
  }[],
  blob: new Blob(["image"], { type: "image/png" }),
  inputs: [] as import("./deck-image").DeckImageInput[],
}))
vi.mock("./deck-image", async (original) => ({
  ...(await original<typeof import("./deck-image")>()),
  renderDeckImage: (input: import("./deck-image").DeckImageInput) =>
    new Promise((resolve, reject) => {
      control.inputs.push(input)
      control.pending.push({ resolve, reject })
    }),
  encodeDeckImage: () => Promise.resolve(control.blob),
}))

const created = vi.fn(() => "blob:deck")
beforeEach(() => {
  vi.stubGlobal("Worker", TestAppWorker)
  control.pending.length = 0
  control.inputs.length = 0
  control.blob = new Blob(["image"], { type: "image/png" })
  created.mockClear()
  URL.createObjectURL = created
  URL.revokeObjectURL = vi.fn()
})
beforeAll(async () => {
  await stubFixtureFetch()
})
afterEach(() => {
  vi.restoreAllMocks()
})
afterAll(() => {
  vi.unstubAllGlobals()
})

async function openImageTab(changes: Partial<Deck> = {}) {
  const deck = {
    ...createDeck(
      { name: "圖", edition: "jp", basis: { kind: "class", classCode: "royal" } },
      new Date("2026-11-03T00:00:00.000Z"),
    ),
    ...changes,
  }
  await decksStore.save(deck)
  prefsStore.set({ uiLanguage: "zh-TW" })
  await renderRoutes(routes, { initialEntries: [`/decks/${deck.id}`] })
  const user = userEvent.setup()
  // Below 1180 px the deck, with its export button, is a sheet over the pool.
  await user.click(await screen.findByRole("button", { name: "打開牌組" }, { timeout: 25_000 }))
  await user.click(await screen.findByRole("button", { name: "匯出／分享" }, { timeout: 25_000 }))
  const dialog = await screen.findByRole("dialog", { name: "匯出／分享" })
  await user.click(await within(dialog).findByRole("radio", { name: "圖片" }, { timeout: 25_000 }))
  return { user, dialog }
}

const finish = async (index: number, width: number) => {
  await act(async () => {
    control.pending[index]?.resolve({ width, height: 600 })
    await Promise.resolve()
  })
}

it("drops a picture still being made when another size is chosen", async () => {
  const { user, dialog } = await openImageTab()
  await user.click(within(dialog).getByRole("button", { name: "產生圖片" }))
  await user.click(within(dialog).getByRole("radio", { name: "高清（看得清卡文）" }))
  await finish(0, 1200)
  expect(within(dialog).queryByRole("img")).not.toBeInTheDocument()
  expect(created).not.toHaveBeenCalled()

  await user.click(within(dialog).getByRole("button", { name: "產生圖片" }))
  await finish(1, 5226)
  expect(await within(dialog).findByRole("img")).toBeInTheDocument()
  expect(within(dialog).getByRole("link", { name: "下載圖片" })).toHaveAttribute(
    "download",
    "圖-hd.png",
  )
}, 60_000)

it("drops a picture still being made when the dialog closes", async () => {
  const { user, dialog } = await openImageTab()
  await user.click(within(dialog).getByRole("button", { name: "產生圖片" }))
  await user.click(within(dialog).getByRole("button", { name: "關閉" }))
  await finish(0, 1200)
  expect(created).not.toHaveBeenCalled()
  await user.click(screen.getByRole("button", { name: "匯出／分享" }))
  const reopened = await screen.findByRole("dialog", { name: "匯出／分享" })
  await user.click(within(reopened).getByRole("radio", { name: "圖片" }))
  expect(within(reopened).queryByRole("img")).not.toBeInTheDocument()
}, 60_000)

it("names the cards whose images did not load, and says when drawing failed otherwise", async () => {
  const { user, dialog } = await openImageTab()
  await user.click(within(dialog).getByRole("button", { name: "產生圖片" }))
  await act(async () => {
    control.pending[0]?.reject(new DeckImageMissing(["試作の王", "試作の見習い兵"]))
    await Promise.resolve()
  })
  expect(await within(dialog).findByRole("alert")).toHaveTextContent(
    "有 2 張卡圖沒有載入，圖片沒有產生：試作の王、試作の見習い兵。",
  )
  await user.click(within(dialog).getByRole("button", { name: "產生圖片" }))
  await act(async () => {
    control.pending[1]?.reject(new Error("toBlob"))
    await Promise.resolve()
  })
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("圖片產生失敗")
}, 60_000)

it("shows a large picture's size in MB and shares it as a file where the browser can", async () => {
  control.blob = new Blob([new Uint8Array(2.5 * 1024 * 1024)], { type: "image/webp" })
  const share = vi.fn(() => Promise.reject(new Error("AbortError")))
  // jsdom has no Web Share API; the test adds it for this one test.
  Object.defineProperties(navigator, {
    canShare: { value: () => true, configurable: true },
    share: { value: share, configurable: true },
  })
  onTestFinished(() => {
    Reflect.deleteProperty(navigator, "canShare")
    Reflect.deleteProperty(navigator, "share")
  })
  // A name of symbols only leaves no file name of its own.
  const { user, dialog } = await openImageTab({
    name: "★",
    leader: { cardId: "c:sd01-001" },
    main: [{ cardId: "c:bp01-001", count: 3 }],
    evolve: [{ cardId: "c:missing", count: 1 }],
  })
  await user.click(within(dialog).getByRole("radio", { name: "高清（看得清卡文）" }))
  await user.click(within(dialog).getByRole("button", { name: "產生圖片" }))
  // The picture shows the leader beside the name and each card with its count; a card the
  // snapshot does not know has no image to load.
  const input = control.inputs[0]
  expect(input).toMatchObject({
    size: "hd",
    title: "★",
    leader: { label: "試作の王" },
    sections: [
      { cards: [{ label: "試作の見習い兵", count: 3 }] },
      { cards: [{ label: "c:missing", count: 1, src: undefined }] },
    ],
  })
  expect(input?.leaderLine).toContain("試作の王")
  expect(input?.leader?.src).toBeTypeOf("string")
  expect(input?.sections[0]?.cards[0]?.src).toBeTypeOf("string")
  await finish(0, 5226)
  expect(await within(dialog).findByText("5226×600 px・2.5 MB")).toBeInTheDocument()
  expect(within(dialog).getByRole("link", { name: "下載圖片" })).toHaveAttribute(
    "download",
    "deck-hd.webp",
  )
  await user.click(within(dialog).getByRole("button", { name: "分享…" }))
  expect(share).toHaveBeenCalledWith({ files: [expect.any(File)], title: "★" })
}, 60_000)
