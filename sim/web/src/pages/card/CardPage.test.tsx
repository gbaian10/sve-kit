import { act, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useParams } from "react-router"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { useCardSnapshot } from "../../app/snapshot"
import { ToastProvider } from "../../components/ui/Toast"
import { stringValue } from "../../data"
import { CardRouteError } from "../../domain/cardRoute"
import { DEFAULT_PREFS, prefsStore, showsOtherEdition } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { CardView } from "./CardPage"

function CardPage() {
  const { catalog, client, status } = useCardSnapshot()
  const { cardNo, intId } = useParams()
  const row = cardNo
    ? (catalog?.index.printingByCardNo("jp", cardNo) ??
      catalog?.index.printingByCardNo("en", cardNo))
    : catalog?.index
        .printingsOf("c:synthetic")
        .find((row) => typeof row["int_id"] === "number" && String(row["int_id"]) === intId)
  return (
    <CardView
      client={client}
      status={status}
      catalog={catalog}
      resolved={
        row
          ? {
              id: stringValue(row["id"]),
              cardId: "c:synthetic",
              cardOwner: "BP-SYNTHETIC",
              printingOwner: "BP-SYNTHETIC",
              region: row["region"] === "en" ? "en" : "jp",
              cardNo: cardNo ?? "BP-SYNTHETIC",
              cardNoState: "official",
            }
          : undefined
      }
    />
  )
}

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
  comparisonReady: false,
  doubleFace: false,
  order: null as { key: string; printingId: string }[] | null,
  multiplePrintings: false,
  printingRows: null as
    | {
        id: string
        card_id: string
        home_set_id?: string
        region: string
        card_no: string
        rarity_code: string
        premium: boolean
      }[]
    | null,
  translated: false,
  originalName: false,
  sections: false,
  candidates: [] as { printing_id: string; revision_id: string | null }[],
  undated: [{ id: "p:undated", cardNo: "PR-SYNTHETIC" }],
  wording: vi.fn(),
}))
vi.mock("../../app/other-edition", () => ({
  useOtherEdition: () => ({
    catalog:
      state.comparisonReady && showsOtherEdition(prefsStore.get(), state.crossRegion ? "en" : "jp")
        ? {}
        : undefined,
    retry: () => undefined,
  }),
}))
vi.mock("../../app/snapshot", () => {
  const printing = () => ({
    id: "p:synthetic",
    card_no: "BP-SYNTHETIC",
    home_set_id: "BP-SYNTHETIC",
    card_id: "c:synthetic",
    region: state.crossRegion ? "en" : "jp",
    int_id: 42,
  })
  return {
    useImageIndex: () => undefined,
    // Most cases leave the exact name pending to exercise the catalog stand-in.
    useAnnotatedText: (
      _client: unknown,
      owner: { kind: string; id: string },
      field: string,
      _language: string,
      _edition: string,
      _resolver: unknown,
      ordinal: number | null,
    ) => ({
      readiness: {
        basic:
          field === "effect" || field === "section" || state.originalName ? "ready" : "pending",
        source: "none",
        annotations: "none",
      },
      value:
        field === "name" && state.originalName && owner.id === "rev:other"
          ? {
              original: { unit: { text: "Synthetic original name", lang: "ja" }, annotation: null },
              concepts: [],
              explanations: [],
              cards: [],
              keywords: [],
              abilities: [],
            }
          : owner.kind === "face_revision" && (field === "effect" || field === "section")
            ? {
                ...(state.sections && field === "effect"
                  ? {
                      originalSections: [
                        {
                          ordinal: 1,
                          label: `Synthetic ${owner.id === "rev:other" ? "older" : "selected"} heading`,
                        },
                        { ordinal: 3 },
                      ],
                    }
                  : {}),
                original: {
                  unit: {
                    text:
                      field === "section"
                        ? `Synthetic ${owner.id === "rev:other" ? "older" : "selected"} reminder ${String(ordinal)}`
                        : owner.id === "rev:other"
                          ? "Synthetic older effect"
                          : "Synthetic selected effect",
                    lang: "ja",
                  },
                  annotation: null,
                },
                ...(state.translated
                  ? {
                      translated: {
                        unit: {
                          text: field === "section" ? "合成補充譯文" : "合成新版譯文",
                          lang: "zh-Hant",
                        },
                        annotation: null,
                      },
                    }
                  : {}),
                concepts: [],
                explanations: [],
                cards: [],
              }
            : null,
    }),
    useSearchReadiness: () => ({
      phase: "ready",
      generation: 1,
      updating: false,
      retry: () => undefined,
    }),
    useNeighbours: (_query: unknown, _target: string, maxItems?: number) => {
      const order =
        state.order ??
        (state.doubleFace
          ? [
              { key: "c:synthetic", printingId: "p:synthetic" },
              { key: "c:other", printingId: "p:synthetic" },
            ]
          : [])
      return {
        result: {
          phase: "complete",
          page: {
            items: order.slice(0, maxItems).map((item) => ({
              ...item,
              summary: { cardNo: "BP-SYNTHETIC", printingId: item.printingId },
            })),
          },
        },
      }
    },
    useCardSnapshot: () => ({
      status: { state: "ready" },
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
                rarityCode: "sr",
                premium: true,
                // A pending card's catalog name is the card-number stand-in.
                name: {
                  original: {
                    lang: "ja",
                    text: state.originalName ? "BP-SYNTHETIC" : "Synthetic front",
                  },
                  translations: {},
                },
                cost: 1,
                attack: 1,
                defense: 1,
              }
            : undefined,
        index: {
          cards: [{ id: "c:synthetic" }],
          card: () => ({ id: "c:synthetic", home_set_id: "BP-SYNTHETIC" }),
          family: () => ({ id: "BP-SYNTHETIC", code: "bp-synthetic", public_code: "BP-SYN" }),
          printingsOf: () =>
            state.printingRows ??
            (state.multiplePrintings
              ? [
                  { ...printing(), card_no: "BP-SYNTHETIC" },
                  { ...printing(), id: "p:second", card_no: "BP-SECOND" },
                  { ...printing(), id: "p:third", card_no: "BP-THIRD" },
                ]
              : [printing()]),
          printingByCardNo: (region: string) =>
            state.crossRegion && region === "jp" ? undefined : printing(),
          facesOf: () => [{ id: "f:synthetic" }, ...(state.doubleFace ? [{ id: "f:back" }] : [])],
          wording: (face: string, region: string) => {
            state.wording(face, region)
            return {
              classification: "same_effect",
              basis_printing_id: "p:undated",
              display: {
                revision_id: state.available ? "rev:synthetic" : null,
                basis: state.current
                  ? "current"
                  : state.available
                    ? "latest_known_release"
                    : "candidates",
              },
              candidates: state.candidates,
              undated_printing_ids: state.undated.map((printing) => printing.id),
            }
          },
          displayRevision: (face: string) =>
            state.originalName
              ? undefined
              : state.doubleFace
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
                  ? { id: "rev:synthetic", face_id: "f:synthetic", name_unit_id: "t:synthetic" }
                  : undefined,
          revision: (id: string) => ({
            id,
            name_unit_id: "t:synthetic",
            translations: [],
            class_code: "royal",
            type_code: "follower",
            traits: ["synthetic"],
            cost: 3,
            attack: 4,
            defense: 5,
          }),
          translation: () => ({ text_unit_id: "t:back-translated", low_confidence: true }),
          vocabulary: (kind: string, code: string) =>
            kind === "rarity" && code === "lg"
              ? { label_unit_id: "t:rarity", translations: [] }
              : undefined,
          textUnit: (id: string) => ({
            lang: "ja",
            text:
              id === "t:rarity"
                ? "傳說"
                : id === "t:back"
                  ? "Synthetic back"
                  : id === "t:back-translated"
                    ? "合成背面譯名"
                    : "Synthetic presentation name",
          }),
          printing: (id: string) => {
            const undated = state.undated.find((printing) => printing.id === id)
            return {
              ...printing(),
              home_set_id: "BP-SYNTHETIC",
              card_no: undated?.cardNo ?? (id === "p:synthetic" ? "BP-SYNTHETIC" : "BP-OTHER"),
            }
          },
        },
      },
    }),
  }
})

