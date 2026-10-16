import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"

import { DEFAULT_PREFS, prefsStore, readPrefs } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { memoryCache } from "../../test-utils/cache"
import { SettingsPage } from "./SettingsPage"

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  prefsStore.set(DEFAULT_PREFS)
})

describe("SettingsPage", () => {
  it("shows the display settings and writes them to the preferences", async () => {
    await renderInRouter(<SettingsPage />)
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("帳號與設定")
    await userEvent.click(screen.getByRole("switch", { name: "圖示旁附名稱" }))
    await userEvent.click(screen.getByRole("radio", { name: "英文圈" }))
    await userEvent.click(
      within(screen.getByRole("radiogroup", { name: "效果文字" })).getByRole("radio", {
        name: "兩者",
      }),
    )
    expect(readPrefs()).toMatchObject({
      symbolLabels: true,
      banRegion: "en",
      effectLanguage: "both",
    })
    expect(screen.getByRole("switch", { name: "圖示旁附名稱" })).toHaveAttribute(
      "aria-checked",
      "true",
    )
    expect(screen.queryByRole("switch", { name: "省流量" })).not.toBeInTheDocument()
  })
})

it("refreshes estimates on demand, leaves caches untouched when clearing is cancelled, and shows request failure", async () => {
  const storage = memoryCache()
  vi.stubGlobal("caches", storage)
  const cache = await storage.open("sve-image-responses-v2")
  await cache.put(
    "https://cdn.test/images/card_m/1.webp?v=1",
    new Response(new Uint8Array(1024 * 1024)),
  )
  await renderInRouter(<SettingsPage />)
  expect(await screen.findByText("1 MiB")).toBeInTheDocument()
  expect(screen.getByText("無法估算")).toBeInTheDocument()
  await cache.put(
    "https://cdn.test/images/card_m/2.webp?v=1",
    new Response(new Uint8Array(1024 * 1024)),
  )
  await userEvent.click(screen.getByRole("radio", { name: "深色" }))
  expect(screen.getByText("1 MiB")).toBeInTheDocument()
  await userEvent.click(screen.getByRole("button", { name: "重新估算" }))
  expect(await screen.findByText("2 MiB")).toBeInTheDocument()
  await userEvent.click(screen.getByRole("button", { name: "清除快取" }))
  await userEvent.click(screen.getByRole("button", { name: "取消" }))
  expect(await cache.keys()).toHaveLength(2)
  await userEvent.click(screen.getByRole("button", { name: "清除快取" }))
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("denied")
  })
  await userEvent.click(screen.getByRole("button", { name: "清除並重新載入" }))
  expect(screen.getByRole("status")).toHaveTextContent("快取清除失敗或未完成")
  expect(await cache.keys()).toHaveLength(2)
})

it("starts preloading disabled and remembers both advanced switches", async () => {
  prefsStore.set(DEFAULT_PREFS)
  const view = await renderInRouter(<SettingsPage />)
  for (const name of ["預先下載所有版次", "預先下載上一張／下一張"]) {
    expect(screen.getByRole("switch", { name })).toHaveAttribute("aria-checked", "false")
    await userEvent.click(screen.getByRole("switch", { name }))
  }
  expect(readPrefs()).toMatchObject({ prefetchPrintings: true, prefetchNeighbours: true })
  view.unmount()
  prefsStore.reload()
  await renderInRouter(<SettingsPage />)
  for (const name of ["預先下載所有版次", "預先下載上一張／下一張"])
    expect(screen.getByRole("switch", { name })).toHaveAttribute("aria-checked", "true")
})
