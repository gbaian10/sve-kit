import { act, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { ToastProvider } from "../../components/ui/Toast"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { CardPage } from "./CardPage"

// The page shows a toast after copying its link, so it needs the shell's provider around it.
const card = {
  element: (
    <ToastProvider>
      <CardPage />
    </ToastProvider>
  ),
}

const state = vi.hoisted(() => ({
  available: false,
  current: false,
  crossRegion: false,
  doubleFace: false,
  order: null as { key: string; printingId: string }[] | null,
  undated: [{ id: "p:undated", cardNo: "PR-SYNTHETIC" }],
  wording: vi.fn(),
}))
vi.mock("../../app/snapshot", () => {
  const printing = () => ({
    id: "p:synthetic",
    card_id: "c:synthetic",
    region: state.crossRegion ? "en" : "jp",
    int_id: 42,
  })
  return {
    useImageIndex: () => undefined,
    // The exact-owner name never resolves here, so the heading shows the catalog stand-in.
    useAnnotatedText: () => ({
      readiness: { basic: "pending", source: "pending", annotations: "pending" },
      value: null,
    }),
    useSearchReadiness: () => ({
      phase: "ready",
      generation: 1,
      updating: false,
      retry: () => undefined,
    }),
    useCatalog: () => ({
      client: { snapshot: () => null },
      catalog: {
        // Previous/next walk this order; the synthetic card is first, so only "next" is enabled.
        results: () =>
          state.order ??
          (state.doubleFace
            ? [
                { key: "c:synthetic", printingId: "p:synthetic" },
                { key: "c:other", printingId: "p:synthetic" },
              ]
            : []),
        summary: () =>
          state.doubleFace
            ? {
                cardId: "c:synthetic",
                printingId: "p:synthetic",
                faceId: "f:synthetic",
                cardNo: "BP-SYNTHETIC",
                region: "jp",
                classCode: null,
                name: { original: { lang: "ja", text: "Synthetic front" }, translations: {} },
                cost: 1,
                attack: 1,
                defense: 1,
              }
            : undefined,
        index: {
          cards: [{ id: "c:synthetic" }],
          card: () => ({ id: "c:synthetic", home_set_id: "BP-SYNTHETIC" }),
          family: () => ({ id: "BP-SYNTHETIC", code: "bp-synthetic", public_code: "BP-SYN" }),
          printingsOf: () => [printing()],
          printingByCardNo: (region: string) =>
            state.crossRegion && region === "jp" ? undefined : printing(),
          facesOf: () => [{ id: "f:synthetic" }, ...(state.doubleFace ? [{ id: "f:back" }] : [])],
          wording: (face: string, region: string) => {
            state.wording(face, region)
            return {
              display: {
                basis: state.current
                  ? "current"
                  : state.available
                    ? "latest_known_release"
                    : "candidates",
              },
              undated_printing_ids: state.undated.map((printing) => printing.id),
            }
          },
          displayRevision: (face: string) =>
            state.doubleFace
              ? {
                  id: face === "f:back" ? "rev:back" : "rev:front",
                  name_unit_id: face === "f:back" ? "t:back" : "t:front",
                  cost: 2,
                  translations:
                    face === "f:back"
                      ? [
                          {
                            field: "name",
                            target_lang: "zh-Hant",
                            translation_id: "tr:back",
                            basis: "jp_source",
                          },
                        ]
                      : [],
                }
              : state.available
                ? { name_unit_id: "t:synthetic" }
                : undefined,
          translation: () => ({ text_unit_id: "t:back-translated", low_confidence: true }),
          vocabulary: () => undefined,
          textUnit: (id: string) => ({
            lang: "ja",
            text:
              id === "t:back"
                ? "Synthetic back"
                : id === "t:back-translated"
                  ? "合成背面譯名"
                  : "Synthetic presentation name",
          }),
          printing: (id: string) => {
            const printing = state.undated.find((printing) => printing.id === id)
            return printing ? { card_no: printing.cardNo } : undefined
          },
        },
      },
    }),
  }
})

beforeEach(() => {
  state.available = false
  state.current = false
  state.crossRegion = false
  state.doubleFace = false
  state.order = null
  state.undated = [{ id: "p:undated", cardNo: "PR-SYNTHETIC" }]
  state.wording.mockClear()
})

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("pending wording on the card page", () => {
  it.each([
    ["zh-TW", "卡文未定", "依已知發售日暫顯"],
    ["ja", "表記未定", "既知の発売日に基づく仮表示"],
    ["en", "Wording pending", "Provisional display by known release date"],
  ] as const)(
    "shows visible pending and release-day labels in %s",
    async (language, pending, provisional) => {
      state.available = true
      await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
        initialEntries: ["/cards/BP-SYNTHETIC"],
        language,
      })
      const section = screen.getByRole("region", { name: pending })
      expect(screen.getByRole("heading", { name: pending })).toBeVisible()
      expect(section).toHaveTextContent("Synthetic presentation name")
      expect(screen.getByText(new RegExp(provisional))).toBeVisible()
      expect(screen.getByText("PR-SYNTHETIC")).toHaveClass("whitespace-nowrap")
      expect(screen.queryByText("p:undated")).not.toBeInTheDocument()
    },
  )
  it.each([
    ["zh-TW", "有多個候選，尚未選定"],
    ["ja", "複数の候補があり、まだ選定されていません"],
    ["en", "Multiple candidates; none selected yet"],
  ] as const)("shows one honest unselected message in %s", async (language, unselected) => {
    await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
      language,
    })
    expect(screen.getAllByText(unselected)).toHaveLength(1)
    expect(
      screen.queryByText(/候選載入中|候補を読み込み中|Loading candidates/),
    ).not.toBeInTheDocument()
  })
  it("keeps a valid current labelled as pending rather than provisional", async () => {
    state.available = true
    state.current = true
    await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByRole("heading", { name: "卡文未定" })).toBeVisible()
    expect(screen.queryByText(/依已知發售日暫顯/)).not.toBeInTheDocument()
  })
  it.each(["zh-TW", "ja", "en"] as const)(
    "uses equal grid tracks sized by the longest card number in %s",
    async (language) => {
      state.undated = [
        { id: "p:suffix-a", cardNo: "BP01-002a" },
        { id: "p:suffix-s", cardNo: "BP01-021S" },
        { id: "p:short", cardNo: "BP01-01" },
        { id: "p:long", cardNo: "PR-SYNTHETIC" },
      ]
      await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
        initialEntries: ["/cards/BP-SYNTHETIC"],
        language,
      })
      const grid = screen.getByRole("list")
      expect(grid).toHaveClass("grid", "font-mono")
      expect(grid).toHaveStyle({ gridTemplateColumns: "repeat(auto-fill, 12ch)" })
      expect(screen.getAllByRole("listitem").map((item) => item.textContent)).toEqual(
        state.undated.map((printing) => printing.cardNo),
      )
      for (const item of screen.getAllByRole("listitem")) {
        expect(item).toHaveClass("text-left", "whitespace-nowrap")
      }
    },
  )
  it.each(["zh-TW", "ja", "en"] as const)(
    "reserves two columns for wide card-number symbols in %s",
    async (language) => {
      state.undated = [
        { id: "p:ascii", cardNo: "BP01-002a" },
        { id: "p:wide", cardNo: "BP01-021Ⓢ" },
        { id: "p:short", cardNo: "BP01-01" },
      ]
      await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
        initialEntries: ["/cards/BP-SYNTHETIC"],
        language,
      })
      expect(screen.getByRole("list")).toHaveStyle({
        gridTemplateColumns: "repeat(auto-fill, 10ch)",
      })
      expect(screen.getAllByRole("listitem").map((item) => item.textContent)).toEqual(
        state.undated.map((printing) => printing.cardNo),
      )
      expect(screen.getByText("BP01-021Ⓢ")).toHaveClass("text-left", "whitespace-nowrap")
      expect(screen.queryByText("p:wide")).not.toBeInTheDocument()
    },
  )
  it.each(["/cards/BP-SYNTHETIC", "/p/42"])(
    "uses the resolved printing's region for %s",
    async (path) => {
      state.crossRegion = true
      state.available = true
      await renderRoutes(
        [
          { path: "/cards/:cardNo", ...card },
          { path: "/p/:intId", ...card },
        ],
        { initialEntries: [path] },
      )
      expect(state.wording).toHaveBeenCalledWith("f:synthetic", "en")
      expect(screen.getByText("Synthetic presentation name")).toBeVisible()
    },
  )
})

