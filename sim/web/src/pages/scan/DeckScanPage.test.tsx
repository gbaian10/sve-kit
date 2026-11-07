import { act, renderHook, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { I18nextProvider } from "react-i18next"
import { MemoryRouter } from "react-router"
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { createScanModelStore, SCAN_PARTS } from "../../data/scan-models"
import { decodeDeckCode } from "../../domain/deck/code"
import { createI18n } from "../../i18n"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter, renderRoutes } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"
import { memoryCache } from "../../test-utils/cache"
import { fakeScanCdn, RUNTIME, SCAN_BASE } from "../../test-utils/scan-publication"
import { useScannedDeckShare } from "./deck-share"
import { DeckScanPage } from "./DeckScanPage"
import { closeScanEngine } from "./engine"

// ONNX Runtime does not run in jsdom. The stand-in finds no cards, and the test decides when the
// card finder answers, to catch a reading still running when the photo goes away.
const finder = vi.hoisted(() => ({ hold: false, card: false, waiting: [] as (() => void)[] }))
vi.mock("onnxruntime-web/wasm", () => {
  class Tensor {
    readonly data: Float32Array
    readonly dims: readonly number[]
    constructor(_type: string, data: Float32Array, dims: readonly number[]) {
      this.data = data
      this.dims = dims
    }
  }
  // With `card`, every tile shows one card-shaped box in its middle (cell 16, 16).
  const output = () => {
    const heat = new Float32Array(1024)
    const corners = new Float32Array(8192)
    if (finder.card) {
      const at = 16 * 32 + 16
      heat[at] = 0.9
      ;[-3, -4.2, 3, -4.2, 3, 4.2, -3, 4.2].forEach((offset, channel) => {
        corners[channel * 1024 + at] = offset
      })
    }
    return { heat: { data: heat }, corners: { data: corners } }
  }
  const session = (model: string) => ({
    outputNames: model.startsWith("det") ? ["heat", "corners"] : ["embedding"],
    run: (feeds: { image: Tensor }) => {
      if (!model.startsWith("det")) {
        // Every sample reads as the first card of the synthetic index.
        const count = feeds.image.dims[0] ?? 0
        const data = new Float32Array(count * 4)
        for (let at = 0; at < count; at += 1) data[at * 4] = 1
        return Promise.resolve({ embedding: { data } })
      }
      if (!finder.hold) return Promise.resolve(output())
      return new Promise((resolve) => {
        finder.waiting.push(() => {
          resolve(output())
        })
      })
    },
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

/** jsdom has no 2D canvas: a stand-in that draws nothing and reads back blank pixels. */
function stubCanvas() {
  const methods: Record<string, unknown> = {
    getImageData: (_x: number, _y: number, width: number, height: number) => ({
      width,
      height,
      data: new Uint8ClampedArray(width * height * 4),
    }),
    measureText: () => ({ width: 10 }),
  }
  const context = new Proxy(methods, {
    get: (target, key: string): unknown => target[key] ?? (() => undefined),
    set: () => true,
  })
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
    context as unknown as CanvasRenderingContext2D,
  )
  vi.stubGlobal("createImageBitmap", () => Promise.resolve({ width: 800, height: 600 }))
  vi.stubGlobal(
    "ImageData",
    class {
      readonly data: Uint8ClampedArray
      readonly width: number
      readonly height: number
      constructor(data: Uint8ClampedArray, width: number, height: number) {
        this.data = data
        this.width = width
        this.height = height
      }
    },
  )
}

/** A computer camera whose permission prompts answer when the test says so. */
function stubCameras() {
  const stopped: number[] = []
  const grants: (() => void)[] = []
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: {
      getUserMedia: () =>
        new Promise((resolve) => {
          const id = grants.length
          grants.push(() => {
            resolve({
              getTracks: () => [
                {
                  stop: () => {
                    stopped.push(id)
                  },
                },
              ],
            })
          })
        }),
    },
  })
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue()
  return { stopped, grants }
}

afterEach(() => {
  closeScanEngine()
  finder.hold = false
  finder.card = false
  finder.waiting.length = 0
  prefsStore.set(DEFAULT_PREFS)
  vi.restoreAllMocks()
})

// The page names classes and cards from the snapshot, through the search worker.
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
  vi.stubGlobal("matchMedia", () => ({ matches: false }))
})
afterAll(() => {
  vi.unstubAllGlobals()
})

function setup() {
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
      renderInRouter(<DeckScanPage store={store} />, { initialEntries: ["/cards/scan/deck"] }),
  }
}

