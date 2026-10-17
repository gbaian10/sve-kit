import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"

import { DEFAULT_PREFS, prefsStore, readPrefs, recentStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { memoryCache } from "../../test-utils/cache"
import { SettingsPage } from "./SettingsPage"

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  prefsStore.set(DEFAULT_PREFS)
  recentStore.clear()
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
  expect(readPrefs()).toMatchObject({ preloadPrintings: true, preloadNeighbours: true })
  view.unmount()
  prefsStore.reload()
  await renderInRouter(<SettingsPage />)
  for (const name of ["預先下載所有版次", "預先下載上一張／下一張"])
    expect(screen.getByRole("switch", { name })).toHaveAttribute("aria-checked", "true")
})

it("defaults to infinite scroll and persists the manual paging preference", async () => {
  prefsStore.set(DEFAULT_PREFS)
  await renderInRouter(<SettingsPage />)
  const toggle = screen.getByRole("switch", { name: "無限捲動" })
  expect(toggle).toHaveAttribute("aria-checked", "true")
  await userEvent.click(toggle)
  expect(readPrefs().infiniteScroll).toBe(false)
})

it("confirms restoring all defaults and keeps recent cards and downloaded data", async () => {
  const storage = memoryCache()
  vi.stubGlobal("caches", storage)
  const cache = await storage.open("sve-image-responses-v2")
  await cache.put(
    "https://cdn.test/images/card_m/synthetic.webp?v=1",
    new Response(new Uint8Array([1])),
  )
  recentStore.push("p:synthetic-recent")
  const recent = recentStore.get()
  prefsStore.set({
    uiLanguage: "ja",
    cardEdition: "en",
    nameDisplay: "both",
    effectLanguage: "original",
    preloadPrintings: true,
    preloadNeighbours: true,
    infiniteScroll: false,
    symbolLabels: true,
    termEmphasis: false,
    theme: "dark",
    accent: "red",
    banRegion: "en",
    gridDensity: 3,
    viewMode: "list",
    showOtherEdition: { jp: true, en: false },
  })
  prefsStore.set({
    listWidths: { name: 280 },
    listVisibility: { ...DEFAULT_PREFS.listVisibility, rarity: false },
  })
  await renderInRouter(<SettingsPage />)
  await userEvent.click(screen.getByRole("button", { name: "恢復預設" }))
  await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "取消" }))
  expect(readPrefs().infiniteScroll).toBe(false)
  await userEvent.click(screen.getByRole("button", { name: "恢復預設" }))
  await userEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", { name: "恢復預設" }),
  )
  expect(readPrefs()).toEqual(DEFAULT_PREFS)
  expect(prefsStore.get()).toEqual(DEFAULT_PREFS)
  expect(recentStore.get()).toEqual(recent)
  expect(await cache.keys()).toHaveLength(1)
})
