import { act, fireEvent, renderHook, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { I18nextProvider } from "react-i18next"
import { MemoryRouter } from "react-router"
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { createScanModelStore } from "../../data/scan-models"
import { decodeDeckCode } from "../../domain/deck/code"
import { createI18n } from "../../i18n"
import { DEFAULT_PREFS, prefsStore, saveScanBackend } from "../../settings"
import { renderInRouter, renderRoutes } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"
import { memoryCache } from "../../test-utils/cache"
import { stubCanvas2d } from "../../test-utils/canvas-2d"
import { fakeScanCdn, RUNTIMES, SCAN_BASE } from "../../test-utils/scan-publication"
import { useScannedDeckShare } from "./deck-share"
import { DeckScanPage } from "./DeckScanPage"
import { closeScanEngine } from "./engine"

// The parts a scan with the CPU engine needs.
const SCAN_PARTS = ["runtime-cpu", "det", "rec"] as const

// ONNX Runtime does not run in jsdom. The stand-in finds no cards, and the test decides when the
// card finder answers, to catch a reading still running when the photo goes away.
const finder = vi.hoisted(() => ({ hold: false, card: false, waiting: [] as (() => void)[] }))
vi.mock("onnxruntime-web/webgpu", () => ({
  env: { wasm: {} },
  InferenceSession: { create: () => Promise.reject(new Error("no adapter")) },
}))
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
        // Cards (four samples each) take turns: a sure BP01-001; a tie of BP01-001 and BP01-002,
        // guessed from the sure one; a tie of BP01-002 and BP01-003, left unsure.
        const count = feeds.image.dims[0] ?? 0
        const data = new Float32Array(count * 4)
        const half = Math.SQRT1_2
        const kinds = [
          [1, 0, 0, 0],
          [half, half, 0, 0],
          [0, half, half, 0],
        ]
        for (let at = 0; at < count; at += 1) data.set(kinds[Math.floor(at / 4) % 3] ?? [], at * 4)
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
  saveScanBackend("cpu")
  closeScanEngine()
  finder.hold = false
  finder.card = false
  // Answer what is still held: an inference that never settles would keep its engine from freeing.
  for (const answer of finder.waiting.splice(0)) answer()
  prefsStore.set(DEFAULT_PREFS)
  vi.restoreAllMocks()
})

// The page names classes and cards from the snapshot, through the search worker.
const built = await buildSnapshot({
  encodeImage: () => Promise.resolve(new TextEncoder().encode("stub")),
})
beforeEach(() => {
  vi.stubGlobal("Worker", TestAppWorker)
  stubCanvas2d()
})
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
    runtimes: RUNTIMES,
  })
  return {
    store,
    render: () =>
      renderInRouter(<DeckScanPage store={store} />, { initialEntries: ["/cards/scan/deck"] }),
  }
}

