import { act, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { createScanModelStore, SCAN_PARTS } from "../../data/scan-models"
import { toggleClass } from "../../domain/scan/class-pick"
import type { UiLanguage } from "../../i18n"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"
import { memoryCache } from "../../test-utils/cache"
import { fakeScanCdn, RUNTIME, SCAN_BASE } from "../../test-utils/scan-publication"
import { closeScanEngine } from "./engine"
import { ScanPage } from "./ScanPage"

// ONNX Runtime does not run in jsdom: a stand-in answers with the real models' output names.
vi.mock("onnxruntime-web/wasm", () => {
  class Tensor {
    readonly data: Float32Array
    readonly dims: readonly number[]
    constructor(_type: string, data: Float32Array, dims: readonly number[]) {
      this.data = data
      this.dims = dims
    }
  }
  const session = (model: string) => ({
    outputNames: model.startsWith("det") ? ["heat", "corners"] : ["embedding"],
    run: () =>
      Promise.resolve(
        model.startsWith("det")
          ? { heat: { data: new Float32Array(1024) }, corners: { data: new Float32Array(8192) } }
          : { embedding: { data: new Float32Array(8) } },
      ),
    release: () => Promise.resolve(),
  })
  return {
    env: { wasm: {} },
    Tensor,
    InferenceSession: {
      create: (bytes: Uint8Array) => Promise.resolve(session(new TextDecoder().decode(bytes))),
    },
  }
})

/** A back camera whose permission prompt answers when the test says so. */
function stubCamera() {
  const stopped: string[] = []
  let grant: () => void = () => undefined
  const track = {
    stop: () => {
      stopped.push("video")
    },
    applyConstraints: () => Promise.resolve(),
  }
  const stream = { getTracks: () => [track], getVideoTracks: () => [track] }
  // jsdom has no camera API; give this navigator one for the test.
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: {
      getUserMedia: () =>
        new Promise((resolve) => {
          grant = () => {
            resolve(stream)
          }
        }),
    },
  })
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue()
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockReturnValue()
  return {
    stopped,
    grant: () => {
      grant()
    },
  }
}

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
afterEach(() => {
  closeScanEngine()
  prefsStore.set(DEFAULT_PREFS)
  vi.restoreAllMocks()
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

  it("runs the camera, and locks and resumes the result from the keyboard", async () => {
    const user = userEvent.setup()
    const camera = stubCamera()
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    await page.render()
    await user.click(await screen.findByRole("button", { name: /開啟相機/ }))
    camera.grant()
    const lock = await screen.findByRole("button", { name: "鎖定結果" })
    expect(lock).toHaveAttribute("aria-pressed", "false")
    lock.focus()
    await user.keyboard("{Enter}")
    expect(lock).toHaveAttribute("aria-pressed", "true")
    expect(screen.getByRole("status")).toHaveTextContent("已鎖定，點畫面繼續")
    await user.keyboard(" ")
    expect(lock).toHaveAttribute("aria-pressed", "false")
  })

  it("keeps the camera on when the account menu switches the card edition", async () => {
    const user = userEvent.setup()
    const camera = stubCamera()
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    await page.store.download("rec", "en", () => undefined)
    await page.render()
    await user.click(await screen.findByRole("button", { name: /開啟相機/ }))
    camera.grant()
    await screen.findByRole("button", { name: "鎖定結果" })
    act(() => {
      prefsStore.set({ cardEdition: "en" })
    })
    await act(() => new Promise((resolve) => setTimeout(resolve, 50)))
    expect(screen.getByRole("button", { name: "鎖定結果" })).toBeInTheDocument()
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
    expect(camera.stopped).toEqual([])
  })

  it("closes a camera whose permission arrives after the page was left", async () => {
    const user = userEvent.setup()
    const camera = stubCamera()
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    const { unmount } = await page.render()
    await user.click(await screen.findByRole("button", { name: /開啟相機/ }))
    // Give the engine time to load, so the page is waiting on the permission prompt.
    await act(() => new Promise((resolve) => setTimeout(resolve, 50)))
    unmount()
    camera.grant()
    await act(() => new Promise((resolve) => setTimeout(resolve, 20)))
    expect(camera.stopped).toEqual(["video"])
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
