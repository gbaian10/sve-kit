import "fake-indexeddb/auto"

import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, beforeAll, beforeEach, expect, it, onTestFinished, vi } from "vitest"

import { routes } from "../../app/routes"
import { createDeck } from "../../domain/deck/model"
import { decksStore, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"
import { stubFixtureFetch } from "../../test-utils/fixture-fetch"

beforeEach(() => vi.stubGlobal("Worker", TestAppWorker))
beforeAll(async () => {
  await stubFixtureFetch()
})
afterAll(() => {
  vi.unstubAllGlobals()
})

it("copies the text, link and code with feedback, and hands the link to the share sheet", async () => {
  const deck = {
    ...createDeck(
      { name: "分享", edition: "jp", basis: { kind: "class", classCode: "royal" } },
      new Date("2026-11-03T00:00:00.000Z"),
    ),
    main: [{ cardId: "c:bp01-001", count: 3 }],
  }
  await decksStore.save(deck)
  prefsStore.set({ uiLanguage: "zh-TW" })
  // jsdom has no Web Share API; the test adds it for this one test.
  const share = vi.fn(() => Promise.reject(new Error("AbortError")))
  Object.defineProperty(navigator, "share", { value: share, configurable: true })
  onTestFinished(() => {
    Reflect.deleteProperty(navigator, "share")
  })
  await renderRoutes(routes, { initialEntries: [`/decks/${deck.id}`] })
  const user = userEvent.setup()
  await user.click(await screen.findByRole("button", { name: "匯出／分享" }, { timeout: 25_000 }))
  const dialog = await screen.findByRole("dialog", { name: "匯出／分享" })

  // A refused clipboard leaves the button as it was.
  const write = vi
    .spyOn(navigator.clipboard, "writeText")
    .mockRejectedValueOnce(new Error("denied"))
  await user.click(
    await within(dialog).findByRole("button", { name: "複製文字" }, { timeout: 25_000 }),
  )
  expect(write).toHaveBeenCalledWith(expect.stringContaining("3 BP01-001"))
  expect(within(dialog).getByRole("button", { name: "複製文字" })).toBeInTheDocument()
  await user.click(within(dialog).getByRole("button", { name: "複製文字" }))
  expect(await within(dialog).findByRole("button", { name: "已複製" })).toBeInTheDocument()

  await user.click(within(dialog).getByRole("radio", { name: "分享" }))
  const link = within(dialog).getByRole<HTMLTextAreaElement>("textbox", { name: "分享連結" }).value
  await user.click(within(dialog).getByRole("button", { name: "複製連結" }))
  expect(await navigator.clipboard.readText()).toBe(link)
  expect(within(dialog).getByRole("button", { name: "已複製" })).toBeInTheDocument()
  await user.click(within(dialog).getByRole("button", { name: "複製分享碼" }))
  expect(await navigator.clipboard.readText()).toMatch(/^SVEK-/u)
  await user.click(within(dialog).getByRole("button", { name: "分享…" }))
  expect(share).toHaveBeenCalledWith({ title: "分享", url: link })
}, 60_000)
