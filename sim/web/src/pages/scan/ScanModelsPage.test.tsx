import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"

import { createScanModelStore } from "../../data/scan-models"
import type { UiLanguage } from "../../i18n"
import { DEFAULT_PREFS, prefsStore, saveScanBackend } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { memoryCache } from "../../test-utils/cache"
import {
  fakeScanCdn,
  fullCache,
  RUNTIMES,
  SCAN_BASE,
  scanPublication,
} from "../../test-utils/scan-publication"
import { ScanModelsPage } from "./ScanModelsPage"

function setup(
  options: {
    cdn?: ReturnType<typeof fakeScanCdn>
    storage?: CacheStorage
    language?: UiLanguage
  } = {},
) {
  const cdn = options.cdn ?? fakeScanCdn()
  const storage = options.storage ?? memoryCache()
  const store = createScanModelStore({
    caches: () => storage,
    fetcher: cdn.fetcher,
    scanBase: () => SCAN_BASE,
    runtimes: RUNTIMES,
  })
  return {
    cdn,
    store,
    render: () =>
      renderInRouter(<ScanModelsPage store={store} />, {
        initialEntries: ["/cards/scan/models"],
        ...(options.language ? { language: options.language } : {}),
      }),
  }
}

const row = (name: string) => screen.getByRole("listitem", { name })
// The engine card appears once the chosen engine is read from settings.
const engineRow = () => screen.findByRole("listitem", { name: "執行引擎" })

afterEach(() => {
  saveScanBackend("cpu")
  prefsStore.set(DEFAULT_PREFS)
  Reflect.deleteProperty(navigator, "gpu")
  vi.restoreAllMocks()
})