beforeEach(() => {
  state.originalName = false
  state.available = false
  state.multiplePrintings = false
  state.printingRows = null
  state.translated = false
  state.sections = false
  state.candidates = [
    { printing_id: "p:synthetic", revision_id: "rev:other" },
    { printing_id: "p:undated", revision_id: "rev:synthetic" },
  ]
  state.current = false
  state.crossRegion = false
  state.comparisonReady = false
  state.doubleFace = false
  state.order = null
  state.undated = [{ id: "p:undated", cardNo: "PR-SYNTHETIC" }]
  state.wording.mockClear()
})

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("original card titles", () => {
  it.each(["zh-TW", "ja", "en"] as const)(
    "shows the original name when no translation or display revision exists in %s",
    async (language) => {
      state.originalName = true
      state.doubleFace = true
      await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
        initialEntries: ["/cards/BP-SYNTHETIC"],
        language,
      })
      expect(
        screen.getAllByRole("heading", { level: 2, name: "Synthetic original name" })[0],
      ).toHaveAttribute("lang", "ja")
      expect(
        screen.queryByRole("heading", { level: 2, name: "BP-SYNTHETIC" }),
      ).not.toBeInTheDocument()
    },
  )
})

it("shows the shared type line and stats without a display revision", async () => {
  state.originalName = true
  state.doubleFace = true
  await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
    initialEntries: ["/cards/BP-SYNTHETIC"],
    language: "zh-TW",
  })
  expect(screen.getAllByText(/royal.*follower/)[0]).toBeVisible()
  expect(screen.getAllByText("synthetic")[0]).toBeVisible()
  expect(
    screen.getAllByRole("img", { name: "官方稀有度：SR" })[0]?.parentElement?.parentElement,
  ).toHaveTextContent(/royal.*follower/)
  expect(document.querySelectorAll("[data-rarity-badges] img")).toHaveLength(2)
  for (const [label, value] of [
    ["消費", "3"],
    ["攻擊力", "4"],
    ["生命值", "5"],
  ] as const) {
    expect(screen.getAllByText(label)[0]?.parentElement).toHaveTextContent(value)
  }
})

