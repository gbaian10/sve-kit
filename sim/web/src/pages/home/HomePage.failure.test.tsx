import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, beforeEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { type JsonObject, objectValue } from "../../data"
import { prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestSearchWorker } from "../../test-utils/search-worker"

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })

beforeEach(() => vi.stubGlobal("Worker", TestSearchWorker))

afterAll(() => {
  vi.unstubAllGlobals()
})

it("keeps the home page and the typed text when the catalog fails to load", async () => {
  // The index points at a manifest whose bytes do not match its hash.
  const files = new Map(built.files)
  const index = objectValue(
    JSON.parse(new TextDecoder().decode(files.get("snapshots/versions/index.json"))) as JsonObject,
  )
  const path = "snapshots/manifests/broken.json"
  objectValue(index["current"])["manifest_path"] = path
  objectValue(index["current"])["manifest_sha256"] = `sha256:${"0".repeat(64)}`
  files.set(path, new TextEncoder().encode("{}"))
  files.set("snapshots/versions/index.json", new TextEncoder().encode(JSON.stringify(index)))
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const body = files.get(url.replace(/^.*\/cdn\//u, ""))
    return Promise.resolve(
      body ? new Response(body.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
  prefsStore.set({ uiLanguage: "zh-TW" })
  const { router } = await renderRoutes(routes, { initialEntries: ["/"] })
  const alert = await screen.findByRole("alert")
  expect(within(alert).getByRole("button", { name: "重試" })).toBeVisible()
  const user = userEvent.setup()
  const input = screen.getByRole("combobox")
  await user.type(input, "bp01{Enter}")
  expect(input).toHaveValue("bp01")
  expect(router.state.location.pathname).toBe("/")
  expect(screen.getByText("卡片資料載入失敗，暫時無法搜尋。")).toBeVisible()
  expect(screen.queryByRole("listbox")).toBeNull()
  expect(screen.getByRole("button", { name: "隨機一張" })).toBeDisabled()
  expect(screen.getByRole("region", { name: "官方最新消息" })).toBeVisible()
})
