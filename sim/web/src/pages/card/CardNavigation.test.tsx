import { act, fireEvent, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routeState } from "../../app/listEntryState"
import { ToastProvider } from "../../components/ui/Toast"
import { type CardSummary, createSnapshotClient, type SnapshotClient } from "../../data"
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

async function setup() {
  const source = createSnapshotClient("https://cdn.test", {
    cacheStorage: memoryCache(),
    fetch: (url) => {
      const bytes = built.files.get(url.slice("https://cdn.test/".length))
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  await source.load()
  let hold: Promise<void> | undefined
  state.client = {
    ...source,
    fragments: async (key) => {
      const pending = hold
      hold = undefined
      await pending
      return source.fragments(key)
    },
  }
  const { catalogOf } = await import("../../data")
  const snapshot = source.snapshot()
  if (!snapshot) throw new Error("missing synthetic snapshot")
  const catalog = catalogOf(snapshot)
  state.order = ["p:bp01-001", "p:bp01-002", "p:bp01-003"].map((id) => {
    const summary = catalog.summary(id)
    if (!summary) throw new Error("missing synthetic card")
    return summary
  })
  const first = state.order[0]
  if (!first) throw new Error("missing first card")
  const rendered = await renderRoutes(
    [
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
  await screen.findByRole("heading", { name: "試作見習兵" })
  await waitFor(() =>
    expect(document.querySelector("article")).toHaveAttribute("aria-busy", "false"),
  )
  return {
    ...rendered,
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
  prefsStore.set({ ...DEFAULT_PREFS, uiLanguage: "zh-TW" })
  Object.defineProperty(HTMLImageElement.prototype, "decode", {
    configurable: true,
    value: () => Promise.resolve(),
  })
})
afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("card navigation commits", () => {
  it("retains the whole page during A → B → C and rejects late B", async () => {
    const user = userEvent.setup()
    const h = await setup()
    const previousEffect = document.querySelector('[data-field="effect"]')
    expect(previousEffect).toBeInTheDocument()
    const releaseB = h.holdNextRead()
    await user.click(screen.getByRole("button", { name: "下一張" }))
    expect(h.router.state.location.pathname).toBe("/cards/BP01-002")
    expect(document.querySelector('[data-field="effect"]')).toBe(previousEffect)
    expect(screen.getByRole("heading", { name: "試作見習兵" })).toBeInTheDocument()
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("BP01-001")
    expect(screen.getByRole("button", { name: /放大/ })).toBeDisabled()
    expect(screen.queryByText("文字與標註載入中")).not.toBeInTheDocument()
    fireEvent.keyDown(window, { key: "ArrowRight" })
    await screen.findByRole("heading", { name: "試作號令" })
    expect(h.router.state.location.pathname).toContain("/cards/BP01-003/")
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("BP01-003")
    await act(async () => {
      releaseB()
      await new Promise((resolve) => setTimeout(resolve, 50))
    })
    expect(screen.queryByRole("heading", { name: "試作隊長" })).not.toBeInTheDocument()
    expect(screen.getByRole("heading", { name: "試作號令" })).toBeInTheDocument()
    expect(screen.queryByText("文字與標註載入中")).not.toBeInTheDocument()
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
})
