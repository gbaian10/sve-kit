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
  let releaseImages = () => {}
  const imageReady = new Promise<void>((resolve) => {
    releaseImages = resolve
  })
  const imagePaths = new Set(
    (built.manifest["files"] as { path: string; row_counts: unknown }[])
      .filter((file) => JSON.stringify(file.row_counts).includes("printing_image"))
      .map((file) => file.path),
  )
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const path = url.replace(/^.*\/cdn\//u, "")
    const body = built.files.get(path)
    return (imagePaths.has(path) ? imageReady : Promise.resolve()).then(() =>
      body ? new Response(body.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
  recentStore.push("p:bp01-001")
  prefsStore.set({ uiLanguage: "zh-TW" })
  await renderRoutes(routes, { initialEntries: ["/"] })
  const user = userEvent.setup()

  const recent = within(screen.getByRole("region", { name: "最近查看" }))
  const recentLink = await recent.findByRole("link", {}, { timeout: 4000 })
  expect(recentLink).toHaveTextContent("BP01-001")
  expect(recentLink).toHaveTextContent("卡圖資訊載入中")
  expect(recentLink.querySelector("img")).toBeNull()
  releaseImages()
  await waitFor(() => {
    expect(recentLink.querySelector("img")).toHaveAttribute(
      "src",
      expect.stringContaining("/images/card_"),
    )
    expect(recentLink).toBeInTheDocument()
  })
  await user.type(screen.getByRole("combobox"), "試作")
  await waitFor(() => {
    const list = screen.getByRole("listbox", { name: "建議" })
    expect(within(list).getAllByRole("option").length).toBeGreaterThan(1)
  })
  const option = within(screen.getByRole("listbox", { name: "建議" })).getAllByRole("option")[0]
  await waitFor(() => {
    expect(option?.querySelector("img")).toHaveAttribute(
      "src",
      expect.stringContaining("/images/card_"),
    )
    expect(option).toBeInTheDocument()
  })
  await waitFor(() => {
    expect(screen.getByRole("button", { name: "隨機一張" })).toBeEnabled()
  })

  expect(snapshotClient().status()).toEqual({ state: "idle" })
  expect(snapshotClient().snapshot()).toBeNull()
})
