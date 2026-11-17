import { act, fireEvent, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routeState } from "../../app/listEntryState"
import { ToastProvider } from "../../components/ui/Toast"
import { type CardSummary, catalogOf, type SnapshotClient } from "../../data"
import { createSnapshotClient } from "../../data/client"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { memoryCache } from "../../test-utils/cache"
import { CardPage } from "./CardPage"

const state = vi.hoisted(() => ({
  client: null as SnapshotClient | null,
  order: [] as CardSummary[],
}))
vi.mock("../../app/other-edition", () => ({ useOtherEdition: () => ({ retry: () => undefined }) }))
vi.mock("../../app/snapshot", async (original) => ({
  ...(await original<typeof import("../../app/snapshot")>()),
  useCardSnapshot: () => ({ client: state.client, status: { state: "ready" } }),
  useRouteEdition: () => "jp",
  useSearchReadiness: () => ({ phase: "ready", retry: () => undefined }),
  useWorkerSuggestions: () => ({
    lastInput: null,
    result: { phase: "pending" },
    images: undefined,
  }),
  useNeighbours: () => ({
    result: {
      phase: "complete",
      page: {
        items: state.order.map((summary) => ({
          key: summary.printingId,
          printingId: summary.printingId,
          summary,
        })),
      },
    },
  }),
}))

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })

