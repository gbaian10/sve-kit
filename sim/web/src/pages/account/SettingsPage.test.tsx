import { createHash } from "node:crypto"

import { act, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"

import { appBackgroundDownloads } from "../../data"
import { DEFAULT_PREFS, prefsStore, recentStore } from "../../settings"
import { readPrefs } from "../../settings/prefs-read-write"
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
    await userEvent.click(screen.getByRole("radio", { name: "英版" }))
    await userEvent.click(
      within(screen.getByRole("radiogroup", { name: "效果文字" })).getByRole("radio", {
        name: "翻譯＋原文",
      }),
    )
    await userEvent.click(
      within(screen.getByRole("radiogroup", { name: "Q&A" })).getByRole("radio", {
        name: "不顯示",
      }),
    )
    await userEvent.click(
      within(screen.getByRole("radiogroup", { name: "卡片開啟" })).getByRole("radio", {
        name: "中間彈出",
      }),
    )
    const layout = within(screen.getByRole("radiogroup", { name: "版面" }))
    expect(layout.getByRole("radio", { name: "卡片在左" })).toBeChecked()
    await userEvent.click(layout.getByRole("radio", { name: "篩選在左" }))
    expect(readPrefs()).toMatchObject({
      symbolLabels: true,
      cardEdition: "en",
      effectLanguage: "both",
      qaDisplay: "none",
      cardOpenMode: "modal",
      cardsLayout: "filterLeft",
    })
    expect(screen.getByRole("switch", { name: "圖示旁附名稱" })).toHaveAttribute(
      "aria-checked",
      "true",
    )
    expect(screen.queryByRole("switch", { name: "省流量" })).not.toBeInTheDocument()
  })
})