describe("ScanModelsPage", () => {
  it("marks WebGPU unavailable and prevents selecting or downloading it without an adapter", async () => {
    await setup().render()
    const engine = await engineRow()
    const gpu = await within(engine).findByRole("radio", { name: /WebGPU/ })
    expect(gpu).toHaveTextContent("此瀏覽器不支援")
    expect(gpu).toBeDisabled()
    expect(within(engine).getByRole("radio", { name: /CPU/ })).toBeChecked()
  })

  it("downloads only the selected engine and retains it when switching backends", async () => {
    saveScanBackend("webgpu")
    Object.defineProperty(navigator, "gpu", {
      configurable: true,
      value: { requestAdapter: () => Promise.resolve({}) },
    })
    const user = userEvent.setup()
    const page = setup()
    await page.render()
    const engine = await engineRow()
    expect(await within(engine).findByRole("radio", { name: /WebGPU/ })).toBeChecked()
    await user.click(screen.getByRole("button", { name: "全部下載" }))
    expect(await screen.findByText("全部已下載，之後拍卡不用再下載")).toBeInTheDocument()
    expect((await page.store.status("jp")).parts["runtime-cpu"].state).toBe("missing")
    await user.click(within(engine).getByRole("radio", { name: /CPU/ }))
    expect(within(engine).getByRole("radio", { name: /CPU/ })).toBeChecked()
    expect(within(engine).getByText("未下載")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "全部下載" }))
    expect(await within(engine).findByText("已下載")).toBeInTheDocument()
    // The engine left behind stays on the device until removed from the same card.
    expect(within(engine).getByText(/另一個引擎（WebGPU）也存在這台裝置/)).toBeInTheDocument()
    await user.click(within(engine).getAllByRole("button", { name: "刪除" })[0] as HTMLElement)
    await waitFor(() => {
      expect(within(engine).queryByText(/另一個引擎/)).not.toBeInTheDocument()
    })
    expect(within(engine).getByText("已下載")).toBeInTheDocument()
  })

  it("lists the three parts with their versions and downloads them all on request", async () => {
    const user = userEvent.setup()
    await setup().render()
    expect(await within(row("辨識模型")).findByText("未下載")).toBeInTheDocument()
    expect(within(row("找卡模型")).getByText("版本 det-1")).toBeInTheDocument()
    expect(within(row("辨識模型")).getByText("模型 rec-1")).toBeInTheDocument()
    expect(within(row("辨識模型")).getByText("日版卡片索引")).toBeInTheDocument()
    expect(within(row("辨識模型")).getByText(/5 個版次 · 卡表 2026-09-29/)).toBeInTheDocument()
    expect(screen.getByText(/^需要下載/)).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: "全部下載" }))
    expect(await screen.findByText("全部已下載，之後拍卡不用再下載")).toBeInTheDocument()
    for (const name of ["執行引擎", "找卡模型", "辨識模型"])
      expect(within(row(name)).getByText("已下載")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "全部下載" })).toBeDisabled()
  })

  it("deletes one part and lets it be downloaded again", async () => {
    const user = userEvent.setup()
    const page = setup()
    await page.store.download("det", "jp", () => undefined)
    await page.render()
    const det = row("找卡模型")
    await user.click(await within(det).findByRole("button", { name: "刪除" }))
    expect(await within(det).findByRole("button", { name: "下載" })).toBeInTheDocument()
    expect(within(det).getByText("未下載")).toBeInTheDocument()
  })

  it("discards a download whose hash does not match and says so", async () => {
    const user = userEvent.setup()
    const cdn = fakeScanCdn()
    cdn.corrupt(scanPublication().manifest.det.file)
    await setup({ cdn }).render()
    const det = row("找卡模型")
    await user.click(await within(det).findByRole("button", { name: "下載" }))
    expect(await within(det).findByRole("alert")).toHaveTextContent("驗證不符")
    expect(within(det).getByText("未下載")).toBeInTheDocument()
  })

  it("says when the device is out of space", async () => {
    const user = userEvent.setup()
    await setup({ storage: fullCache() }).render()
    const runtime = await engineRow()
    await user.click(await within(runtime).findByRole("button", { name: "下載" }))
    expect(await within(runtime).findByRole("alert")).toHaveTextContent("裝置空間不足")
  })

  it("offers an update when a newer model is published", async () => {
    const page = setup()
    await page.store.download("rec", "jp", () => undefined)
    page.cdn.publish(scanPublication({ rec: 2 }))
    await page.render()
    const rec = row("辨識模型")
    expect(await within(rec).findByText("有新版本")).toBeInTheDocument()
    expect(within(rec).getByText("模型 rec-2")).toBeInTheDocument()
    expect(within(rec).getByRole("button", { name: "更新" })).toBeEnabled()
  })

  it("asks for a site update instead of downloading a list it cannot read", async () => {
    await setup({ cdn: fakeScanCdn(scanPublication({ format: 2 })) }).render()
    expect(await screen.findByRole("alert")).toHaveTextContent("新格式")
    await waitFor(() => {
      expect(within(row("找卡模型")).getByRole("button", { name: "下載" })).toBeDisabled()
    })
    expect(within(row("執行引擎")).getByRole("button", { name: "下載" })).toBeEnabled()
  })

  it("does not claim everything is downloaded while an edition's index is unpublished", async () => {
    prefsStore.set({ cardEdition: "en" })
    const page = setup({ cdn: fakeScanCdn(scanPublication({ en: null })) })
    await page.store.download("runtime-cpu", "en", () => undefined)
    await page.store.download("det", "en", () => undefined)
    await page.render()
    expect(await screen.findByText("部分模型目前無法下載")).toBeInTheDocument()
    expect(within(row("辨識模型")).getByText("英版卡片索引還沒有發佈。")).toBeInTheDocument()
  })

  it.each([
    ["ja", "カード認識モデル", "認識モデル"],
    ["en", "Card scan models", "Recogniser"],
  ] as const)("renders in %s", async (language, title, rec) => {
    await setup({ language }).render()
    expect(await screen.findByRole("heading", { name: title })).toBeInTheDocument()
    expect(row(rec)).toBeInTheDocument()
  })
})