let sequence = 0
async function setup(pendingCandidate = false, blockBackground = false) {
  const base = `https://navigation-${String(sequence++)}.test`
  const source = createSnapshotClient(base, {
    cacheStorage: memoryCache(),
    fetch: (url) => {
      const bytes = built.files.get(url.slice(base.length + 1))
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  await source.load()
  let snapshot = source.snapshot()
  if (!snapshot) throw new Error("missing synthetic snapshot")
  const historyKey = [...snapshot.files].find(
    ([, file]) => file["purpose"] === "history" && file["region"] === "jp",
  )?.[0]
  if (!historyKey) throw new Error("missing synthetic history")
  if (pendingCandidate) {
    const history = await source.fragments(historyKey)
    const revision = history
      .flatMap((part) => (part.table === "face_revision" ? part.rows : []))
      .find((row) => row["face_id"] === "f:bp01-021")
    if (!revision) throw new Error("missing historical synthetic revision")
    snapshot = {
      ...snapshot,
      bootstrap: snapshot.bootstrap.map((part) => ({
        ...part,
        rows: part.rows.map((row) =>
          part.table === "face" && row["id"] === "f:bp01-021"
            ? {
                ...row,
                current: [],
                wording: [
                  {
                    region: "jp",
                    state: "pending",
                    display: { revision_id: null, basis: "candidates" },
                    candidates: [
                      { printing_id: "p:bp01-021", revision_id: revision["id"] ?? null },
                    ],
                    undated_printing_ids: [],
                    common_name: null,
                  },
                ],
              }
            : row,
        ),
      })),
    }
  }
  let hold: Promise<void> | undefined
  let historyHold: Promise<void> | undefined
  let historyFailure = false
  let failedPurposes = new Set<string>()
  const backgroundReads: string[] = []
  const backgroundSignals: AbortSignal[] = []
  let releaseBackground: () => void = () => undefined
  const backgroundHold = new Promise<void>((resolve) => {
    releaseBackground = resolve
  })
  let pinnedSnapshot = snapshot
  let foregroundReads = 0
  let inflightReads = 0
  state.client = {
    ...source,
    snapshot: () => pinnedSnapshot,
    selectRegion: async (region) => {
      const pending = hold
      hold = undefined
      await pending
      await source.selectRegion?.(region)
    },
    fragments: async (key, backgroundSignal) => {
      if (backgroundSignal) {
        backgroundReads.push(key)
        backgroundSignals.push(backgroundSignal)
        if (blockBackground) await backgroundHold
        backgroundSignal.throwIfAborted()
      } else {
        foregroundReads += 1
        inflightReads += 1
      }
      try {
        if (pendingCandidate && key === historyKey) {
          await historyHold
          if (historyFailure) throw new Error("synthetic candidate history unavailable")
        }
        const purpose = pinnedSnapshot.files.get(key)?.["purpose"]
        if (typeof purpose === "string" && failedPurposes.has(purpose))
          throw new Error("synthetic read failure")
        return await source.fragments(key, backgroundSignal)
      } finally {
        if (!backgroundSignal) inflightReads -= 1
      }
    },
  }
  const catalog = catalogOf(snapshot)
  state.order = ["p:bp01-001", pendingCandidate ? "p:bp01-021" : "p:bp01-002", "p:bp01-003"].map(
    (id) => {
      const summary = catalog.summary(id)
      if (!summary) throw new Error("missing synthetic card")
      return summary
    },
  )
  const first = state.order[0]
  if (!first) throw new Error("missing first card")
  const rendered = await renderRoutes(
    [
      { path: "/elsewhere", element: <p>Elsewhere</p> },
      {
        path: "/cards/:cardNo/:slug?",
        element: (
          <ToastProvider>
            <CardPage />
          </ToastProvider>
        ),
      },
    ],
    {
      initialEntries: [
        {
          pathname: "/cards/BP01-001",
          state: {
            background: "",
            resultKey: first.printingId,
            ...routeState(first),
          },
        },
      ],
    },
  )
  await screen.findByRole("heading", { name: "試作見習兵" }, { timeout: 5000 })
  await waitFor(() =>
    expect(document.querySelector("article")).toHaveAttribute("aria-busy", "false"),
  )
  // The page keeps reading after it is shown (printing metadata, related cards, thumbnails); a
  // hold placed by a test must catch the navigation's read, not one of these.
  await act(async () => {
    for (let attempt = 0; attempt < 100; attempt += 1) {
      const before = foregroundReads
      await new Promise((resolve) => setTimeout(resolve, 30))
      if (inflightReads === 0 && foregroundReads === before) return
    }
    throw new Error("synthetic page never settled")
  })
  return {
    ...rendered,
    backgroundReads,
    backgroundSignals,
    releaseBackground,
    nextSnapshot: () => {
      pinnedSnapshot = { ...pinnedSnapshot, manifestHash: "sha256:" + "0".repeat(64) }
    },
    holdHistory: () => {
      let release: () => void = () => undefined
      historyHold = new Promise<void>((resolve) => {
        release = resolve
      })
      return release
    },
    failHistory: (failed: boolean) => {
      historyFailure = failed
    },
    failReads: (purposes: readonly string[]) => {
      failedPurposes = new Set(purposes)
    },
    // Locating the target always starts at selectRegion, so the hold waits there: no read the
    // page already shown makes late (images, relations) can take it, however slow the machine.
    holdNextRead: () => {
      let release: () => void = () => undefined
      hold = new Promise<void>((resolve) => {
        release = resolve
      })
      return release
    },
  }
}

beforeEach(() => {
  prefsStore.set({ ...DEFAULT_PREFS, uiLanguage: "zh-TW", nameDisplay: "translated" })
  Object.defineProperty(HTMLImageElement.prototype, "decode", {
    configurable: true,
    value: () => Promise.resolve(),
  })
})
afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("card navigation commits", () => {
  it("does not restore the retained card from a different snapshot", async () => {
    const h = await setup()
    const first = state.order[0]
    if (!first) throw new Error("missing first card")
    await act(async () => {
      await h.router.navigate("/elsewhere")
    })
    h.nextSnapshot()
    const release = h.holdNextRead()
    await act(async () => {
      await h.router.navigate("/cards/BP01-001", {
        state: { background: "", resultKey: first.printingId, ...routeState(first) },
      })
    })
    expect(screen.queryByRole("heading", { name: "試作見習兵" })).not.toBeInTheDocument()
    act(release)
    await screen.findByRole("heading", { name: "試作見習兵" })
  })

  it("replaces the retained page with a missing-target error and can navigate again", async () => {
    const h = await setup()
    const release = h.holdNextRead()
    await act(async () => {
      await h.router.navigate("/cards/MISSING-999")
    })
    expect(screen.getByRole("heading", { name: "試作見習兵" })).toBeInTheDocument()
    act(() => {
      release()
    })
    await screen.findByRole("heading", { name: "找不到這個卡號" })
    expect(screen.queryByRole("heading", { name: "試作見習兵" })).not.toBeInTheDocument()
    await act(async () => {
      await h.router.navigate("/cards/BP01-003")
    })
    await screen.findByRole("heading", { name: "試作號令" })
  })

  it("commits a located card whose text and image cannot load, then retries them", async () => {
    const user = userEvent.setup()
    const h = await setup()
    h.failReads(["detail", "images"])
    await user.click(screen.getByRole("button", { name: "下一張" }))
    await screen.findByRole("heading", { name: "試作隊長" })
    expect(screen.queryByRole("heading", { name: "無法定位卡片" })).not.toBeInTheDocument()
    expect(screen.getByText("卡圖資訊載入失敗")).toBeInTheDocument()
    expect((await screen.findAllByText("文字載入失敗，請稍後再試。")).length).toBeGreaterThan(0)
    h.failReads([])
    await user.click(screen.getByRole("button", { name: "上一張" }))
    await screen.findByRole("heading", { name: "試作見習兵" })
    await user.click(screen.getByRole("button", { name: "下一張" }))
    await screen.findByRole("heading", { name: "試作隊長" })
    await waitFor(() => {
      expect(screen.queryByText("卡圖資訊載入失敗")).not.toBeInTheDocument()
    })
    expect(screen.queryByText("文字載入失敗，請稍後再試。")).not.toBeInTheDocument()
  })

  it("reports failed candidate history as a target failure and retries the target", async () => {
    const h = await setup(true)
    h.failHistory(true)
    await userEvent.click(screen.getByRole("button", { name: "下一張" }))
    await screen.findByRole("heading", { name: "無法定位卡片" })
    expect(screen.queryByText("另一版次載入失敗")).not.toBeInTheDocument()
    h.failHistory(false)
    await userEvent.click(screen.getByRole("button", { name: "重試" }))
    await screen.findByRole("heading", { name: "試作魔導士" })
    expect(screen.getByRole("listitem", { name: "消費 4" })).toBeInTheDocument()
  })
})

it("waits for the current image before preparing a neighbour, then switches with its full prepared content", async () => {
  prefsStore.set({ preloadNeighbours: true })
  let release: () => void = () => undefined
  const currentDecode = new Promise<void>((resolve) => {
    release = resolve
  })
  const decode = vi
    .fn()
    .mockImplementationOnce(() => currentDecode)
    .mockResolvedValue(undefined)
  Object.defineProperty(HTMLImageElement.prototype, "decode", { configurable: true, value: decode })
  const h = await setup()
  expect(h.backgroundReads).toHaveLength(0)
  await act(async () => {
    release()
    await currentDecode
    const image = document.querySelector("article img[decoding]")
    if (!image) throw new Error("missing first card image")
    fireEvent.load(image)
  })
  await waitFor(() => {
    expect(h.backgroundReads.length).toBeGreaterThan(0)
  })
  await waitFor(() => {
    expect(decode).toHaveBeenCalledTimes(2)
  })
  await act(async () => {
    await Promise.resolve()
  })
  h.failReads(["print", "detail", "images", "history"])
  await userEvent.click(screen.getByRole("button", { name: "下一張" }))
  await screen.findByRole("heading", { name: "試作隊長" })
  expect(screen.queryByText("文字與標註載入中")).not.toBeInTheDocument()
  expect(screen.queryByText("卡圖資訊載入失敗")).not.toBeInTheDocument()
})

it("discards a neighbour still preparing when the switch is turned off", async () => {
  prefsStore.set({ preloadNeighbours: true })
  let release: () => void = () => undefined
  const neighbourDecode = new Promise<void>((resolve) => {
    release = resolve
  })
  const decode = vi
    .fn()
    .mockResolvedValueOnce(undefined)
    .mockImplementationOnce(() => neighbourDecode)
    .mockResolvedValue(undefined)
  Object.defineProperty(HTMLImageElement.prototype, "decode", { configurable: true, value: decode })
  const h = await setup()
  await waitFor(() => {
    expect(decode).toHaveBeenCalledTimes(2)
  })
  act(() => {
    prefsStore.set({ preloadNeighbours: false })
  })
  await act(async () => {
    release()
    await neighbourDecode
  })
  h.failReads(["print", "detail", "images", "history"])
  await userEvent.click(screen.getByRole("button", { name: "下一張" }))
  await screen.findByRole("heading", { name: "試作隊長" })
  expect(await screen.findByText("卡圖資訊載入失敗")).toBeInTheDocument()
})

it.each(["button", "keyboard"])(
  "cancels background work at %s navigation intent until the target commits",
  async (action) => {
    prefsStore.set({ preloadNeighbours: true, preloadPrintings: true })
    const h = await setup(false, true)
    try {
      await waitFor(() => {
        expect(h.backgroundSignals.length).toBeGreaterThan(0)
      })
      const releaseTarget = h.holdNextRead()
      const observed: boolean[] = []
      const unsubscribe = h.router.subscribe((router) => {
        if (router.location.pathname !== "/cards/BP01-002" || observed.length) return
        observed.push(h.backgroundSignals.every((signal) => signal.aborted))
        state.order = state.order.map((summary) => ({ ...summary }))
        prefsStore.set({ theme: "light" })
      })
      if (action === "button") fireEvent.click(screen.getByRole("button", { name: "下一張" }))
      else fireEvent.keyDown(window, { key: "ArrowRight" })
      expect(observed).toEqual([true])
      expect(screen.getByRole("heading", { name: "試作見習兵" })).toBeInTheDocument()
      expect(h.backgroundSignals.every((signal) => signal.aborted)).toBe(true)
      act(releaseTarget)
      await screen.findByRole("heading", { name: "試作隊長" })
      h.releaseBackground()
      await waitFor(() => {
        expect(h.backgroundSignals.some((signal) => !signal.aborted)).toBe(true)
      })
      unsubscribe()
    } finally {
      h.releaseBackground()
    }
  },
)

describe("history stack", () => {
  // The related tabs hold the same cards under different tabs: open each until the link shows up.
  async function relatedLink(user: ReturnType<typeof userEvent.setup>, cardNo: string) {
    const region = await screen.findByRole("region", { name: "關聯卡牌" })
    for (const tab of within(region).getAllByRole("tab")) {
      await user.click(tab)
      const link = region.querySelector<HTMLAnchorElement>(`a[href="/cards/${cardNo}"]`)
      if (link) return link
    }
    throw new Error(`no related link to ${cardNo}`)
  }
  const settled = async (name: string) => {
    await screen.findByRole("heading", { name })
    await waitFor(() =>
      expect(document.querySelector("article")).toHaveAttribute("aria-busy", "false"),
    )
  }

  // Entries reached by a link are replaced once by the canonical address, so they are told apart
  // by walking back from them rather than by history action or key.
  const stepBack = async (h: Awaited<ReturnType<typeof setup>>) => {
    await act(async () => {
      await h.router.navigate(-1)
    })
    return h.router.state.location.pathname
  }

  it("goes back for 002 → 002a → 002 and records the printing, not the card, as previous", async () => {
    const user = userEvent.setup()
    const h = await setup()
    await user.click(screen.getByRole("button", { name: "下一張" }))
    await settled("試作隊長")
    await user.click(await relatedLink(user, "BP01-002a"))
    await waitFor(() => {
      expect(h.router.state.location.pathname).toContain("/cards/BP01-002a")
    })
    expect(h.router.state.location.state).toMatchObject({ previous: "p:bp01-002" })
    await user.click(await relatedLink(user, "BP01-001"))
    await settled("試作見習兵")
    // 001 was reached from 002a: a link to 002 (the same card) would not be a way back.
    expect(h.router.state.location.state).toMatchObject({ previous: "p:bp01-002-alt" })
    expect(await stepBack(h)).toContain("/cards/BP01-002a")
    expect(await stepBack(h)).toContain("/cards/BP01-002")
    await user.click(await relatedLink(user, "BP01-002a"))
    await waitFor(() => {
      expect(h.router.state.location.pathname).toContain("/cards/BP01-002a")
    })
    await user.click(await relatedLink(user, "BP01-002"))
    await waitFor(() => {
      expect(h.router.state.location.pathname).not.toContain("002a")
    })
    // Back landed on the first entry again, so the 002a entry is still ahead.
    await act(async () => {
      await h.router.navigate(1)
    })
    expect(h.router.state.location.pathname).toContain("/cards/BP01-002a")
  })
})