describe("DeckScanPage", () => {
  it("offers a photo three ways, the class filter, and the model download before reading", async () => {
    const user = userEvent.setup()
    await setup().render()
    for (const name of ["拍照", "選擇照片", "從剪貼簿貼上"])
      expect(screen.getByRole("button", { name })).toBeInTheDocument()
    expect(screen.getByText("電腦上也可以把照片拖進來，或按 Ctrl+V")).toBeInTheDocument()
    expect(
      await screen.findByRole("group", { name: "職業" }, { timeout: 15_000 }),
    ).toBeInTheDocument()
    await user.click(await screen.findByRole("button", { name: /下載模型/ }))
    expect(await screen.findByRole("button", { name: "開始讀取" })).toBeDisabled()
    expect(screen.getByRole("link", { name: "單張" })).toHaveAttribute("href", "/cards/scan")
  }, 30_000)

  it("closes a computer camera that opens after the page was left", async () => {
    const user = userEvent.setup()
    const stopped: string[] = []
    let grant: () => void = () => undefined
    const track = {
      stop: () => {
        stopped.push("video")
      },
    }
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: () =>
          new Promise((resolve) => {
            grant = () => {
              resolve({ getTracks: () => [track] })
            }
          }),
      },
    })
    const { unmount } = await setup().render()
    await user.click(screen.getByRole("button", { name: "拍照" }))
    unmount()
    await act(async () => {
      grant()
      await new Promise((resolve) => setTimeout(resolve, 0))
    })
    expect(stopped).toEqual(["video"])
  })

  it("drops a reading still running when the photo is cleared", async () => {
    const user = userEvent.setup({ applyAccept: false })
    stubCanvas()
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    const { container } = await page.render()
    const input = container.querySelector<HTMLInputElement>('input[type="file"]:not([capture])')
    if (!input) throw new Error("file input")
    await user.upload(input, new File(["png"], "deck.png", { type: "image/png" }))
    finder.hold = true
    await user.click(await screen.findByRole("button", { name: "開始讀取" }))
    await waitFor(() => {
      expect(finder.waiting.length).toBeGreaterThan(0)
    })
    await user.click(screen.getByRole("button", { name: "清掉照片" }))
    finder.hold = false
    await act(async () => {
      for (const answer of finder.waiting.splice(0)) answer()
      await new Promise((resolve) => setTimeout(resolve, 50))
    })
    expect(screen.queryByRole("heading", { name: "存成牌組或分享" })).not.toBeInTheDocument()
    expect(screen.getByRole("button", { name: "選擇照片" })).toBeInTheDocument()
  })

  it("lets a new reading start after switching edition away and back during one", async () => {
    const user = userEvent.setup({ applyAccept: false })
    stubCanvas()
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    const { container } = await page.render()
    const input = container.querySelector<HTMLInputElement>('input[type="file"]:not([capture])')
    if (!input) throw new Error("file input")
    await user.upload(input, new File(["png"], "deck.png", { type: "image/png" }))
    finder.hold = true
    await user.click(await screen.findByRole("button", { name: "開始讀取" }))
    await waitFor(() => {
      expect(finder.waiting.length).toBeGreaterThan(0)
    })
    act(() => {
      prefsStore.set({ cardEdition: "en" })
    })
    act(() => {
      prefsStore.set({ cardEdition: "jp" })
    })
    expect(await screen.findByRole("button", { name: "開始讀取" })).toBeEnabled()
    // The page keeps its photo for the page load; leave none for the next test.
    await user.click(screen.getByRole("button", { name: "清掉照片" }))
  })

  it("reopens the candidates on coming back from a card page, and Back then closes them", async () => {
    const user = userEvent.setup({ applyAccept: false })
    stubCanvas()
    finder.card = true
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    const { container, router } = await renderRoutes(
      [
        { path: "/cards", element: <p>cards</p> },
        { path: "/cards/scan/deck", element: <DeckScanPage store={page.store} /> },
        { path: "/cards/:cardNo", element: <p>card page</p> },
      ],
      { initialEntries: ["/cards", "/cards/scan/deck"] },
    )
    const input = container.querySelector<HTMLInputElement>('input[type="file"]:not([capture])')
    if (!input) throw new Error("file input")
    await user.upload(input, new File(["png"], "deck.png", { type: "image/png" }))
    await user.click(await screen.findByRole("button", { name: "開始讀取" }))
    await user.click(await screen.findByRole("button", { name: /#1\b/u }, { timeout: 15_000 }))
    const picker = await screen.findByRole("dialog", { name: "照片裡的這張是？" })
    await user.click(within(picker).getAllByRole("link", { name: /卡片頁/ })[0] as HTMLElement)
    expect(await screen.findByText("card page")).toBeInTheDocument()
    await act(() => router.navigate(-1))
    expect(await screen.findByRole("dialog", { name: "照片裡的這張是？" })).toBeInTheDocument()
    await act(() => router.navigate(-1))
    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "照片裡的這張是？" })).not.toBeInTheDocument()
    })
    expect(router.state.location.pathname).toBe("/cards/scan/deck")
    await act(() => router.navigate(-1))
    expect(router.state.location.pathname).toBe("/cards")
    // The page keeps its photo for the page load; leave none for the next test.
    await act(() => router.navigate("/cards/scan/deck"))
    await user.click(await screen.findByRole("button", { name: "清掉照片" }))
  }, 40_000)

  it("shows the computer camera in a modal and closes it on leaving the page", async () => {
    const user = userEvent.setup()
    const cameras = stubCameras()
    const { unmount } = await setup().render()
    await user.click(screen.getByRole("button", { name: "拍照" }))
    await act(async () => {
      cameras.grants[0]?.()
      await new Promise((resolve) => setTimeout(resolve, 0))
    })
    const dialog = await screen.findByRole("dialog", { name: "對準整副牌，按「拍下」" })
    expect(dialog.tagName).toBe("DIALOG")
    expect(dialog).toContainElement(document.activeElement as HTMLElement)
    unmount()
    expect(cameras.stopped).toEqual([0])
  })

  it("closes every camera a repeated request opened", async () => {
    const user = userEvent.setup()
    const cameras = stubCameras()
    await setup().render()
    await user.click(screen.getByRole("button", { name: "拍照" }))
    await user.click(screen.getByRole("button", { name: "拍照" }))
    await act(async () => {
      cameras.grants[0]?.()
      cameras.grants[1]?.()
      await new Promise((resolve) => setTimeout(resolve, 0))
    })
    await user.click(await screen.findByRole("button", { name: "取消" }))
    expect([...cameras.stopped].sort()).toEqual([0, 1])
  })

  it("says a picked file is not an image", async () => {
    const user = userEvent.setup({ applyAccept: false })
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    const { container } = await page.render()
    const input = container.querySelector<HTMLInputElement>('input[type="file"]:not([capture])')
    if (!input) throw new Error("file input")
    await user.upload(input, new File(["text"], "deck.txt", { type: "text/plain" }))
    expect(await screen.findByRole("status")).toHaveTextContent("這不是圖片")
  })
})

