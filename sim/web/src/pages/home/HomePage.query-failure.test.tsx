import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, beforeEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { type JsonObject, type JsonValue, stringValue } from "../../data"
import { prefsStore, recentStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestSearchWorker } from "../../test-utils/search-worker"

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })

beforeEach(() => vi.stubGlobal("Worker", TestSearchWorker))
afterAll(() => {
  vi.unstubAllGlobals()
})

it("shows query failures after readiness and retries without losing history or input", async () => {
  const metadata = new Set(
    (built.manifest["files"] as JsonObject[])
      .filter((file) =>
        (file["row_counts"] as JsonValue[][]).some((row) => row[0] === "printing_image"),
      )
      .map((file) => stringValue(file["path"])),
  )
  let broken = false
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const key = url.replace(/^.*\/cdn\//u, "")
    if (broken && metadata.has(key)) return Promise.resolve(new Response("{}"))
    const body = built.files.get(key)
    return Promise.resolve(
      body ? new Response(body.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
  prefsStore.set({ uiLanguage: "zh-TW" })
  const { router } = await renderRoutes(routes, { initialEntries: ["/"] })
  const user = userEvent.setup()
  await waitFor(
    () => {
      expect(screen.getByRole("button", { name: "隨機一張" })).toBeEnabled()
    },
    { timeout: 4000 },
  )
  broken = true
  recentStore.push("p:bp01-001")
  const recent = within(screen.getByRole("region", { name: "最近查看" }))
  const recentFailure = await recent.findByRole("alert")
  expect(recentFailure).toHaveTextContent("最近查看載入失敗")
  expect(recent.queryByRole("list", { name: "最近查看載入中" })).toBeNull()
  expect(recentStore.get()).toEqual(["p:bp01-001"])
  const search = within(screen.getByRole("region", { name: "搜尋卡片" }))
  await user.type(screen.getByRole("combobox"), "bp01-51")
  const searchFailure = await search.findByRole("alert")
  expect(searchFailure).toHaveTextContent("暫時無法搜尋")
  expect(search.queryByText("沒有符合的卡")).toBeNull()
  await user.keyboard("{Enter}")
  expect(router.state.location.pathname).toBe("/")
  broken = false
  const retry = within(searchFailure).getByRole("button", { name: "重試" })
  await user.tab()
  await user.tab()
  expect(retry).toHaveFocus()
  await user.keyboard("{Enter}")
  const list = await screen.findByRole("listbox", { name: "建議" })
  expect(within(list).getAllByRole("option")[0]).toHaveTextContent("BP01-051")
  await waitFor(() => {
    expect(recent.getByRole("link")).toHaveTextContent("BP01-001")
  })
  expect(screen.getByRole("combobox")).toHaveValue("bp01-51")
})
