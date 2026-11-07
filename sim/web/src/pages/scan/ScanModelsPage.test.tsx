import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it } from "vitest"

import { createScanModelStore } from "../../data/scan-models"
import type { UiLanguage } from "../../i18n"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { memoryCache } from "../../test-utils/cache"
import {
  fakeScanCdn,
  fullCache,
  RUNTIME,
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
    runtime: RUNTIME,
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

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("ScanModelsPage", () => {
  it("lists the three parts with their versions and downloads them all on request", async () => {
    const user = userEvent.setup()
    await setup().render()
    expect(await within(row("辨識模型")).findByText("未下載")).toBeInTheDocument()
    expect(within(row("找卡模型")).getByText("版本 det-1")).toBeInTheDocument()
    expect(within(row("辨識模型")).getByText("版本 rec-1")).toBeInTheDocument()
    expect(
      within(row("辨識模型")).getByText(/含日版卡片索引：5 張卡，卡表 2026-09-29/),
    ).toBeInTheDocument()
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
    const runtime = row("執行引擎")
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
    expect(within(rec).getByText("版本 rec-2")).toBeInTheDocument()
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
    await page.store.download("runtime", "en", () => undefined)
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