it("groups settings by purpose, with the reset button above every group", async () => {
  await renderInRouter(<SettingsPage />)
  const headings = screen.getAllByRole("heading", { level: 2 }).map((node) => node.textContent)
  expect(headings).toEqual([
    "介面",
    "卡片資料",
    "卡文翻譯顯示",
    "卡文樣式",
    "瀏覽",
    "預先下載",
    "背景下載",
    "儲存空間",
  ])
  const text = within(screen.getByRole("region", { name: "卡文翻譯顯示" }))
  for (const name of ["卡名顯示", "效果文字", "風味文字", "Q&A"])
    expect(text.getByRole("radiogroup", { name })).toBeInTheDocument()
  const style = within(screen.getByRole("region", { name: "卡文樣式" }))
  expect(style.getByRole("switch", { name: "術語加粗" })).toBeInTheDocument()
  expect(style.getByRole("switch", { name: "圖示旁附名稱" })).toBeInTheDocument()
  const reset = screen.getByRole("button", { name: "恢復預設" })
  // The reset button sits in the title row, before every group.
  const first = screen.getByRole("region", { name: "介面" })
  expect(reset.compareDocumentPosition(first) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  expect(screen.getAllByRole("button", { name: "恢復預設" })).toHaveLength(1)
})

it("previews each option with the sample card and marks the chosen one", async () => {
  prefsStore.set({ effectLanguage: "both", termEmphasis: false })
  await renderInRouter(<SettingsPage />)
  expect(screen.queryByRole("tooltip")).toBeNull()
  await userEvent.hover(screen.getByRole("button", { name: "說明：效果文字" }))
  const options = within(screen.getByRole("tooltip")).getAllByRole("listitem")
  const current = (list: readonly HTMLElement[]) =>
    list.map((node) => node.getAttribute("aria-current"))
  expect(options).toHaveLength(3)
  expect(current(options)).toEqual([null, null, "true"])
  // The sample renders with the real card-text pieces: icons and the framed original text.
  const [original, , both] = options as [HTMLElement, HTMLElement, HTMLElement]
  expect(within(both).getByText("原文", { selector: "p" })).toBeInTheDocument()
  expect(within(both).getAllByRole("img", { name: "入場曲" })).toHaveLength(1)
  expect(original.querySelector("strong")).toBeNull()
  await userEvent.unhover(screen.getByRole("button", { name: "說明：效果文字" }))
  await userEvent.hover(screen.getByRole("button", { name: "說明：術語加粗" }))
  const toggles = within(screen.getByRole("tooltip")).getAllByRole("listitem")
  const [on, off] = toggles as [HTMLElement, HTMLElement]
  expect(current(toggles)).toEqual([null, "true"])
  // Only the data-marked terms are bold, mid-sentence ones included; the rest of the line is not.
  expect([...on.querySelectorAll("strong")].map((node) => node.textContent)).toEqual([
    "守護",
    "鐵甲騎士",
    "守護",
  ])
  expect(off.querySelector("strong")).toBeNull()
  expect(screen.getByRole("tooltip")).toHaveTextContent("加粗卡文中標為術語的字")
  await userEvent.keyboard("{Escape}")
  expect(screen.queryByRole("tooltip")).toBeNull()
})

it("refreshes estimates on demand, leaves caches untouched when clearing is cancelled, and shows request failure", async () => {
  const storage = memoryCache()
  vi.stubGlobal("caches", storage)
  const cache = await storage.open("sve-image-responses-v2")
  await cache.put(
    "https://cdn.test/images/card_l/1.webp?v=1",
    new Response(new Uint8Array(1024 * 1024)),
  )
  await renderInRouter(<SettingsPage />)
  expect(await screen.findByText("1 MiB")).toBeInTheDocument()
  expect(screen.getByText("無法估算")).toBeInTheDocument()
  await cache.put(
    "https://cdn.test/images/card_l/2.webp?v=1",
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
    "https://cdn.test/images/card_l/synthetic.webp?v=1",
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
    flavorDisplay: "none",
    qaDisplay: "original",
    backgroundDownloads: false,
    symbolLabels: true,
    termEmphasis: false,
    theme: "dark",
    accent: "red",
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
  await userEvent.click(screen.getByRole("button", { name: "恢復預設" }))
  await userEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", { name: "恢復預設" }),
  )
  // Everything returns to the defaults except the independent background-download setting.
  const expected = { ...DEFAULT_PREFS, uiLanguage: "ja" as const, backgroundDownloads: false }
  expect(readPrefs()).toEqual(expected)
  expect(prefsStore.get()).toEqual(expected)
  expect(recentStore.get()).toEqual(recent)
  expect(await cache.keys()).toHaveLength(1)
})

it("shows flavor text by default and remembers hiding it among the text settings", async () => {
  const view = await renderInRouter(<SettingsPage />)
  const flavor = () => screen.getByRole("radiogroup", { name: "風味文字" })
  expect(within(flavor()).getByRole("radio", { name: "翻譯＋原文" })).toHaveAttribute(
    "aria-checked",
    "true",
  )
  // Only flavor text and Q&A can be hidden; names and effect text always show.
  for (const name of ["卡名顯示", "效果文字"])
    expect(
      within(screen.getByRole("radiogroup", { name })).queryByRole("radio", { name: "不顯示" }),
    ).toBeNull()
  await userEvent.click(within(flavor()).getByRole("radio", { name: "不顯示" }))
  expect(readPrefs().flavorDisplay).toBe("none")
  view.unmount()
  prefsStore.reload()
  await renderInRouter(<SettingsPage />)
  expect(within(flavor()).getByRole("radio", { name: "不顯示" })).toHaveAttribute(
    "aria-checked",
    "true",
  )
})

it("shows live download, pause, failed writes and completion only while the selected bytes remain saved", async () => {
  let hidden = false
  vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden)
  const estimate = vi.fn(() => Promise.resolve({ usage: 1024 * 1024 }))
  vi.stubGlobal("navigator", { onLine: true, storage: { estimate } })
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
    const usage = within(screen.getByRole("region", { name: "儲存空間" }))
    expect(await usage.findByText("1 MiB")).toBeInTheDocument()
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
    expect(estimate).toHaveBeenCalledTimes(1)
    const storage = memoryCache()
    vi.stubGlobal("caches", storage)
    estimate.mockResolvedValue({ usage: 2 * 1024 * 1024 })
    act(() => {
      appBackgroundDownloads.select({ ...selection, manifestHash: "next" })
    })
    expect(await downloads.findByText("已完成")).toBeInTheDocument()
    expect(downloads.getByText("2026-10-24 · 日版")).toBeInTheDocument()
    expect(downloads.getByRole("progressbar")).toHaveAttribute("value", "1")
    expect(await usage.findByText("2 MiB")).toBeInTheDocument()
    await act(async () => {
      await appBackgroundDownloads.recheck()
    })
    expect(estimate).toHaveBeenCalledTimes(2)
    await act(async () => {
      await storage.delete(`sve-raw:${encodeURIComponent(selection.base)}`)
      await appBackgroundDownloads.recheck()
    })
    await waitFor(() => {
      expect(downloads.getByText("尚未全部下載")).toBeInTheDocument()
    })
    expect(downloads.getByText(/部分資料已被移除/u)).toBeInTheDocument()
    expect(estimate).toHaveBeenCalledTimes(2)
  } finally {
    gate.resolve(undefined)
    appBackgroundDownloads.select(null)
  }
})