describe("double-face headers", () => {
  it("shows each face's own name and preserves the back translation quality", async () => {
    state.doubleFace = true
    prefsStore.set({ uiLanguage: "zh-TW", nameDisplay: "translated" })
    await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByRole("heading", { level: 2, name: "Synthetic front" })).toBeVisible()
    expect(screen.getByRole("heading", { level: 2, name: "合成背面譯名" })).toBeVisible()
    expect(screen.getByRole("button", { name: "放大卡圖：合成背面譯名" })).toBeInTheDocument()
    expect(screen.getByText("待校對")).toBeVisible()
    expect(screen.getByText("依日文原文翻譯")).toBeVisible()
    // The stat line names each value in text; cost has no icon, attack and defense do.
    expect(screen.getAllByText("費用")).toHaveLength(2)
    expect(screen.getAllByText("2")).toHaveLength(2)
    expect(screen.queryByText("攻擊力")).not.toBeInTheDocument()
  })
  it("keeps previous/next inside the pages loaded by the originating list", async () => {
    state.doubleFace = true
    state.order = Array.from({ length: 61 }, (_, index) => ({
      key: `c:${String(index)}`,
      printingId: index === 59 ? "p:synthetic" : `p:${String(index)}`,
    }))
    prefsStore.set({ uiLanguage: "zh-TW" })
    const { router } = await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByRole("button", { name: "下一張" })).toBeEnabled()
    await act(() =>
      router.navigate("/cards/BP-SYNTHETIC", {
        replace: true,
        state: { background: "", source: "results", pages: 1, resultKey: "c:59" },
      }),
    )
    await waitFor(() => expect(screen.getByRole("button", { name: "下一張" })).toBeDisabled())
    expect(screen.getByRole("button", { name: "上一張" })).toBeEnabled()
  })
  it("closes the enlarged image on browser back before leaving the card", async () => {
    state.doubleFace = true
    prefsStore.set({ uiLanguage: "zh-TW" })
    const { router } = await renderRoutes(
      [
        { path: "/cards", element: <p>Synthetic list</p> },
        { path: "/cards/:cardNo", ...card },
      ],
      { initialEntries: ["/cards", "/cards/BP-SYNTHETIC"] },
    )
    await userEvent.click(screen.getByRole("button", { name: "放大卡圖：Synthetic front" }))
    await waitFor(() => {
      expect(router.state.location.state).toMatchObject({ dialog: "card-zoom" })
    })
    await act(() => router.navigate(-1))
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    expect(router.state.location.pathname).toBe("/cards/BP-SYNTHETIC")
    await act(() => router.navigate(-1))
    expect(screen.getByText("Synthetic list")).toBeVisible()
  })
  it("offers previous/next from the list order and enlarges the image in a dialog", async () => {
    state.doubleFace = true
    prefsStore.set({ uiLanguage: "zh-TW" })
    await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByRole("button", { name: "上一張" })).toBeDisabled()
    expect(screen.getByRole("button", { name: "下一張" })).toBeEnabled()
    await userEvent.click(screen.getByRole("button", { name: "放大卡圖：Synthetic front" }))
    const dialog = screen.getByRole("dialog", { name: "Synthetic front" })
    expect(dialog).toBeVisible()
    await userEvent.click(screen.getByRole("button", { name: "關閉" }))
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
  })
})
