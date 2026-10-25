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

const paragraphs = (root: Element, lang: string) =>
  [...root.querySelectorAll(`p[lang="${lang}"]`)].map((p) => p.textContent)

const state = vi.hoisted(() => ({
  evolve: false,
  traits: ["synthetic"],
  available: false,
  current: false,
  crossRegion: false,
  comparisonReady: false,
  doubleFace: false,
  order: null as { key: string; printingId: string }[] | null,
  suggestions: [] as { cardId: string; printingId: string; cardNo: string }[],
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
  // A recognisable image per printing face, so identity checks can read it back from the DOM.
  images: false,
  summary: false,
  candidates: [] as { printing_id: string; revision_id: string | null }[],
  classification: "same_effect",
  /** Revision ids whose effect has no translation in any language. */
  untranslated: [] as string[],
  /** Revision ids whose printed effect is blank on the official page. */
  blank: [] as string[],
  /** The face's `printed_text_state` in the print-detail band; the face itself has no text. */
  printedTextState: null as string | null,
  /** What the artist read gives back: credited, nobody credited, or a failed read. */
  credits: null as "found" | "none" | "failed" | null,
  basisPrintingId: "p:undated",
  undated: [{ id: "p:undated", cardNo: "PR-SYNTHETIC" }],
  wording: vi.fn(),
}))
vi.mock("../../data", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../data")>()),
  artCredits: () =>
    state.credits === "failed"
      ? Promise.reject(new Error("synthetic read failure"))
      : Promise.resolve(
          new Map(
            state.credits === "found"
              ? [["a:synthetic", [{ id: "y:synthetic", name: "合成繪師" }]]]
              : [],
          ),
        ),
}))
vi.mock("../../app/printing-detail", () => ({
  loadPrintedTextState: () => Promise.resolve(state.printedTextState ?? undefined),
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
    faces: [{ face_id: "f:synthetic", art_id: "a:synthetic" }],
  })
  // Most cases leave the exact name pending to exercise the catalog stand-in.
  const annotatedText = (
    _client: unknown,
    owner: { kind: string; id: string },
    field: string,
    _language: string,
    _edition: string,
    _resolver: unknown,
    ordinal: number | null,
  ) => ({
    readiness: {
      basic: field === "effect" || field === "section" || state.originalName ? "ready" : "pending",
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
                  text: state.blank.includes(owner.id)
                    ? ""
                    : field === "section"
                      ? `Synthetic ${owner.id === "rev:other" ? "older" : "selected"} reminder ${String(ordinal)}`
                      : owner.id === "rev:other"
                        ? "Synthetic older effect"
                        : "Synthetic selected effect",
                  lang: "ja",
                },
                annotation: null,
              },
              ...(state.translated &&
              _language === "zh-Hant" &&
              !state.untranslated.includes(owner.id) &&
              !state.blank.includes(owner.id)
                ? {
                    translated: {
                      unit: {
                        text:
                          field === "section"
                            ? `合成${owner.id === "rev:other" ? "舊版" : "新版"}補充譯文 ${String(ordinal)}`
                            : owner.id === "rev:other"
                              ? "合成舊版譯文"
                              : "合成新版譯文",
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
  })
  return {
    useImageIndex: () =>
      state.images
        ? {
            cardImage: (printingId: string, faceId: string) => ({
              src: `/synthetic/${printingId}/${faceId}.webp`,
              width: 63,
              height: 88,
            }),
            asset: () => undefined,
            known: () => true,
          }
        : undefined,
    useAnnotatedText: annotatedText,
    useAnnotatedTexts: (
      client: unknown,
      pointers: readonly {
        owner: { kind: string; id: string }
        field: string
        ordinal: number | null
      }[],
      language: string,
      edition: string,
      resolver: unknown,
    ) =>
      pointers.map((pointer) =>
        annotatedText(
          client,
          pointer.owner,
          pointer.field,
          language,
          edition,
          resolver,
          pointer.ordinal,
        ),
      ),
    useSearchReadiness: () => ({
      phase: "ready",
      generation: 1,
      updating: false,
      retry: () => undefined,
    }),
    useWorkerSuggestions: (text: string, edition: string) => ({
      lastInput: { state: { text }, edition },
      result: {
        phase: "complete",
        suggestions: text === "" ? [] : state.suggestions.map((summary) => ({ summary })),
      },
      images: undefined,
    }),
    useNeighbours: (_query: unknown, target: string | null, maxItems?: number) => {
      const all =
        state.order ??
        (state.doubleFace
          ? [
              { key: "c:synthetic", printingId: "p:synthetic" },
              { key: "c:other", printingId: "p:synthetic" },
            ]
          : [])
      // Like the index: the loaded order when the target names one of its cells, else nothing.
      const loaded = all.slice(0, maxItems)
      const order = loaded.some((item) => item.key === target || item.printingId === target)
        ? loaded
        : []
      return {
        result: {
          phase: "complete",
          page: {
            items: order.map((item) => ({
              ...item,
              summary: { cardNo: "BP-SYNTHETIC", printingId: item.printingId },
            })),
          },
        },
      }
    },
    useCardSnapshot: () => ({
      status: { state: "ready" },
      client: {
        snapshot: () =>
          state.credits
            ? { manifestHash: "synthetic", manifest: { data_as_of: "2026-10-24" } }
            : null,
      },
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
          state.doubleFace || state.summary
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
              classification: state.classification,
              basis_printing_id: state.basisPrintingId,
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
            traits: state.traits,
            special_kinds: state.evolve ? ["evolve"] : [],
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
              id,
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
  state.evolve = false
  state.credits = null
  state.traits = ["synthetic"]
  state.originalName = false
  state.available = false
  state.multiplePrintings = false
  state.printingRows = null
  state.translated = false
  state.sections = false
  state.images = false
  state.summary = false
  state.candidates = [
    { printing_id: "p:synthetic", revision_id: "rev:other" },
    { printing_id: "p:undated", revision_id: "rev:synthetic" },
  ]
  state.current = false
  state.crossRegion = false
  state.comparisonReady = false
  state.doubleFace = false
  state.order = null
  state.suggestions = []
  state.undated = [{ id: "p:undated", cardNo: "PR-SYNTHETIC" }]
  state.classification = "same_effect"
  state.untranslated = []
  state.blank = []
  state.printedTextState = null
  state.basisPrintingId = "p:undated"
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
  const meta = screen
    .getAllByRole("img", { name: "官方稀有度：SR" })[0]
    ?.closest("p") as HTMLElement
  expect(meta).toHaveTextContent("royal・synthetic")
  expect(meta).not.toHaveTextContent("follower")
  expect(within(meta).getByRole("img", { name: "follower" })).toBeVisible()
  expect(document.querySelectorAll("[data-rarity-badges] img")).toHaveLength(2)
  for (const name of ["消費 3", "攻擊力 4", "生命值 5"]) {
    expect(screen.getAllByRole("listitem", { name })[0]).toBeVisible()
  }
  // The crystal shows numbers up to 10 itself; attack and defense print theirs beside the icon.
  const cost = screen.getAllByRole("listitem", { name: "消費 3" })[0] as HTMLElement
  expect(within(cost).getByRole("img", { name: "消費" })).toBeVisible()
  expect(within(cost).queryByText("3")).not.toBeInTheDocument()
  const attack = screen.getAllByRole("listitem", { name: "攻擊力 4" })[0] as HTMLElement
  expect(within(attack).getByRole("img", { name: "攻擊力" })).toBeVisible()
  expect(within(attack).getByText("4")).toBeVisible()
  const defense = screen.getAllByRole("listitem", { name: "生命值 5" })[0] as HTMLElement
  expect(within(defense).getByText("5")).toBeVisible()
})

describe("the artist line", () => {
  const open = () => {
    state.summary = true
    state.originalName = true
    return renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
      language: "zh-TW",
    })
  }

  it("links each credited artist to the list filtered by that artist", async () => {
    state.credits = "found"
    await open()
    const link = await screen.findByRole("link", { name: "查看繪師 合成繪師 的卡片" })
    expect(link).toHaveAttribute("href", "/cards?artist=y%3Asynthetic")
  })

  it("says so, as plain text, when the official page credits nobody", async () => {
    state.credits = "none"
    await open()
    const line = await screen.findByText("官方未標示", { exact: false })
    expect(line.closest("[data-card-artists]")).toHaveTextContent("繪師：官方未標示")
    expect(line.closest("[data-card-artists]")?.querySelector("a")).toBeNull()
  })

  it("leaves the line out when the read failed instead of claiming nobody is credited", async () => {
    state.credits = "failed"
    await open()
    await screen.findByRole("heading", { level: 2 })
    await waitFor(() => {
      expect(document.querySelector("[data-card-artists]")).toBeNull()
    })
    expect(screen.queryByText("官方未標示")).not.toBeInTheDocument()
  })
})

describe("plain wording on the card page", () => {
  it("prints each revision's own sections under its effect, translation beside this printing's original", async () => {
    state.available = true
    state.translated = true
    state.sections = true
    state.undated = []
    await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    const region = screen.getByRole("region", { name: "效果" })
    // One box: each translation with this printing's own text in a box under it, by section ordinal.
    const translation = region
    const boxes = [...region.querySelectorAll<HTMLElement>("[data-original]")]
    expect(boxes).toHaveLength(3)
    expect(paragraphs(translation, "zh-Hant")).toEqual([
      "合成舊版譯文",
      "合成舊版補充譯文 1",
      "合成舊版補充譯文 3",
    ])
    expect(within(translation).getByText("Synthetic older heading")).toBeVisible()
    expect(translation.querySelectorAll("[data-section]")).toHaveLength(2)
    expect(boxes.map((box) => paragraphs(box, "ja"))).toEqual([
      ["Synthetic older effect"],
      ["Synthetic older reminder 1"],
      ["Synthetic older reminder 3"],
    ])
    for (const [index, section] of [...translation.querySelectorAll("[data-section]")].entries())
      expect(section).toContainElement(boxes[index + 1] ?? null)
    expect(screen.queryByText("合成新版譯文")).not.toBeInTheDocument()
    expect(screen.queryByText("Synthetic selected effect")).not.toBeInTheDocument()
    expect(screen.queryByText("附註")).not.toBeInTheDocument()
    expect(screen.queryByText("此版次的原文")).not.toBeInTheDocument()
    expect(document.querySelector("[data-borrowed-translation]")).toBeNull()
    // The note under the effect names the latest wording's basis printing, in every mode.
    const note = () => document.querySelector("[data-wording-note]")
    expect(note()).toHaveTextContent("與 BP-OTHER 文字描述略有差異，但意思相同。")
    expect(
      region.compareDocumentPosition(note() as Element) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy()
    act(() => {
      prefsStore.set({ effectLanguage: "translated" })
    })
    expect(paragraphs(screen.getByRole("region", { name: "效果" }), "zh-Hant")).toHaveLength(3)
    expect(paragraphs(screen.getByRole("region", { name: "效果" }), "ja")).toHaveLength(0)
    expect(note()).toHaveTextContent("BP-OTHER")
    act(() => {
      prefsStore.set({ effectLanguage: "original" })
    })
    expect(paragraphs(screen.getByRole("region", { name: "效果" }), "ja")).toHaveLength(3)
    expect(paragraphs(screen.getByRole("region", { name: "效果" }), "zh-Hant")).toHaveLength(0)
    expect(note()).toHaveTextContent("BP-OTHER")
  })
  it.each(["errata", "pending_review"] as const)(
    "says the newer wording applies when the wording is classified %s",
    async (classification) => {
      state.available = true
      state.current = true
      state.translated = true
      state.classification = classification
      await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
        initialEntries: ["/cards/BP-SYNTHETIC"],
      })
      expect(screen.getByText("合成舊版譯文")).toBeVisible()
      expect(screen.getByText("Synthetic older effect")).toBeVisible()
      expect(document.querySelector("[data-wording-note]")).toHaveTextContent(
        "與 PR-SYNTHETIC 文字描述略有差異，以較新的寫法為準。",
      )
      expect(screen.queryByText(/意思相同/)).not.toBeInTheDocument()
    },
  )
  it("names the first printing of the older wording on a latest-wording printing and links it like a chip", async () => {
    state.available = true
    state.current = true
    state.translated = true
    // This printing carries the latest wording; two older printings share the other one.
    state.candidates = [
      { printing_id: "p:synthetic", revision_id: "rev:synthetic" },
      { printing_id: "p:third", revision_id: "rev:other" },
      { printing_id: "p:second", revision_id: "rev:other" },
    ]
    state.undated = [
      { id: "p:second", cardNo: "BP-SECOND" },
      { id: "p:third", cardNo: "BP-THIRD" },
    ]
    state.multiplePrintings = true
    const { router } = await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByText("合成新版譯文")).toBeVisible()
    expect(screen.getByText("Synthetic selected effect")).toBeVisible()
    expect(screen.queryByText("Synthetic older effect")).not.toBeInTheDocument()
    const note = document.querySelector("[data-wording-note]")
    expect(note).toHaveTextContent("與 BP-SECOND 文字描述略有差異，但意思相同。")
    expect(note).not.toHaveTextContent("BP-THIRD")
    await userEvent.click(within(note as HTMLElement).getByRole("link", { name: "BP-SECOND" }))
    expect(router.state.historyAction).toBe("REPLACE")
    expect(router.state.location.pathname).toContain("/cards/BP-SECOND")
  })
  it("borrows the latest wording's translation when this printing's revision has none", async () => {
    state.available = true
    state.current = true
    state.translated = true
    state.untranslated = ["rev:other"]
    await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    const region = screen.getByRole("region", { name: "效果" })
    const translation = region
    const original = region.querySelector<HTMLElement>("[data-original]")
    if (!original) throw new Error("original box expected")
    expect(paragraphs(translation, "zh-Hant")).toEqual(["合成新版譯文"])
    expect(within(translation).getByText("機器譯文 · 非官方")).toBeVisible()
    expect(
      within(translation).getByText("此版次的卡文尚無譯文，顯示的是最新寫法的譯文。"),
    ).toBeVisible()
    expect(paragraphs(original, "ja")).toEqual(["Synthetic older effect"])
    expect(screen.queryByText("Synthetic selected effect")).not.toBeInTheDocument()
    expect(document.querySelector("[data-wording-note]")).toHaveTextContent("PR-SYNTHETIC")
  })
  it("names the latest wording's basis printing even when another of its printings comes first in chip order", async () => {
    state.available = true
    state.current = true
    state.translated = true
    // The latest wording has two printings; the basis is the later one in chip order.
    state.candidates = [
      { printing_id: "p:synthetic", revision_id: "rev:other" },
      { printing_id: "p:second", revision_id: "rev:synthetic" },
      { printing_id: "p:third", revision_id: "rev:synthetic" },
    ]
    state.undated = [
      { id: "p:second", cardNo: "BP-SECOND" },
      { id: "p:third", cardNo: "BP-THIRD" },
    ]
    state.basisPrintingId = "p:third"
    state.multiplePrintings = true
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    const note = document.querySelector("[data-wording-note]")
    expect(note).toHaveTextContent("與 BP-THIRD 文字描述略有差異，但意思相同。")
    expect(note).not.toHaveTextContent("BP-SECOND")
  })
  it("names one printing per other wording when the face has three", async () => {
    state.available = true
    state.current = true
    state.translated = true
    state.candidates = [
      { printing_id: "p:synthetic", revision_id: "rev:other" },
      { printing_id: "p:second", revision_id: "rev:synthetic" },
      { printing_id: "p:third", revision_id: "rev:third" },
      { printing_id: "p:fourth", revision_id: "rev:third" },
    ]
    state.undated = [
      { id: "p:second", cardNo: "BP-SECOND" },
      { id: "p:third", cardNo: "BP-THIRD" },
      { id: "p:fourth", cardNo: "BP-FOURTH" },
    ]
    state.basisPrintingId = "p:second"
    state.multiplePrintings = true
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    const note = document.querySelector("[data-wording-note]")
    expect(note).toHaveTextContent("與 BP-SECOND、BP-FOURTH 文字描述略有差異，但意思相同。")
    expect(note).not.toHaveTextContent("BP-THIRD")
  })
  it("shows the latest wording alone when this printing's official text is blank", async () => {
    state.available = true
    state.current = true
    state.translated = true
    state.blank = ["rev:other"]
    state.printedTextState = "unknown"
    await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByText("合成新版譯文")).toBeVisible()
    expect(screen.getByText("Synthetic selected effect")).toBeVisible()
    expect(screen.getAllByText("原文")[0]).toBeVisible()
    // The state comes from the print-detail band, not from a printed text unit (there is none).
    expect(await screen.findByText("印刷文字尚未確認")).toBeVisible()
    expect(screen.queryByText("這張卡沒有效果文字")).not.toBeInTheDocument()
    expect(document.querySelector("[data-wording-note]")).toBeNull()
  })
  it.each(["ja", "en"] as const)(
    "keeps sections in the original language without Traditional Chinese in %s",
    async (language) => {
      state.available = true
      state.translated = true
      state.sections = true
      state.undated = []
      await renderRoutes([{ path: "/cards/:cardNo", ...card }], {
        initialEntries: ["/cards/BP-SYNTHETIC"],
        language,
      })
      const effect = document.querySelector('[data-field="effect"]')
      if (!effect) throw new Error("effect region missing")
      expect(document.querySelectorAll('[data-field="effect"]')).toHaveLength(1)
      // No translation is expected in this language: only this printing's own text shows.
      expect(paragraphs(effect, "ja")).toEqual([
        "Synthetic older effect",
        "Synthetic older reminder 1",
        "Synthetic older reminder 3",
      ])
      expect(document.querySelectorAll('p[lang="zh-Hant"]')).toHaveLength(0)
      expect(screen.queryByText("Synthetic selected effect")).not.toBeInTheDocument()
      expect(document.querySelector("[data-wording-note]")).toHaveTextContent("BP-OTHER")
    },
  )
  it("switches between candidates' own effect and sections without changing the printing", async () => {
    state.available = true
    state.translated = true
    state.sections = true
    state.images = true
    state.summary = true
    state.candidates.push({ printing_id: "p:synthetic", revision_id: "rev:synthetic" })
    const { router, i18n } = await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    const identity = () => {
      const slot = document.querySelector("[data-image-printing]")
      const image = slot?.querySelector("img")
      return {
        printing: slot?.getAttribute("data-image-printing"),
        face: slot?.getAttribute("data-image-face"),
        src: image?.getAttribute("src"),
        zoom: screen.getByRole("button", { name: /Synthetic front/ }).getAttribute("aria-label"),
      }
    }
    const before = identity()
    const pathname = router.state.location.pathname
    expect(pathname).toMatch(/^\/cards\/BP-SYNTHETIC/)
    expect(before).toEqual({
      printing: "p:synthetic",
      face: "f:synthetic",
      src: "/synthetic/p:synthetic/f:synthetic.webp",
      zoom: "放大卡圖：Synthetic front",
    })
    const [older, selected] = [...document.querySelectorAll("details")]
    if (!older || !selected) throw new Error("two candidates expected")
    expect(selected).toHaveAttribute("open")
    expect(older).not.toHaveAttribute("open")
    expect(paragraphs(selected, "ja")).toEqual([
      "Synthetic selected effect",
      "Synthetic selected reminder 1",
      "Synthetic selected reminder 3",
    ])
    expect(within(selected).getByText("Synthetic selected heading")).toBeVisible()
    expect(within(selected).getByText("目前顯示的卡文")).toBeVisible()
    expect(paragraphs(older, "ja")).toEqual([])
    await userEvent.click(within(older).getByText("BP-SYNTHETIC"))
    expect(older).toHaveAttribute("open")
    expect(paragraphs(older, "ja")).toEqual([
      "Synthetic older effect",
      "Synthetic older reminder 1",
      "Synthetic older reminder 3",
    ])
    expect(within(older).getByText("Synthetic older heading")).toBeVisible()
    expect(document.querySelectorAll('p[lang="zh-Hant"]')).toHaveLength(0)
    expect(router.state.location.pathname).toBe(pathname)
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("BP-SYNTHETIC")
    expect(identity()).toEqual(before)
    // Text preference and UI language only change what is shown for the same printing.
    act(() => {
      prefsStore.set({ effectLanguage: "translated" })
    })
    expect(identity()).toEqual(before)
    expect(paragraphs(older, "ja")).toHaveLength(3)
    await act(() => i18n.changeLanguage("en"))
    expect(identity()).toEqual({ ...before, zoom: "Enlarge card image: Synthetic front" })
    expect(paragraphs(older, "ja")).toEqual([
      "Synthetic older effect",
      "Synthetic older reminder 1",
      "Synthetic older reminder 3",
    ])
    expect(document.querySelectorAll('p[lang="zh-Hant"]')).toHaveLength(0)
    expect(state.wording).toHaveBeenCalledWith("f:synthetic", "jp")
    expect(state.wording).not.toHaveBeenCalledWith("f:synthetic", "en")
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
      expect(document.querySelector("[data-wording-note]")).not.toBeNull()
      expect(container.querySelectorAll("details")).toHaveLength(0)
    },
  )
  it("stacks a printing's own original under its own translation without a source area", async () => {
    state.available = true
    state.translated = true
    state.undated = []
    prefsStore.set({ effectLanguage: "both" })
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByText("Synthetic older effect")).toBeVisible()
    expect(screen.getByText("合成舊版譯文")).toBeVisible()
    expect(screen.queryByText("合成新版譯文")).not.toBeInTheDocument()
    expect(screen.queryByText("Synthetic selected effect")).not.toBeInTheDocument()
    expect(screen.getAllByText("原文")[0]).toBeVisible()
    // The translation stacks above this printing's original, never beside it.
    expect(
      screen
        .getByText("合成舊版譯文")
        .compareDocumentPosition(screen.getByText("Synthetic older effect")) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy()
    act(() => {
      prefsStore.set({ effectLanguage: "original" })
    })
    expect(screen.queryByText("合成舊版譯文")).not.toBeInTheDocument()
    expect(screen.getByText("Synthetic older effect")).toBeVisible()
    act(() => {
      prefsStore.set({ effectLanguage: "translated" })
    })
    expect(screen.queryByText("來源與依據")).not.toBeInTheDocument()
    expect(screen.queryByText("繁中翻譯依據")).not.toBeInTheDocument()
    expect(screen.queryByRole("link", { name: "BP-OTHER" })).not.toBeInTheDocument()
  })
  it("marks a display without an adopted current as provisional and leaves an adopted one unmarked", async () => {
    state.available = true
    state.current = true
    const adopted = await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByText("Synthetic older effect")).toBeVisible()
    expect(screen.queryByRole("note")).not.toBeInTheDocument()
    adopted.unmount()
    // The selected printing's own revision differs from the display: its own text still shows.
    state.current = false
    const own = await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByRole("note")).toHaveTextContent("卡文未定：尚無採納的現行卡文")
    expect(screen.getByRole("note")).not.toHaveTextContent("發售")
    // No translation anywhere: the one box holds this printing's own text, the note still names
    // the other wording.
    expect(screen.getByText("Synthetic older effect")).toBeVisible()
    expect(screen.queryByText("Synthetic selected effect")).not.toBeInTheDocument()
    expect(document.querySelector("[data-wording-note]")).not.toBeNull()
    own.unmount()
    // Several observed revisions on this printing: the candidate comparison keeps the note.
    state.candidates.push({ printing_id: "p:synthetic", revision_id: "rev:synthetic" })
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByRole("heading", { name: "此卡各版次的卡文不同" })).toBeVisible()
    expect(screen.getByRole("note")).toHaveTextContent("卡文未定：尚無採納的現行卡文")
  })
  it("keeps all candidates when the printing has multiple observations", async () => {
    state.available = true
    state.translated = true
    state.candidates.push({ printing_id: "p:synthetic", revision_id: "rev:synthetic" })
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByRole("heading", { name: "此卡各版次的卡文不同" })).toBeVisible()
    expect(document.querySelector("[data-wording-note]")).toBeNull()
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
    // Each face has its own three cells; a face without attack or defense says so with a dash.
    expect(screen.getAllByRole("listitem", { name: "消費 2" })).toHaveLength(2)
    expect(screen.getAllByRole("listitem", { name: "攻擊力 —" })).toHaveLength(2)
    expect(screen.getAllByRole("listitem", { name: "生命值 —" })).toHaveLength(2)
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
  it.each([
    {
      name: "walks the suggestion order, not the result order, from a suggestion",
      entry: { background: "?q=syn", source: "suggest", resultKey: "p:synthetic" },
      // The results put this card first; the suggestions put it last.
      previous: true,
      next: false,
    },
    {
      name: "finds the card's own cell when the list key names another printing",
      entry: { background: "?unit=card", source: "results", resultKey: "p:synthetic" },
      previous: true,
      next: true,
    },
  ])("$name", async ({ entry, previous, next }) => {
    state.summary = true
    state.order = [
      { key: "c:synthetic", printingId: "p:representative" },
      { key: "c:second", printingId: "p:second" },
    ]
    state.suggestions = [
      { cardId: "c:first", printingId: "p:first", cardNo: "BP-FIRST" },
      { cardId: "c:synthetic", printingId: "p:synthetic", cardNo: "BP-SYNTHETIC" },
    ]
    if (entry.source === "results")
      state.order = [{ key: "c:first", printingId: "p:first" }, ...state.order]
    prefsStore.set({ uiLanguage: "zh-TW" })
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: [{ pathname: "/cards/BP-SYNTHETIC", state: { ...entry, pages: 1 } }],
    })
    const button = (name: string) => screen.findByRole("button", { name })
    expect(await button("上一張")).toHaveProperty("disabled", !previous)
    expect(await button("下一張")).toHaveProperty("disabled", !next)
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

describe("page layout", () => {
  it("shows the card beside its text with no side columns", async () => {
    state.available = true
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByRole("heading", { level: 1, name: "BP-SYNTHETIC" })).toBeVisible()
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument()
  })

  it.each(["both", "translated", "original"] as const)(
    "a printing whose text is the shared revision shows one effect section (%s)",
    async (effectLanguage) => {
      state.available = true
      state.translated = true
      state.candidates = [{ printing_id: "p:synthetic", revision_id: "rev:synthetic" }]
      state.undated = []
      prefsStore.set({ uiLanguage: "zh-TW", effectLanguage })
      await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
        initialEntries: ["/cards/BP-SYNTHETIC"],
      })
      expect(document.querySelectorAll('[data-field="effect"]')).toHaveLength(1)
      expect(document.querySelector("[data-wording-note]")).toBeNull()
    },
  )

  it("stacks the translation above the original when both are shown", async () => {
    state.available = true
    state.translated = true
    // This printing's own text is the displayed one, so the box shows its translation and original.
    state.candidates = [{ printing_id: "p:synthetic", revision_id: "rev:synthetic" }]
    state.undated = []
    prefsStore.set({ uiLanguage: "zh-TW", effectLanguage: "both" })
    await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    const section = document.querySelector('[data-field="effect"]') as HTMLElement
    const translated = within(section).getByText("合成新版譯文")
    const original = within(section).getByText("Synthetic selected effect")
    expect(translated).toBeVisible()
    expect(original).toBeVisible()
    expect(section.className).not.toContain("grid")
    expect(
      translated.compareDocumentPosition(original) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy()
  })
})

it("shows EVOLVE after the type tag using agreed face values", async () => {
  state.evolve = true
  state.originalName = true
  state.doubleFace = true
  await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
    initialEntries: ["/cards/BP-SYNTHETIC"],
  })
  const badges = screen.getAllByRole("img", { name: "進化" })
  expect(badges).toHaveLength(2)
  for (const classification of document.querySelectorAll("[data-card-classification]")) {
    expect(
      within(classification as HTMLElement)
        .getAllByRole("img")
        .map((image) => image.getAttribute("alt")),
    ).toEqual(["follower", "進化", expect.stringContaining("官方稀有度")])
  }
})

it("separates multiple traits with the printed classification joiner", async () => {
  state.summary = true
  state.traits = ["synthetic", "second"]
  await renderRoutes([{ path: "/cards/:cardNo/:slug?", ...card }], {
    initialEntries: ["/cards/BP-SYNTHETIC"],
  })
  expect(screen.getByText("synthetic・second")).toBeVisible()
})