describe("scanned deck share", () => {
  it("makes no code when some cards are not in the card data", async () => {
    const i18n = await createI18n("zh-TW")
    const wrapper = ({ children }: { readonly children: ReactNode }) => (
      <I18nextProvider i18n={i18n}>
        <MemoryRouter initialEntries={["/cards/scan/deck"]}>{children}</MemoryRouter>
      </I18nextProvider>
    )
    const text = "# 拍照讀取的牌組\n3 BP01-001 name\n2 BP99-999 unknown\n"
    const { result } = renderHook(() => useScannedDeckShare(text, "jp", "拍照讀取的牌組"), {
      wrapper,
    })
    await waitFor(
      () => {
        expect(result.current.missing).toEqual(["BP99-999"])
      },
      { timeout: 25_000 },
    )
    expect(result.current.code).toBe("")
    expect(result.current.text).toBe(text)
  }, 30_000)

  it("makes a share code the import page reads back as the same cards", async () => {
    const i18n = await createI18n("zh-TW")
    const wrapper = ({ children }: { readonly children: ReactNode }) => (
      <I18nextProvider i18n={i18n}>
        <MemoryRouter initialEntries={["/cards/scan/deck"]}>{children}</MemoryRouter>
      </I18nextProvider>
    )
    const text = "# 拍照讀取的牌組\n3 BP01-001 name\n"
    const { result } = renderHook(() => useScannedDeckShare(text, "jp", "拍照讀取的牌組"), {
      wrapper,
    })
    await waitFor(
      () => {
        expect(result.current.code).not.toBe("")
      },
      { timeout: 25_000 },
    )
    const payload = decodeDeckCode(result.current.code)
    expect(payload).toMatchObject({ edition: "jp", name: "拍照讀取的牌組" })
    expect(payload.main).toEqual([{ intId: 1001, count: 3 }])
  }, 30_000)
})
