import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, beforeEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { type JsonObject, stringValue } from "../../data"
import { prefsStore, recentStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestSearchWorker } from "../../test-utils/search-worker"

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })

beforeEach(() => vi.stubGlobal("Worker", TestSearchWorker))

afterAll(() => {
  vi.unstubAllGlobals()
})

it("keeps the typed text and stored recent ids during a download, then searches once", async () => {
  const startup = (built.manifest["files"] as JsonObject[]).filter(
    (file) => file["purpose"] === "catalog" && (file["region"] === "jp" || file["region"] === null),
  )
  const held = stringValue(startup[0]?.["path"])
  let release = () => {}
  const gate = new Promise<void>((resolve) => {
    release = resolve
  })
  vi.stubGlobal("fetch", async (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const key = url.replace(/^.*\/cdn\//u, "")
    if (key === held) await gate
    const body = built.files.get(key)
    return body ? new Response(body.slice().buffer) : new Response(null, { status: 404 })
  })
  recentStore.push("p:unknown")
  recentStore.push("p:bp01-001")
  prefsStore.set({ uiLanguage: "zh-TW" })
  const { router } = await renderRoutes(routes, { initialEntries: ["/"] })
  const user = userEvent.setup()

  // The shell and the search are usable before any data arrives.
  await screen.findByRole("progressbar", { name: "卡片資料下載進度" })
  const random = screen.getByRole("button", { name: "隨機一張" })
  expect(random).toBeDisabled()
  const recent = within(screen.getByRole("region", { name: "最近查看" }))
  expect(recent.getByRole("list", { name: "最近查看載入中" })).toHaveAttribute("aria-busy", "true")
  expect(recent.getAllByRole("listitem")).toHaveLength(2)

  const input = screen.getByRole("combobox")
  await user.type(input, "bp01-51{Enter}")
  expect(input).toHaveValue("bp01-51")
  expect(router.state.location.pathname).toBe("/")
  expect(screen.getByText(/可以先輸入，下載完成後會自動搜尋/u)).toBeVisible()
  expect(screen.queryByRole("listbox")).toBeNull()
  expect(screen.queryByRole("button", { name: /看全部/u })).toBeNull()
  expect(screen.queryByText("沒有符合的卡")).toBeNull()

  release()
  // Once the catalog is complete the last input is searched without another keystroke.
  const list = await screen.findByRole("listbox", { name: "建議" }, { timeout: 4000 })
  expect(within(list).getAllByRole("option")[0]).toHaveTextContent("BP01-051")
  expect(screen.getByRole("button", { name: /看全部/u })).toBeVisible()
  await waitFor(() => {
    expect(random).toBeEnabled()
    expect(recent.getAllByRole("link")).toHaveLength(1)
  })
  expect(recentStore.get()).toEqual(["p:bp01-001", "p:unknown"])
})
