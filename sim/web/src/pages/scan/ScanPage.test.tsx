import { screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { createScanModelStore, SCAN_PARTS } from "../../data/scan-models"
import { toggleClass } from "../../domain/scan/class-pick"
import type { UiLanguage } from "../../i18n"
import { renderInRouter } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"
import { memoryCache } from "../../test-utils/cache"
import { fakeScanCdn, RUNTIME, SCAN_BASE } from "../../test-utils/scan-publication"
import { ScanPage } from "./ScanPage"

// The page names classes and recognised cards from the snapshot, through the search worker.
const built = await buildSnapshot({
  encodeImage: () => Promise.resolve(new TextEncoder().encode("stub")),
})
beforeEach(() => vi.stubGlobal("Worker", TestAppWorker))
beforeAll(() => {
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const bytes = built.files.get(url.replace(/^.*\/cdn\//u, ""))
    return Promise.resolve(
      bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
})
afterAll(() => {
  vi.unstubAllGlobals()
})

function setup(language?: UiLanguage) {
  const storage = memoryCache()
  const store = createScanModelStore({
    caches: () => storage,
    fetcher: fakeScanCdn().fetcher,
    scanBase: () => SCAN_BASE,
    runtime: RUNTIME,
  })
  return {
    store,
    render: () =>
      renderInRouter(<ScanPage store={store} />, {
        initialEntries: ["/cards/scan"],
        ...(language ? { language } : {}),
      }),
  }
}

describe("ScanPage", () => {
  it("asks to download the models first, downloads them in place, then offers the camera", async () => {
    const user = userEvent.setup()
    await setup().render()
    expect(await screen.findByText(/第一次使用要先下載辨識模型/)).toBeInTheDocument()
    expect(screen.getByRole("link", { name: "管理模型" })).toHaveAttribute(
      "href",
      "/cards/scan/models",
    )
    expect(screen.queryByRole("button", { name: /開啟相機/ })).not.toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: /下載模型/ }))
    expect(await screen.findByRole("button", { name: /開啟相機/ })).toBeEnabled()
    expect(screen.getByText("辨識結果會出現在這裡")).toBeInTheDocument()
  })

  it("starts with the camera offer when the models are already on the device", async () => {
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    await page.render()
    expect(await screen.findByRole("button", { name: /開啟相機/ })).toBeInTheDocument()
    expect(screen.getByRole("link", { name: "模型" })).toHaveAttribute("href", "/cards/scan/models")
  })

  it("switches between portrait and sideways cards", async () => {
    const user = userEvent.setup()
    await setup().render()
    const turn = screen.getByRole("button", { name: "直放" })
    expect(turn).toHaveAttribute("aria-pressed", "false")
    await user.click(turn)
    expect(screen.getByRole("button", { name: "橫放" })).toHaveAttribute("aria-pressed", "true")
  })

  it.each([
    ["ja", "カード認識モデル", "モデルを管理"],
    ["en", "Card scan models", "Manage models"],
  ] as const)("renders in %s", async (language, _, manage) => {
    await setup(language).render()
    expect(await screen.findByRole("link", { name: manage })).toBeInTheDocument()
  })
})

describe("class pick", () => {
  it("means every class when empty, brings neutral along with the first class, and lets neutral go", () => {
    expect(toggleClass([], "elf")).toEqual(["elf", "neutral"])
    expect(toggleClass(["elf", "neutral"], "neutral")).toEqual(["elf"])
    expect(toggleClass(["elf"], "dragon")).toEqual(["elf", "dragon"])
    expect(toggleClass(["elf"], "elf")).toEqual([])
    expect(toggleClass([], "neutral")).toEqual(["neutral"])
  })
})
