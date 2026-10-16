import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { prefsStore, recentStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestSearchWorker } from "../../test-utils/search-worker"

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })

afterAll(() => vi.unstubAllGlobals())

it("keeps recent and suggested rows with the image error label when metadata fails", async () => {
  vi.stubGlobal("Worker", TestSearchWorker)
  const imagePaths = new Set(
    (built.manifest["files"] as { path: string; row_counts: unknown }[])
      .filter((file) => JSON.stringify(file.row_counts).includes("printing_image"))
      .map((file) => file.path),
  )
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const path = url.replace(/^.*\/cdn\//u, "")
    const bytes = built.files.get(path)
    return Promise.resolve(
      imagePaths.has(path)
        ? new Response(null, { status: 503 })
        : bytes
          ? new Response(bytes.slice().buffer)
          : new Response(null, { status: 404 }),
    )
  })
  recentStore.push("p:bp01-001")
  prefsStore.set({ uiLanguage: "zh-TW" })
  await renderRoutes(routes, { initialEntries: ["/"] })
  const recent = within(screen.getByRole("region", { name: "最近查看" }))
  const link = await recent.findByRole("link", {}, { timeout: 4000 })
  await waitFor(() => expect(link).toHaveTextContent("卡圖資訊載入失敗"))
  expect(link).toHaveTextContent("BP01-001")
  expect(link.querySelector("img")).toBeNull()
  await userEvent.type(screen.getByRole("combobox"), "bp01-001")
  const list = await screen.findByRole("listbox")
  await waitFor(() => {
    const option = within(list).getAllByRole("option")[0]
    expect(option).toHaveTextContent("BP01-001")
    expect(option).toHaveTextContent("卡圖資訊載入失敗")
  })
})
