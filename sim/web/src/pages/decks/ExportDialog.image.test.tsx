import "fake-indexeddb/auto"

import { act, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, beforeAll, beforeEach, expect, it, vi } from "vitest"

import { routes } from "../../app/routes"
import { createDeck } from "../../domain/deck/model"
import { decksStore, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"
import { stubFixtureFetch } from "../../test-utils/fixture-fetch"

// jsdom has no canvas: drawing and encoding are held at their boundary so the test decides when
// (and in which order) they finish.
const control = vi.hoisted(() => ({
  pending: [] as ((canvas: { width: number; height: number }) => void)[],
}))
vi.mock("./deck-image", async (original) => ({
  ...(await original<typeof import("./deck-image")>()),
  renderDeckImage: () =>
    new Promise((resolve) => {
      control.pending.push(resolve)
    }),
  encodeDeckImage: () => Promise.resolve(new Blob(["image"], { type: "image/png" })),
}))

const created = vi.fn(() => "blob:deck")
beforeEach(() => {
  vi.stubGlobal("Worker", TestAppWorker)
  control.pending.length = 0
  created.mockClear()
  URL.createObjectURL = created
  URL.revokeObjectURL = vi.fn()
})
beforeAll(async () => {
  await stubFixtureFetch()
})
afterAll(() => {
  vi.unstubAllGlobals()
})

async function openImageTab() {
  const deck = createDeck(
    { name: "圖", edition: "jp", basis: { kind: "class", classCode: "royal" } },
    new Date("2026-11-03T00:00:00.000Z"),
  )
  await decksStore.save(deck)
  prefsStore.set({ uiLanguage: "zh-TW" })
  await renderRoutes(routes, { initialEntries: [`/decks/${deck.id}`] })
  const user = userEvent.setup()
  await user.click(await screen.findByRole("button", { name: "匯出／分享" }, { timeout: 25_000 }))
  const dialog = await screen.findByRole("dialog", { name: "匯出／分享" })
  await user.click(await within(dialog).findByRole("radio", { name: "圖片" }, { timeout: 25_000 }))
  return { user, dialog }
}

const finish = async (index: number, width: number) => {
  await act(async () => {
    control.pending[index]?.({ width, height: 600 })
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