describe("plain wording on the card page", () => {
  it("retains every section of this printing as original text beside the shared translation", async () => {
    state.available = true
    state.translated = true
    state.sections = true
    state.undated = []
    prefsStore.set({ effectLanguage: "translated" })
    await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByText("Synthetic older reminder 1")).toBeVisible()
    expect(screen.getByText("Synthetic older reminder 3")).toBeVisible()
    expect(screen.getByRole("heading", { name: "Synthetic older heading" })).toBeVisible()
    expect(screen.queryByText("Synthetic selected heading")).not.toBeInTheDocument()
    expect(screen.queryByText("Synthetic selected reminder 1")).not.toBeInTheDocument()
    expect(screen.queryByText("合成補充譯文")).not.toBeInTheDocument()
  })
  it.each(["zh-TW", "ja", "en"] as const)(
    "shows this printing's own original in %s",
    async (language) => {
      state.available = true
      state.current = true
      state.candidates.push({ printing_id: "p:additional", revision_id: "rev:other" })
      const { container } = await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
        initialEntries: ["/cards/BP-SYNTHETIC"],
        language,
      })
      expect(screen.getByText("Synthetic older effect")).toBeVisible()
      expect(screen.queryByText("Synthetic selected effect")).not.toBeInTheDocument()
      expect(container.querySelectorAll("details")).toHaveLength(0)
    },
  )
  it("shows a printing's own original beside the latest translation and preserves the list on its source link", async () => {
    state.available = true
    state.translated = true
    state.undated = []
    prefsStore.set({ effectLanguage: "translated" })
    const { router } = await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    await act(() =>
      router.navigate("/cards/BP-SYNTHETIC", {
        replace: true,
        state: { background: "?q=synthetic", source: "results", pages: 2, resultKey: "c:anchor" },
      }),
    )
    expect(screen.getByText("Synthetic older effect")).toBeVisible()
    expect(screen.getByText("合成新版譯文")).toBeVisible()
    expect(screen.queryByText("Synthetic selected effect")).not.toBeInTheDocument()
    expect(screen.getByText("此版次的原文")).toBeVisible()
    expect(screen.getByText("繁中翻譯依據")).toBeVisible()
    await userEvent.click(screen.getByRole("link", { name: "BP-OTHER" }))
    expect(router.state.historyAction).toBe("REPLACE")
    expect(router.state.location.state).toMatchObject({ background: "?q=synthetic", pages: 2 })
  })
  it("keeps all candidates when the printing has multiple observations", async () => {
    state.available = true
    state.translated = true
    state.candidates.push({ printing_id: "p:synthetic", revision_id: "rev:synthetic" })
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByRole("heading", { name: "此卡各版次的卡文不同" })).toBeVisible()
    expect(screen.queryByText("此版次的原文")).not.toBeInTheDocument()
    expect(screen.queryByText("合成新版譯文")).not.toBeInTheDocument()
  })
  it("opens this printing's own wording when no wording is displayed yet", async () => {
    const { container } = await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(container.querySelectorAll("details")).toHaveLength(0)
    expect(screen.getByText("Synthetic older effect")).toBeVisible()
    expect(screen.queryByText("Synthetic selected effect")).not.toBeInTheDocument()
    expect(screen.queryByText("目前顯示的卡文")).not.toBeInTheDocument()
  })
  it("does not claim an unknown candidate has no effect", async () => {
    state.candidates = [{ printing_id: "p:synthetic", revision_id: null }]
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByText("這些版次的卡文還在確認中")).toBeVisible()
    expect(screen.queryByText("這張卡沒有效果文字")).not.toBeInTheDocument()
  })
  it.each(["/cards/BP-SYNTHETIC", "/p/42"])(
    "uses the resolved printing's region for %s",
    async (path) => {
      state.crossRegion = true
      state.available = true
      await renderRoutes(
        [
          { path: "/cards/:cardNo/:slug?", ...card },
          { path: "/p/:intId/:slug?", ...card },
        ],
        { initialEntries: [path] },
      )
      expect(state.wording).toHaveBeenCalledWith("f:synthetic", "en")
      expect(screen.getByText("Synthetic older effect")).toBeVisible()
    },
  )
})

