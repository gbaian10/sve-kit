import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, beforeEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { snapshotClient } from "../../data"
import { prefsStore, recentStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestSearchWorker } from "../../test-utils/search-worker"

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })

beforeEach(() => vi.stubGlobal("Worker", TestSearchWorker))
afterAll(() => {
  vi.unstubAllGlobals()
})

it("fills every block from the search Worker without loading the full snapshot", async () => {
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const body = built.files.get(url.replace(/^.*\/cdn\//u, ""))
    return Promise.resolve(
      body ? new Response(body.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
  recentStore.push("p:bp01-001")
  prefsStore.set({ uiLanguage: "zh-TW" })
  await renderRoutes(routes, { initialEntries: ["/"] })
  const user = userEvent.setup()

  const recent = within(screen.getByRole("region", { name: "最近查看" }))
  expect(await recent.findByRole("link", {}, { timeout: 4000 })).toHaveTextContent("BP01-001")
  const banner = within(screen.getByRole("region", { name: "試作補充包第1彈" }))
  expect(await banner.findByText(/已收錄/u)).toBeVisible()
  await user.type(screen.getByRole("combobox"), "試作")
  await waitFor(() => {
    const list = screen.getByRole("listbox", { name: "建議" })
    expect(within(list).getAllByRole("option").length).toBeGreaterThan(1)
  })
  await waitFor(() => {
    expect(screen.getByRole("button", { name: "隨機一張" })).toBeEnabled()
  })

  expect(snapshotClient().status()).toEqual({ state: "idle" })
  expect(snapshotClient().snapshot()).toBeNull()
})
