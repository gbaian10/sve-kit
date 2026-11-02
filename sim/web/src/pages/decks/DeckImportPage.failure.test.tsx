import "fake-indexeddb/auto"

import { fireEvent, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"

beforeEach(() => vi.stubGlobal("Worker", TestAppWorker))
afterEach(() => vi.unstubAllGlobals())

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })

it("says the cards could not load, keeps the pasted list and loads them on retry", async () => {
  let online = false
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const body = online ? built.files.get(url.replace(/^.*\/cdn\//u, "")) : undefined
    return Promise.resolve(
      body ? new Response(body.slice().buffer) : new Response(null, { status: 503 }),
    )
  })
  prefsStore.set({ uiLanguage: "zh-TW" })
  await renderRoutes(routes, { initialEntries: ["/decks/import"] })
  fireEvent.change(screen.getByRole("textbox", { name: "牌組文字或分享碼" }), {
    target: { value: "3 BP01-001" },
  })
  const alert = await screen.findByRole("alert", {}, { timeout: 15_000 })
  expect(alert).toHaveTextContent("載入失敗")
  online = true
  await userEvent.setup().click(screen.getByRole("button", { name: "重試" }))
  expect(
    await screen.findByRole("region", { name: "辨識結果" }, { timeout: 15_000 }),
  ).toBeInTheDocument()
  expect(screen.getByRole("textbox", { name: "牌組文字或分享碼" })).toHaveValue("3 BP01-001")
}, 45_000)
