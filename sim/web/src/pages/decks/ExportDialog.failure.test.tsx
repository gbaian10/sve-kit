import "fake-indexeddb/auto"

import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { type JsonObject, stringValue } from "../../data"
import { createDeck } from "../../domain/deck/model"
import { decksStore, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"

beforeEach(() => vi.stubGlobal("Worker", TestAppWorker))
afterEach(() => vi.unstubAllGlobals())

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })
// The files that complete the printing data, beyond the basic catalog.
const printingFiles = new Set(
  (built.manifest["files"] as JsonObject[])
    .filter((file) => ["print", "product"].includes(stringValue(file["purpose"])))
    .map((file) => stringValue(file["path"])),
)

it("offers a retry when the printings fail to load, then exports", async () => {
  expect(printingFiles.size).toBeGreaterThan(0)
  let printingsOnline = false
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const path = url.replace(/^.*\/cdn\//u, "")
    const body = printingFiles.has(path) && !printingsOnline ? undefined : built.files.get(path)
    return Promise.resolve(
      body ? new Response(body.slice().buffer) : new Response(null, { status: 503 }),
    )
  })
  const deck = createDeck(
    { name: "重試", edition: "jp", basis: { kind: "class", classCode: "royal" } },
    new Date("2026-11-03T00:00:00.000Z"),
  )
  await decksStore.save(deck)
  prefsStore.set({ uiLanguage: "zh-TW" })
  await renderRoutes(routes, { initialEntries: [`/decks/${deck.id}`] })
  const user = userEvent.setup()
  await user.click(await screen.findByRole("button", { name: "匯出／分享" }, { timeout: 15_000 }))
  const dialog = await screen.findByRole("dialog", { name: "匯出／分享" })
  expect(await within(dialog).findByRole("alert", {}, { timeout: 15_000 })).toHaveTextContent(
    "載入失敗",
  )
  printingsOnline = true
  await user.click(within(dialog).getByRole("button", { name: "重試" }))
  expect(
    await within(dialog).findByRole("textbox", { name: "文字" }, { timeout: 15_000 }),
  ).toHaveValue("# 重試\n\n## 主牌組 (0)\n\n## 進化牌組 (0)\n")
}, 45_000)
