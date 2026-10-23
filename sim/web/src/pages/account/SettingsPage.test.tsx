import { createHash } from "node:crypto"

import { act, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"

import { appBackgroundDownloads } from "../../data"
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
    await userEvent.click(
      within(screen.getByRole("radiogroup", { name: "單卡開啟方式" })).getByRole("radio", {
        name: "中間彈出",
      }),
    )
    expect(readPrefs()).toMatchObject({
      symbolLabels: true,
      banRegion: "en",
      effectLanguage: "both",
      cardOpenMode: "modal",
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
  const background = screen.getByRole("switch", { name: "背景下載" })
  expect(background).toHaveAttribute("aria-checked", "true")
  await userEvent.click(background)
  expect(readPrefs()).toMatchObject({
    backgroundDownloads: false,
    uiLanguage: null,
    cardEdition: "jp",
  })
  for (const name of ["預先下載所有版次", "預先下載上一張／下一張"]) {
    expect(screen.getByRole("switch", { name })).toHaveAttribute("aria-checked", "false")
    await userEvent.click(screen.getByRole("switch", { name }))
  }
  expect(readPrefs()).toMatchObject({ preloadPrintings: true, preloadNeighbours: true })
  view.unmount()
  prefsStore.reload()
  await renderInRouter(<SettingsPage />)
  expect(screen.getByRole("switch", { name: "背景下載" })).toHaveAttribute("aria-checked", "false")
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
    collapsedText: { effectOriginal: true, flavorOriginal: true },
    showFlavorText: false,
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
    listMobileWidths: { number: 160 },
    listVisibility: { ...DEFAULT_PREFS.listVisibility, rarity: false },
  })
  await renderInRouter(<SettingsPage />)
  await userEvent.click(screen.getByRole("button", { name: "恢復預設" }))
  await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "取消" }))
  expect(readPrefs().infiniteScroll).toBe(false)
  expect(readPrefs().collapsedText).toEqual({
    effectOriginal: true,
    flavorOriginal: true,
  })
  await userEvent.click(screen.getByRole("button", { name: "恢復預設" }))
  await userEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", { name: "恢復預設" }),
  )
  expect(readPrefs()).toEqual(DEFAULT_PREFS)
  expect(prefsStore.get()).toEqual(DEFAULT_PREFS)
  expect(recentStore.get()).toEqual(recent)
  expect(await cache.keys()).toHaveLength(1)
})

it("shows flavor text by default and remembers disabling it in advanced settings", async () => {
  const view = await renderInRouter(<SettingsPage />)
  const advanced = within(screen.getByRole("region", { name: "進階設定" }))
  const toggle = advanced.getByRole("switch", { name: "顯示風味文字" })
  expect(toggle).toHaveAttribute("aria-checked", "true")
  await userEvent.click(toggle)
  expect(readPrefs().showFlavorText).toBe(false)
  view.unmount()
  prefsStore.reload()
  await renderInRouter(<SettingsPage />)
  expect(screen.getByRole("switch", { name: "顯示風味文字" })).toHaveAttribute(
    "aria-checked",
    "false",
  )
})

it("shows live download, pause, failed writes and completion only while the selected bytes remain saved", async () => {
  let hidden = false
  vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden)
  vi.spyOn(navigator, "onLine", "get").mockReturnValue(true)
  const bytes = new TextEncoder().encode("background bytes")
  const gate = Promise.withResolvers<undefined>()
  vi.stubGlobal("fetch", async () => {
    await gate.promise
    return new Response(bytes.slice().buffer)
  })
  const selection = {
    base: "https://settings.test",
    manifestHash: "selected",
    dataVersion: "20261024T000000Z-0001",
    edition: "jp" as const,
    files: new Map([
      [
        "data",
        {
          key: "data",
          path: "data.json",
          sha256: `sha256:${createHash("sha256").update(bytes).digest("hex")}`,
          bytes: bytes.length,
          compressed_bytes: { br: 8 },
          role: "text",
          region: "jp",
          dependencies: [],
          source_refs: [],
        },
      ],
    ]),
  }
  const failing = memoryCache()
  const rawName = `sve-raw:${encodeURIComponent(selection.base)}`
  const raw = await failing.open(rawName)
  // Retention metadata must persist so this scenario reaches the raw write.
  raw.put = () => Promise.reject(new Error("quota"))
  const open = failing.open.bind(failing)
  failing.open = (name) => (name === rawName ? Promise.resolve(raw) : open(name))
  vi.stubGlobal("caches", failing)
  appBackgroundDownloads.setEnabled(true)
  appBackgroundDownloads.select(selection)
  try {
    await renderInRouter(<SettingsPage />)
    const downloads = within(screen.getByRole("region", { name: "背景下載" }))
    expect(await downloads.findByText("下載中")).toBeInTheDocument()
    act(() => {
      hidden = true
      document.dispatchEvent(new Event("visibilitychange"))
    })
    expect(downloads.getByText("已暫停")).toBeInTheDocument()
    gate.resolve(undefined)
    act(() => {
      hidden = false
      document.dispatchEvent(new Event("visibilitychange"))
    })
    expect(await downloads.findByText("尚未全部下載")).toBeInTheDocument()
    expect(downloads.getByText(/寫入失敗或儲存空間不足/u)).toBeInTheDocument()
    const denied = memoryCache()
    await denied.open(`sve-raw:${encodeURIComponent(selection.base)}`)
    denied.open = () => Promise.reject(new Error("storage unavailable"))
    vi.stubGlobal("caches", denied)
    act(() => {
      appBackgroundDownloads.select({ ...selection, manifestHash: "denied" })
    })
    expect(await downloads.findByText(/瀏覽器目前無法儲存資料/u)).toBeInTheDocument()
    const storage = memoryCache()
    vi.stubGlobal("caches", storage)
    act(() => {
      appBackgroundDownloads.select({ ...selection, manifestHash: "next" })
    })
    expect(await downloads.findByText("已完成")).toBeInTheDocument()
    expect(downloads.getByText("2026-10-24 · 日版")).toBeInTheDocument()
    expect(downloads.getByRole("progressbar")).toHaveAttribute("value", "1")
    await act(async () => {
      await storage.delete(`sve-raw:${encodeURIComponent(selection.base)}`)
      await appBackgroundDownloads.recheck()
    })
    await waitFor(() => {
      expect(downloads.getByText("尚未全部下載")).toBeInTheDocument()
    })
    expect(downloads.getByText(/部分資料已被移除/u)).toBeInTheDocument()
  } finally {
    gate.resolve(undefined)
    appBackgroundDownloads.select(null)
  }
})