describe("DeckScanPage", () => {
  it("asks to switch to CPU when WebGPU cannot start", async () => {
    saveScanBackend("webgpu")
    const page = setup()
    for (const part of ["runtime-webgpu", "det", "rec"] as const)
      await page.store.download(part, "jp", () => undefined)
    const user = userEvent.setup()
    const { container } = await page.render()
    const input = container.querySelector<HTMLInputElement>('input[type="file"]:not([capture])')
    if (!input) throw new Error("file input")
    await user.upload(input, new File(["png"], "deck.png", { type: "image/png" }))
    await user.click(await screen.findByRole("button", { name: "開始讀取" }))
    expect(await screen.findByText(/WebGPU 無法啟動，請到模型頁改用 CPU 引擎/)).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "清掉照片" }))
  })

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
    stubCanvas2d()
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
    stubCanvas2d()
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
    stubCanvas2d()
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

  it("lists the photo's cards and lets them be sorted, dimmed, confirmed, corrected and shared", async () => {
    const user = userEvent.setup({ applyAccept: false })
    stubCanvas2d()
    finder.card = true
    const written: string[] = []
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: (text: string) => {
          written.push(text)
          return Promise.resolve()
        },
        read: () => Promise.resolve([{ types: ["text/plain"] }]),
      },
    })
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    const { container } = await page.render()
    const input = container.querySelector<HTMLInputElement>('input[type="file"]:not([capture])')
    if (!input) throw new Error("file input")
    await user.upload(input, new File(["png"], "deck.png", { type: "image/png" }))
    await user.click(await screen.findByRole("button", { name: "開始讀取" }))
    expect(await screen.findByText(/^讀出 \d+ 張/u, {}, { timeout: 20_000 })).toBeInTheDocument()

    const counts = screen.getByRole("group", { name: "顏色（按一下變灰）" })
    const [ok, guess, unsure] = within(counts).getAllByRole("button")
    expect(ok).toHaveTextContent(/認出 [1-9]/u)
    expect(guess).toHaveTextContent(/推測 [1-9]/u)
    expect(unsure).toHaveTextContent(/\?\?\? [1-9]/u)
    await user.click(unsure as HTMLElement)
    expect(unsure).toHaveAttribute("aria-pressed", "false")

    await user.click(screen.getByRole("radio", { name: "依卡名" }))
    expect(screen.getAllByText(/×\d+/u).length).toBeGreaterThan(0)
    await user.click(screen.getByRole("radio", { name: "依編號" }))

    const gap = screen.getByRole("spinbutton", { name: "推測差距上限" })
    await user.clear(gap)
    await user.type(gap, "0.1")
    await user.click(screen.getByRole("button", { name: /全部確認/ }))
    expect(await screen.findByText(/張推測（差距 ≤ 0.10）確認為認出/u)).toBeInTheDocument()
    expect(within(counts).getAllByRole("button")[1]).toHaveTextContent("推測 0")

    // An unsure card: pick its second candidate, then remove another one.
    await user.click(screen.getAllByRole("button", { name: /\?\?\? 可能是/u })[0] as HTMLElement)
    const picker = await screen.findByRole("dialog", { name: "照片裡的這張是？" })
    const choices = within(picker)
      .getAllByRole("button")
      .filter((button) => /\d\.\d\d/u.test(button.textContent))
    await user.click(choices[1] as HTMLElement)
    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "照片裡的這張是？" })).not.toBeInTheDocument()
    })
    const before = screen.getAllByRole("button", { name: /#\d+/u }).length
    await user.click(screen.getAllByRole("button", { name: /#\d+/u })[0] as HTMLElement)
    await user.click(await screen.findByRole("button", { name: /不是卡/ }))
    await waitFor(() => {
      expect(screen.getAllByRole("button", { name: /#\d+/u })).toHaveLength(before - 1)
    })

    // Sharing: a code, a link and the text list, each copied as shown.
    const share = screen.getByRole("region", { name: "存成牌組或分享" })
    expect(
      within(share)
        .getByRole("link", { name: /在組牌頁開啟/ })
        .getAttribute("href"),
    ).toMatch(/^\/decks\/import\?d=/u)
    for (const tab of ["牌組代碼", "分享連結", "文字清單"]) {
      await user.click(within(share).getByRole("radio", { name: tab }))
      const copy = await within(share).findByRole("button", { name: "複製" })
      await waitFor(() => {
        expect(copy).toBeEnabled()
      })
      await user.click(copy)
    }
    expect(written[0]).toMatch(/^SVEK-/u)
    expect(written[1]).toMatch(/\/decks\/import\?d=SVEK-/u)
    expect(written[2]).toMatch(/^# 拍照讀取的牌組\n\d+ BP01-00\d /u)

    // Pasting finds no picture on the clipboard.
    await user.click(screen.getByRole("button", { name: "清掉照片" }))
    await user.click(screen.getByRole("button", { name: "從剪貼簿貼上" }))
    expect(await screen.findByRole("status")).toHaveTextContent("剪貼簿裡沒有圖片")
  }, 60_000)

  it("takes a dropped photo, and explains clipboard and format problems", async () => {
    stubCanvas2d()
    const { container } = await setup().render()
    const zone = screen.getByRole("button", { name: "選擇照片" }).closest("div.border-dashed")
    if (!zone) throw new Error("drop zone")
    const file = new File(["png"], "deck.png", { type: "image/png" })
    fireEvent.dragOver(zone)
    fireEvent.drop(zone, { dataTransfer: { files: [file] } })
    expect(await screen.findByRole("status")).toHaveTextContent("已載入照片 800×600")
    await userEvent.click(screen.getByRole("button", { name: "清掉照片" }))

    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { read: () => Promise.reject(new DOMException("denied", "NotAllowedError")) },
    })
    await userEvent.click(screen.getByRole("button", { name: "從剪貼簿貼上" }))
    expect(await screen.findByRole("status")).toHaveTextContent("讀不到剪貼簿（NotAllowedError）")

    // Neither decoder reads it (as with HEIC on a desktop browser).
    vi.stubGlobal("createImageBitmap", () => Promise.reject(new Error("unsupported")))
    vi.stubGlobal(
      "Image",
      class {
        onerror: (() => void) | null = null
        onload: (() => void) | null = null
        set src(_value: string) {
          setTimeout(() => this.onerror?.(), 0)
        }
      },
    )
    const RealImage = globalThis.Image
    Object.defineProperty(URL, "createObjectURL", { configurable: true, value: () => "blob:x" })
    Object.defineProperty(URL, "revokeObjectURL", { configurable: true, value: () => undefined })
    const input = container.querySelector<HTMLInputElement>('input[type="file"]:not([capture])')
    if (!input) throw new Error("file input")
    await userEvent
      .setup({ applyAccept: false })
      .upload(input, new File(["heic"], "deck.heic", { type: "image/heic" }))
    expect(await screen.findByRole("status")).toHaveTextContent("（image/heic）")
    vi.stubGlobal("Image", RealImage)
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