describe("double-face headers", () => {
  it("shows each face's own name and preserves the back translation quality", async () => {
    state.doubleFace = true
    prefsStore.set({ uiLanguage: "zh-TW", nameDisplay: "translated" })
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByRole("heading", { level: 2, name: "Synthetic front" })).toBeVisible()
    expect(screen.getByRole("heading", { level: 2, name: "合成背面譯名" })).toBeVisible()
    expect(screen.getByRole("button", { name: "放大卡圖：合成背面譯名" })).toBeInTheDocument()
    expect(screen.getByText("待校對")).toBeVisible()
    expect(screen.getByText("依日文原文翻譯")).toBeVisible()
    // The stat line names each value in text; cost has no icon, attack and defense do.
    expect(screen.getAllByText("消費")).toHaveLength(2)
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
    const { router } = await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.queryByRole("button", { name: "下一張" })).not.toBeInTheDocument()
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
        { path: "/cards/:cardNo/:slug?", ...card },
      ],
      { initialEntries: ["/cards", "/cards/BP-SYNTHETIC"] },
    )
    await userEvent.click(screen.getByRole("button", { name: "放大卡圖：Synthetic front" }))
    await waitFor(() => {
      expect(router.state.location.state).toMatchObject({ dialog: "card-zoom" })
    })
    await act(() => router.navigate(-1))
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    expect(router.state.location.pathname).toBe("/cards/BP-SYNTHETIC/Synthetic%20front")
    await act(() => router.navigate(-1))
    expect(screen.getByText("Synthetic list")).toBeVisible()
  })
  it("offers previous/next from the list order and enlarges the image in a dialog", async () => {
    state.doubleFace = true
    prefsStore.set({ uiLanguage: "zh-TW" })
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: [
        {
          pathname: "/cards/BP-SYNTHETIC",
          state: { background: "", source: "results", pages: 1, resultKey: "c:synthetic" },
        },
      ],
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

describe("returning from printing changes", () => {
  it.each(["button", "browser"] as const)(
    "returns to the same list entry using %s back",
    async (method) => {
      state.multiplePrintings = true
      const { router } = await renderRoutes(
        [
          { path: "/cards", element: <p>Synthetic list</p> },
          { path: "/cards/:cardNo/:slug?", ...card },
        ],
        { initialEntries: ["/cards?q=synthetic"] },
      )
      await act(() =>
        router.navigate("/cards?q=synthetic", {
          replace: true,
          state: { pages: 2, anchor: "c:anchor" },
        }),
      )
      const listKey = router.state.location.key
      await act(() =>
        router.navigate("/cards/BP-SYNTHETIC", {
          state: { background: "?q=synthetic", source: "results", pages: 2, resultKey: "c:anchor" },
        }),
      )
      await userEvent.click(screen.getByRole("link", { name: "BP-SECOND" }))
      await userEvent.click(screen.getByRole("link", { name: "BP-THIRD" }))
      expect(router.state.location.pathname).toBe("/cards/BP-THIRD")
      if (method === "button") await userEvent.click(screen.getByRole("button", { name: "返回" }))
      else await act(() => router.navigate(-1))
      expect(router.state.location).toMatchObject({
        pathname: "/cards",
        search: "?q=synthetic",
        key: listKey,
        state: { pages: 2, anchor: "c:anchor" },
      })
    },
  )
  it("returns to /cards when opened directly", async () => {
    const { router } = await renderRoutes(
      [
        { path: "/cards", element: <p>Synthetic list</p> },
        { path: "/cards/:cardNo/:slug?", ...card },
      ],
      { initialEntries: ["/cards/BP-SYNTHETIC"] },
    )
    await userEvent.click(screen.getByRole("link", { name: "返回" }))
    expect(router.state.location.pathname).toBe("/cards")
  })
})

describe("regional printing lists", () => {
  it("hides the other region and uses vocabulary labels for printing rarities", async () => {
    state.comparisonReady = true
    state.printingRows = [
      {
        id: "p:promo",
        card_id: "c:synthetic",
        region: "jp",
        card_no: "PR-299",
        rarity_code: "pr",
        premium: false,
      },
      {
        id: "p:en",
        card_id: "c:synthetic",
        region: "en",
        card_no: "BP01-053EN",
        rarity_code: "lg",
        premium: false,
      },
      {
        id: "p:sl",
        card_id: "c:synthetic",
        region: "jp",
        card_no: "BP01-SL09",
        rarity_code: "sl",
        premium: false,
      },
      {
        id: "p:reprint",
        card_id: "c:synthetic",
        region: "jp",
        card_no: "EBD02-005",
        rarity_code: "lg",
        premium: false,
      },
      {
        id: "p:synthetic",
        card_id: "c:synthetic",
        region: "jp",
        card_no: "BP01-053",
        rarity_code: "lg",
        premium: false,
      },
    ].map((row) => ({ ...row, home_set_id: "BP-SYNTHETIC" }))
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP01-053"],
    })
    const jp = screen.getByRole("list", { name: "JP" })
    expect(screen.queryByRole("list", { name: "EN" })).not.toBeInTheDocument()
    expect(
      within(jp)
        .getAllByRole("link")
        .map((link) => link.textContent),
    ).toEqual(["BP01-053 (傳說)", "EBD02-005 (傳說)", "BP01-SL09 (sl)", "PR-299 (pr)"])
    expect(within(jp).getByRole("link", { current: "page" })).toHaveTextContent("BP01-053 (傳說)")
    act(() => {
      prefsStore.set({ showOtherEdition: { ...prefsStore.get().showOtherEdition, jp: true } })
    })
    const en = screen.getByRole("list", { name: "EN" })
    expect(within(en).getByRole("link")).toHaveAttribute("href", "/cards/BP01-053EN")
    expect(within(en).getByRole("link")).toHaveTextContent("BP01-053EN (傳說)")
    act(() => {
      prefsStore.set({ showOtherEdition: { ...prefsStore.get().showOtherEdition, jp: false } })
    })
    expect(screen.queryByRole("list", { name: "EN" })).not.toBeInTheDocument()
    expect(screen.getByRole("list", { name: "JP" })).toBeInTheDocument()
  })
})

describe("route failures", () => {
  function Rejected({ kind }: { readonly kind: CardRouteError["kind"] }) {
    const { client, status } = useCardSnapshot()
    return (
      <CardView
        client={client}
        status={status}
        catalog={null}
        resolved={undefined}
        routeError={new CardRouteError(kind, "synthetic route failure")}
      />
    )
  }
  it.each<[CardRouteError["kind"], string]>([
    ["invalid", "找不到這個卡號"],
    ["not-found", "找不到這個卡號"],
    ["needs-update", "卡表需要更新"],
    ["reference", "無法定位卡片"],
  ])("a %s route shows %s", async (kind, heading) => {
    prefsStore.set({ uiLanguage: "zh-TW" })
    const element = (
      <ToastProvider>
        <Rejected kind={kind} />
      </ToastProvider>
    )
    await renderRoutes([{ path: "/cards/:cardNo", element }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByRole("heading", { level: 1, name: heading })).toBeVisible()
  })
})
