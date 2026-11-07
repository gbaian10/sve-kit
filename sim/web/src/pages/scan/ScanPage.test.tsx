import { act, renderHook, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { I18nextProvider } from "react-i18next"
import { MemoryRouter } from "react-router"
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { createScanModelStore, SCAN_PARTS } from "../../data/scan-models"
import { toggleClass } from "../../domain/scan/class-pick"
import type { ScanCard } from "../../domain/scan/search"
import { createI18n, type UiLanguage } from "../../i18n"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"
import { memoryCache } from "../../test-utils/cache"
import { stubCanvas2d } from "../../test-utils/canvas-2d"
import { fakeScanCdn, RUNTIME, SCAN_BASE } from "../../test-utils/scan-publication"
import { closeScanEngine } from "./engine"
import { useScanResults } from "./scan-hooks"
import { ScanPage } from "./ScanPage"

// ONNX Runtime does not run in jsdom: a stand-in answers with the real models' output names. With
// `sees.card`, the finder sees one card in the middle and the recogniser reads it as BP01-001.
const sees = vi.hoisted(() => ({ card: false }))
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
    run: (feeds: { image: Tensor }) => {
      if (model.startsWith("det")) {
        const heat = new Float32Array(1024)
        const corners = new Float32Array(8192)
        if (sees.card) {
          const at = 16 * 32 + 16
          heat[at] = 0.9
          ;[-3, -4.2, 3, -4.2, 3, 4.2, -3, 4.2].forEach((offset, channel) => {
            corners[channel * 1024 + at] = offset
          })
        }
        return Promise.resolve({ heat: { data: heat }, corners: { data: corners } })
      }
      const count = feeds.image.dims[0] ?? 0
      const data = new Float32Array(count * 4)
      if (sees.card) for (let at = 0; at < count; at += 1) data[at * 4] = 1
      return Promise.resolve({ embedding: { data } })
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

/** A back camera whose permission prompt answers when the test says so. */
function stubCamera() {
  const stopped: string[] = []
  let grant: () => void = () => undefined
  let focused: () => void = () => undefined
  let holdFocus = false
  const track = {
    stop: () => {
      stopped.push("video")
    },
    // An optical zoom up to 2×; the presets beyond it scale the picture.
    getCapabilities: () => ({ zoom: { min: 1, max: 2 } }),
    applyConstraints: () =>
      holdFocus
        ? new Promise<void>((resolve) => {
            focused = resolve
          })
        : Promise.resolve(),
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
    holdFocus: () => {
      holdFocus = true
    },
    focus: () => {
      focused()
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
  sees.card = false
  closeScanEngine()
  prefsStore.set(DEFAULT_PREFS)
  vi.restoreAllMocks()
})

const container = () => document.body

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

  it("recognises the card in view, and follows zoom, orientation, class and lock changes", async () => {
    const user = userEvent.setup()
    const camera = stubCamera()
    stubCanvas2d()
    sees.card = true
    vi.spyOn(HTMLVideoElement.prototype, "videoWidth", "get").mockReturnValue(640)
    vi.spyOn(HTMLVideoElement.prototype, "videoHeight", "get").mockReturnValue(480)
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(
      DOMRect.fromRect({ x: 0, y: 0, width: 390, height: 520 }),
    )
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    await page.render()
    await user.click(await screen.findByRole("button", { name: /開啟相機/ }))
    camera.grant()
    expect(await screen.findByText("認出來了", {}, { timeout: 10_000 })).toBeInTheDocument()
    expect(
      await screen.findByRole("link", { name: /BP01-001/ }, { timeout: 10_000 }),
    ).toHaveAttribute("href", "/cards/BP01-001")
    await user.click(screen.getByRole("button", { name: "2×" }))
    expect(screen.getByRole("button", { name: "2×" })).toHaveAttribute("aria-pressed", "true")
    await user.click(screen.getByRole("button", { name: "3×" }))
    expect(container().querySelector("video")?.style.transform).toBe("scale(1.5)")
    await user.click(screen.getByRole("button", { name: "直放" }))
    expect(screen.getByRole("button", { name: "橫放" })).toHaveAttribute("aria-pressed", "true")
    const classes = await screen.findByRole("group", { name: "職業" }, { timeout: 15_000 })
    const [first] = within(classes).getAllByRole("button")
    if (!first) throw new Error("class buttons")
    await user.click(first)
    expect(first).toHaveAttribute("aria-pressed", "true")
    // Tapping the picture locks too.
    await user.click(container().querySelector("video") as HTMLVideoElement)
    expect(screen.getByRole("button", { name: "鎖定結果" })).toHaveAttribute("aria-pressed", "true")
  }, 40_000)

  it("ignores a late answer for an edition the user already switched away from", async () => {
    const user = userEvent.setup()
    const camera = stubCamera()
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    await page.store.download("rec", "en", () => undefined)
    let refuseEnglish: (cause: Error) => void = () => undefined
    const held = {
      ...page.store,
      load: (edition: "jp" | "en") =>
        edition === "en"
          ? new Promise<never>((_, reject) => {
              refuseEnglish = reject
            })
          : page.store.load(edition),
    }
    await renderInRouter(<ScanPage store={held} />, { initialEntries: ["/cards/scan"] })
    await user.click(await screen.findByRole("button", { name: /開啟相機/ }))
    camera.grant()
    await screen.findByRole("button", { name: "鎖定結果" })
    act(() => {
      prefsStore.set({ cardEdition: "en" })
    })
    act(() => {
      prefsStore.set({ cardEdition: "jp" })
    })
    await act(() => new Promise((resolve) => setTimeout(resolve, 50)))
    await act(async () => {
      refuseEnglish(new Error("late"))
      await new Promise((resolve) => setTimeout(resolve, 20))
    })
    expect(camera.stopped).toEqual([])
    expect(screen.getByRole("button", { name: "鎖定結果" })).toBeInTheDocument()
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
  })

  it("does not come back on after an edition switch stopped a camera still starting", async () => {
    const user = userEvent.setup()
    const camera = stubCamera()
    camera.holdFocus()
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    await page.render()
    await user.click(await screen.findByRole("button", { name: /開啟相機/ }))
    camera.grant()
    await act(() => new Promise((resolve) => setTimeout(resolve, 20)))
    // The English index is not on this device: the switch stops the camera and offers the download.
    act(() => {
      prefsStore.set({ cardEdition: "en" })
    })
    expect(await screen.findByRole("button", { name: /下載模型/ })).toBeInTheDocument()
    await act(async () => {
      camera.focus()
      await new Promise((resolve) => setTimeout(resolve, 20))
    })
    expect(screen.queryByRole("button", { name: "鎖定結果" })).not.toBeInTheDocument()
    expect(screen.getByRole("button", { name: /下載模型/ })).toBeInTheDocument()
    // Stopping a track twice is harmless; what matters is that it stopped.
    expect(camera.stopped).toContain("video")
  })

  it("keeps one camera start at a time and closes it when the page is left", async () => {
    const user = userEvent.setup()
    const camera = stubCamera()
    const page = setup()
    for (const part of SCAN_PARTS) await page.store.download(part, "jp", () => undefined)
    let release: () => void = () => undefined
    const gate = new Promise<void>((resolve) => {
      release = resolve
    })
    const slow = {
      ...page.store,
      load: (edition: "jp" | "en") => gate.then(() => page.store.load(edition)),
    }
    const { unmount } = await renderInRouter(<ScanPage store={slow} />, {
      initialEntries: ["/cards/scan"],
    })
    const open = await screen.findByRole("button", { name: /開啟相機|讀取模型/ })
    await user.click(open)
    expect(screen.getByRole("button", { name: /讀取模型|開啟相機/ })).toBeDisabled()
    await act(async () => {
      release()
      await new Promise((resolve) => setTimeout(resolve, 20))
    })
    camera.grant()
    await screen.findByRole("button", { name: "鎖定結果" })
    unmount()
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

describe("scan results", () => {
  it("names a front face from the site and shows another face with its own picture", async () => {
    const i18n = await createI18n("zh-TW")
    const wrapper = ({ children }: { readonly children: ReactNode }) => (
      <I18nextProvider i18n={i18n}>
        <MemoryRouter initialEntries={["/cards/scan"]}>{children}</MemoryRouter>
      </I18nextProvider>
    )
    const face = (f: number): ScanCard => ({
      no: "BP01-001",
      cardId: "c:bp01-001",
      imageId: 1001,
      face: f,
      name: f ? "back" : "front",
      artGroup: "",
      classCode: "",
    })
    const cards = [face(0), face(1)]
    const { result } = renderHook(() => useScanResults(cards, "jp"), { wrapper })
    await waitFor(
      () => {
        expect(result.current.results[0]?.summary?.cardNo).toBe("BP01-001")
      },
      { timeout: 25_000 },
    )
    expect(result.current.results[0]?.name).toBeDefined()
    expect(result.current.results[1]).toMatchObject({ summary: undefined, name: undefined })
    expect(result.current.results[1]?.faceImage).toMatch(/\/images\/card_s\/1001-f1\.webp$/u)
  }, 30_000)
})
