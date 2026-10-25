import { act, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { afterEach, describe, expect, it, vi } from "vitest"

import {
  type CardSummary,
  createAnnotatedTextResolver,
  type SelectedText,
  SnapshotError,
  textRuns,
} from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { AnnotatedText } from "./AnnotatedText"
import { CardLinkContext } from "./card-link-context"
import { CardText } from "./CardText"

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

// An unloaded client: linked cards keep their image placeholders, which is all these cases need.
const client = async () => (await annotatedOrigin()).client

const summary = (
  over: Pick<CardSummary, "cardId" | "printingId" | "cardNo" | "region"> & Partial<CardSummary>,
): CardSummary => ({
  faceId: `f:${over.cardId}`,
  classCode: null,
  cost: null,
  attack: null,
  defense: null,
  name: { original: { lang: "ja", text: "手牌の王" }, translations: { "zh-Hant": "手牌之王" } },
  ...over,
})

const cardName = (text: string, termId: string, bold: boolean | null = true) => ({
  unit: { text, lang: "ja" },
  annotation: {
    occurrences: [
      {
        ordinal: 0,
        reference: { kind: "card_name", term_id: termId },
        ranges: [{ start: 0, end: Array.from(text).length }],
        bold,
      },
    ],
  },
})

describe("annotation presentation", () => {
  it("leaves a vocabulary reference as plain text", async () => {
    const value = {
      unit: { text: "手札", lang: "ja" },
      annotation: {
        occurrences: [
          {
            ordinal: 0,
            reference: { kind: "vocabulary", key: ["zone", "hand"] },
            ranges: [{ start: 0, end: 2 }],
            bold: true,
          },
        ],
      },
    }
    await renderInRouter(
      <AnnotatedText
        value={value}
        context={{
          original: value,
          concepts: [],
          explanations: [],
          cards: [],
          vocabulary: [{ kind: "zone", code: "hand", unit: { text: "原始區域標籤", lang: "ja" } }],
        }}
        emphasis={true}
        client={await client()}
        edition="jp"
      />,
    )
    expect(screen.getByText("手札").tagName).toBe("STRONG")
    expect(screen.queryByRole("button")).not.toBeInTheDocument()
    expect(screen.queryByRole("link")).not.toBeInTheDocument()
  })

  it("draws an annotated keyword marker as one official icon without an entry", async () => {
    // The keyword range covers only the word inside the braces, as the real annotations do.
    const value = {
      unit: { text: "{ファンファーレ}{コスト1}：1枚引く。", lang: "ja" },
      annotation: {
        occurrences: [
          {
            ordinal: 0,
            reference: { kind: "vocabulary", key: ["ability", "fanfare"] },
            ranges: [{ start: 1, end: 8 }],
            bold: true,
          },
        ],
      },
    }
    const { container } = await renderInRouter(
      <AnnotatedText
        value={value}
        context={{ original: value, concepts: [], explanations: [], cards: [] }}
        emphasis={true}
        client={await client()}
        edition="jp"
      />,
    )
    expect(screen.getAllByRole("img", { name: "ファンファーレ" })).toHaveLength(1)
    expect(container.querySelector("p")).toHaveTextContent("：1枚引く。")
    expect(screen.queryByRole("button")).not.toBeInTheDocument()
  })

  it.each([
    [new SnapshotError("public-annotation/reference", "invalid reference"), "文字資料無法通過驗證"],
    [new Error("network unavailable"), "文字載入失敗，請稍後再試。"],
  ])("distinguishes validation and temporary loading failures", async (error, message) => {
    const { client } = await annotatedOrigin()
    await client.load()
    const resolve = vi.fn().mockRejectedValue(error)
    await renderInRouter(
      <CardText
        edition="jp"
        client={client}
        sharedResolver={{ resolve, peek: () => undefined }}
        owner={{ kind: "face_revision", id: "revision" }}
        field="name"
      />,
    )
    expect(await screen.findByRole("alert")).toHaveTextContent(message)
  })

  it("does not repeat a request when rendering an equal owner object", async () => {
    const { client } = await annotatedOrigin()
    await client.load()
    const resolve = vi.fn().mockResolvedValue(null)
    const sharedResolver = { resolve, peek: () => undefined }
    function Harness() {
      const [revision, setRevision] = useState(0)
      return (
        <>
          <button
            onClick={() => {
              setRevision((value) => value + 1)
            }}
          >
            rerender {revision}
          </button>
          <CardText
            edition="jp"
            client={client}
            sharedResolver={sharedResolver}
            owner={{ kind: "face_revision", id: "revision" }}
            field="name"
          />
        </>
      )
    }
    await renderInRouter(<Harness />)
    await screen.findByText("此段文字尚未提供")
    const calls = resolve.mock.calls.length
    await userEvent.click(screen.getByRole("button", { name: "rerender 0" }))
    expect(calls).toBeGreaterThan(0)
    expect(resolve).toHaveBeenCalledTimes(calls)
  })

  it("slices exact codepoints without NFC, NFKC or UTF-16 offsets", () => {
    const value = {
      unit: { text: "A😀Ｂé手牌", lang: "ja" },
      annotation: {
        occurrences: [
          {
            ordinal: 0,
            reference: { kind: "glossary", key: "hand" },
            ranges: [{ start: 5, end: 7 }],
            bold: true,
          },
        ],
      },
    }
    expect(textRuns(value).map((run) => run.text)).toEqual(["A😀Ｂé", "手牌"])
    expect(
      textRuns(value)
        .map((run) => run.text)
        .join(""),
    ).toBe(value.unit.text)
  })
  it("keeps an unlinked card name bold but not clickable, and plain when emphasis is off", async () => {
    const value = cardName("手牌の王", "king")
    const context: SelectedText = {
      original: value,
      concepts: [{ id: "king", category: "card_name", explanations: [], card_ids: [] }],
      explanations: [],
      cards: [],
    }
    const snapshotClient = await client()
    const rendered = await renderInRouter(
      <AnnotatedText
        value={value}
        context={context}
        emphasis={true}
        client={snapshotClient}
        edition="jp"
      />,
    )
    expect(rendered.container.querySelector("strong")?.textContent).toBe("手牌の王")
    expect(screen.queryByRole("link")).not.toBeInTheDocument()
    rendered.rerender(
      <AnnotatedText
        value={value}
        context={context}
        emphasis={false}
        client={snapshotClient}
        edition="jp"
      />,
    )
    expect(rendered.container.querySelector("strong")).toBeNull()
    expect(screen.getByText("手牌の王")).toBeVisible()
    expect(screen.queryByRole("link")).not.toBeInTheDocument()
    expect(value.annotation.occurrences[0]?.reference).toEqual({
      kind: "card_name",
      term_id: "king",
    })
  })

  it("previews a single linked card on hover and goes to it on click", async () => {
    prefsStore.set({ uiLanguage: "zh-TW" })
    const value = cardName("手牌の王", "king")
    const context: SelectedText = {
      original: value,
      concepts: [{ id: "king", category: "card_name", explanations: [], card_ids: ["c:king"] }],
      explanations: [],
      cards: [
        summary({ cardId: "c:king", printingId: "p:king", cardNo: "BP01-001", region: "jp" }),
      ],
    }
    const { router } = await renderInRouter(
      <AnnotatedText
        value={value}
        context={context}
        emphasis={true}
        client={await client()}
        edition="jp"
      />,
    )
    const link = screen.getByRole("link", { name: "手牌の王" })
    expect(link).toHaveAttribute("href", "/cards/BP01-001")
    expect(link.querySelector("strong")).toHaveTextContent("手牌の王")
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument()
    await userEvent.hover(link)
    const preview = screen.getByRole("tooltip")
    expect(preview).toHaveTextContent("手牌之王")
    expect(preview).toHaveTextContent("BP01-001")
    expect(preview).not.toHaveTextContent("JP")
    expect(preview.closest("p")).toBeNull()
    expect(preview.querySelector("[data-image-printing='p:king']")).toBeInTheDocument()
    // The link describes itself by the preview only while the preview is up.
    expect(preview.id).not.toBe("")
    expect(link).toHaveAttribute("aria-describedby", preview.id)
    await userEvent.unhover(link)
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument()
    expect(link).not.toHaveAttribute("aria-describedby")
    await userEvent.tab()
    expect(link).toHaveFocus()
    expect(link).toHaveAttribute("aria-describedby", screen.getByRole("tooltip").id)
    await userEvent.tab()
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument()
    await userEvent.click(link)
    expect(router.state.location.pathname).toBe("/cards/BP01-001")
  })

  it("opens a single reference where the surrounding frame says, not on the card's page", async () => {
    const value = cardName("手牌の王", "king")
    const context: SelectedText = {
      original: value,
      concepts: [{ id: "king", category: "card_name", explanations: [], card_ids: ["c:king"] }],
      explanations: [],
      cards: [
        summary({ cardId: "c:king", printingId: "p:king", cardNo: "BP01-001", region: "jp" }),
      ],
    }
    const { router } = await renderInRouter(
      <CardLinkContext
        value={(card) => ({ to: `/cards?card=${card.cardNo}`, state: { opened: card.cardNo } })}
      >
        <AnnotatedText
          value={value}
          context={context}
          emphasis={true}
          client={await client()}
          edition="jp"
        />
      </CardLinkContext>,
    )
    const link = screen.getByRole("link", { name: "手牌の王" })
    expect(link).toHaveAttribute("href", "/cards?card=BP01-001")
    await userEvent.click(link)
    expect(router.state.location.pathname).toBe("/cards")
    expect(router.state.location.search).toBe("?card=BP01-001")
    expect(router.state.location.state).toEqual({ opened: "BP01-001" })
  })

  it("opens each card of a reference list where the surrounding frame says", async () => {
    const value = cardName("手牌の王", "king")
    const context: SelectedText = {
      original: value,
      concepts: [
        { id: "king", category: "card_name", explanations: [], card_ids: ["c:king", "c:other"] },
      ],
      explanations: [],
      cards: [
        summary({ cardId: "c:king", printingId: "p:king", cardNo: "BP01-001", region: "jp" }),
        summary({ cardId: "c:other", printingId: "p:other", cardNo: "BP01-002", region: "jp" }),
      ],
    }
    const { router } = await renderInRouter(
      <CardLinkContext value={(card) => ({ to: `/cards?card=${card.cardNo}` })}>
        <AnnotatedText
          value={value}
          context={context}
          emphasis={true}
          client={await client()}
          edition="jp"
        />
      </CardLinkContext>,
    )
    await userEvent.click(screen.getByRole("button", { name: "手牌の王" }))
    const links = within(screen.getByRole("complementary")).getAllByRole("link")
    expect(links.map((link) => link.getAttribute("href"))).toEqual([
      "/cards?card=BP01-001",
      "/cards?card=BP01-002",
    ])
    await userEvent.click(links[1] as HTMLElement)
    expect(router.state.location.pathname).toBe("/cards")
    expect(router.state.location.search).toBe("?card=BP01-002")
  })

  it("lists several linked cards with image, preferred name and edition, closing by × or Escape", async () => {
    prefsStore.set({ uiLanguage: "zh-TW" })
    const value = cardName("手牌の王", "king")
    const context: SelectedText = {
      original: value,
      concepts: [
        { id: "king", category: "card_name", explanations: [], card_ids: ["c:king", "c:other"] },
        { id: "unrelated", category: "card_name", explanations: [], card_ids: ["c:unrelated"] },
      ],
      explanations: [],
      cards: [
        summary({ cardId: "c:king", printingId: "p:king", cardNo: "BP01-001", region: "jp" }),
        summary({
          cardId: "c:other",
          printingId: "p:other",
          cardNo: "_provisional",
          cardNoState: "provisional",
          intId: 42,
          region: "en",
          name: { original: { lang: "en", text: "Hand King" }, translations: {} },
        }),
        summary({
          cardId: "c:unrelated",
          printingId: "p:unrelated",
          cardNo: "BP01-009",
          region: "jp",
        }),
      ],
    }
    await renderInRouter(
      <AnnotatedText
        value={value}
        context={context}
        emphasis={true}
        client={await client()}
        edition="jp"
      />,
    )
    expect(screen.queryByRole("link")).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole("button", { name: "手牌の王" }))
    const list = screen.getByRole("complementary", { name: "卡名引用" })
    const links = within(list).getAllByRole("link")
    expect(links.map((link) => link.getAttribute("href"))).toEqual([
      "/cards/BP01-001",
      "/cards/_provisional/42",
    ])
    expect(list.querySelectorAll("[data-image-printing]")).toHaveLength(2)
    expect(links[0]).toHaveTextContent("手牌之王")
    expect(links[0]).not.toHaveTextContent("JP")
    expect(links[1]).toHaveTextContent("Hand King")
    expect(links[1]).toHaveTextContent("EN")
    expect(list).not.toHaveTextContent("BP01-009")
    act(() => {
      prefsStore.set({ nameDisplay: "original" })
    })
    expect(within(list).getAllByRole("link")[0]).toHaveTextContent("手牌の王")
    expect(within(list).getAllByRole("link")[0]).not.toHaveTextContent("手牌之王")
    expect(within(list).queryByText("關閉說明")).not.toBeInTheDocument()
    // Closing by × or by Escape from inside the list hands focus back to the reference.
    const trigger = screen.getByRole("button", { name: "手牌の王" })
    await userEvent.click(within(list).getByRole("button", { name: "關閉說明" }))
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument()
    expect(trigger).toHaveFocus()
    await userEvent.click(trigger)
    const reopened = screen.getByRole("complementary")
    expect(reopened).toBeVisible()
    within(reopened).getByRole("button", { name: "關閉說明" }).focus()
    await userEvent.keyboard("{Escape}")
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument()
    expect(trigger).toHaveFocus()
  })

  it("treats one card with printings in both editions as one target, preferring the shown edition", async () => {
    const value = cardName("手牌の王", "king")
    const cards = [
      summary({ cardId: "c:king", printingId: "p:jp", cardNo: "BP01-001", region: "jp" }),
      summary({ cardId: "c:king", printingId: "p:en", cardNo: "BP01-001EN", region: "en" }),
    ]
    const context: SelectedText = {
      original: value,
      concepts: [{ id: "king", category: "card_name", explanations: [], card_ids: ["c:king"] }],
      explanations: [],
      cards,
    }
    const snapshotClient = await client()
    const rendered = await renderInRouter(
      <AnnotatedText
        value={value}
        context={context}
        emphasis={true}
        client={snapshotClient}
        edition="en"
      />,
    )
    expect(screen.queryByRole("button")).not.toBeInTheDocument()
    expect(screen.getByRole("link", { name: "手牌の王" })).toHaveAttribute(
      "href",
      "/cards/BP01-001EN",
    )
    // Without a printing in the shown edition, the other edition's stands in.
    rendered.unmount()
    await renderInRouter(
      <AnnotatedText
        value={value}
        context={{ ...context, cards: cards.slice(0, 1) }}
        emphasis={true}
        client={snapshotClient}
        edition="en"
      />,
    )
    expect(screen.getByRole("link", { name: "手牌の王" })).toHaveAttribute(
      "href",
      "/cards/BP01-001",
    )
    await userEvent.hover(screen.getByRole("link", { name: "手牌の王" }))
    expect(screen.getByRole("tooltip")).toHaveTextContent("BP01-001 · JP")
  })

  it("re-derives an open multi-card list and a direct link when the edition changes", async () => {
    const value = cardName("手牌の王", "king")
    const both = (cardId: string, no: string) => [
      summary({ cardId, printingId: `p:${no}-jp`, cardNo: no, region: "jp" }),
      summary({ cardId, printingId: `p:${no}-en`, cardNo: `${no}EN`, region: "en" }),
    ]
    const context: SelectedText = {
      original: value,
      concepts: [
        { id: "king", category: "card_name", explanations: [], card_ids: ["c:king", "c:second"] },
      ],
      explanations: [],
      cards: [...both("c:king", "BP01-001"), ...both("c:second", "BP01-002")],
    }
    const snapshotClient = await client()
    function Harness() {
      const [edition, setEdition] = useState<"jp" | "en">("jp")
      return (
        <>
          <button
            onClick={() => {
              setEdition((value) => (value === "jp" ? "en" : "jp"))
            }}
          >
            switch edition
          </button>
          <AnnotatedText
            value={value}
            context={context}
            emphasis={true}
            client={snapshotClient}
            edition={edition}
          />
        </>
      )
    }
    await renderInRouter(<Harness />)
    const hrefs = () =>
      within(screen.getByRole("complementary"))
        .getAllByRole("link")
        .map((link) => link.getAttribute("href"))
    await userEvent.click(screen.getByRole("button", { name: "手牌の王" }))
    expect(hrefs()).toEqual(["/cards/BP01-001", "/cards/BP01-002"])
    await userEvent.click(screen.getByRole("button", { name: "switch edition" }))
    expect(hrefs()).toEqual(["/cards/BP01-001EN", "/cards/BP01-002EN"])
    expect(screen.getByRole("complementary")).not.toHaveTextContent("· EN")
    await userEvent.click(screen.getByRole("button", { name: "switch edition" }))
    expect(hrefs()).toEqual(["/cards/BP01-001", "/cards/BP01-002"])
  })

  it("drops an open list whose reference stops listing several cards, without moving focus", async () => {
    const value = cardName("手牌の王", "king")
    const two = [
      summary({ cardId: "c:king", printingId: "p:king", cardNo: "BP01-001", region: "jp" }),
      summary({ cardId: "c:second", printingId: "p:second", cardNo: "BP01-002", region: "jp" }),
    ]
    const concept = {
      id: "king",
      category: "card_name",
      explanations: [],
      card_ids: ["c:king", "c:second"],
    }
    const snapshotClient = await client()
    function Harness() {
      const [cards, setCards] = useState(two)
      return (
        <>
          <button
            onClick={() => {
              setCards(two.slice(0, 1))
            }}
          >
            narrow
          </button>
          <AnnotatedText
            value={value}
            context={{ original: value, concepts: [concept], explanations: [], cards }}
            emphasis={true}
            client={snapshotClient}
            edition="jp"
          />
        </>
      )
    }
    await renderInRouter(<Harness />)
    await userEvent.click(screen.getByRole("button", { name: "手牌の王" }))
    expect(screen.getByRole("complementary")).toBeVisible()
    const narrow = screen.getByRole("button", { name: "narrow" })
    await userEvent.click(narrow)
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument()
    expect(screen.getByRole("link", { name: "手牌の王" })).toHaveAttribute(
      "href",
      "/cards/BP01-001",
    )
    expect(narrow).toHaveFocus()
  })

  it("links a listed trait to the filtered list and leaves other glossary terms as text", async () => {
    const value = {
      unit: { text: "妖精殺手の守護者", lang: "ja" },
      annotation: {
        occurrences: [
          {
            ordinal: 0,
            reference: { kind: "glossary", key: "term:trait.pixie" },
            ranges: [{ start: 0, end: 2 }],
            bold: null,
          },
          {
            ordinal: 1,
            reference: { kind: "glossary", key: "term:trait.killer" },
            ranges: [{ start: 2, end: 4 }],
            bold: true,
          },
          {
            ordinal: 2,
            reference: { kind: "glossary", key: "term:ability.guard" },
            ranges: [{ start: 5, end: 8 }],
            bold: true,
          },
        ],
      },
    }
    const context: SelectedText = {
      original: value,
      concepts: [
        { id: "term:trait.pixie", category: "trait", explanations: [], card_ids: [] },
        { id: "term:trait.killer", category: "trait", explanations: [], card_ids: [] },
        { id: "term:ability.guard", category: "ability", explanations: [], card_ids: [] },
      ],
      explanations: [],
      cards: [],
      vocabulary: [{ kind: "trait", code: "pixie", unit: { text: "妖精", lang: "ja" } }],
    }
    const { container } = await renderInRouter(
      <AnnotatedText
        value={value}
        context={context}
        emphasis={true}
        client={await client()}
        edition="en"
      />,
    )
    // Only the trait the list can filter by links; its style is its own, not the card-name dots.
    const links = screen.getAllByRole("link")
    expect(links).toHaveLength(1)
    expect(links[0]).toHaveTextContent("妖精")
    expect(links[0]).toHaveAttribute("href", "/cards?trait=pixie&ed=en")
    expect(links[0]).toHaveAccessibleDescription("查看這個類型的卡")
    expect(links[0]?.querySelector("strong")).toBeNull()
    // The unlisted trait and the ability stay bold text without any control.
    expect([...container.querySelectorAll("strong")].map((node) => node.textContent)).toEqual([
      "殺手",
      "守護者",
    ])
    expect(screen.queryByRole("button")).not.toBeInTheDocument()
  })

  it("keeps a rule term as text with or without emphasis", async () => {
    const value = {
      unit: { text: "ターン終了時", lang: "ja" },
      annotation: {
        occurrences: [
          {
            ordinal: 0,
            reference: { kind: "glossary", key: "term:rule_term.end_of_turn" },
            ranges: [{ start: 0, end: 6 }],
            bold: true,
          },
        ],
      },
    }
    const context: SelectedText = {
      original: value,
      concepts: [
        { id: "term:rule_term.end_of_turn", category: "rule_term", explanations: [], card_ids: [] },
      ],
      explanations: [],
      cards: [],
    }
    const snapshotClient = await client()
    const rendered = await renderInRouter(
      <AnnotatedText
        value={value}
        context={context}
        emphasis={true}
        client={snapshotClient}
        edition="jp"
      />,
    )
    expect(screen.getByText("ターン終了時").tagName).toBe("STRONG")
    expect(screen.queryByRole("link")).not.toBeInTheDocument()
    expect(screen.queryByRole("button")).not.toBeInTheDocument()
    rendered.rerender(
      <AnnotatedText
        value={value}
        context={context}
        emphasis={false}
        client={snapshotClient}
        edition="jp"
      />,
    )
    expect(screen.getByText("ターン終了時").tagName).toBe("SPAN")
  })

  it("renders the actual native C/P/R text while the preference changes only emphasis", async () => {
    prefsStore.set({ nameDisplay: "translated" })
    const { client } = await annotatedOrigin()
    await client.load()
    const { container } = await renderInRouter(
      <CardText
        edition="jp"
        client={client}
        owner={{ kind: "face_revision", id: "revision" }}
        field="name"
      />,
    )
    expect(screen.getByRole("status")).toHaveTextContent("文字與標註載入中")
    await screen.findByText("translation")
    expect(container.querySelector("strong")?.textContent).toBe("Synthetic")
    expect(screen.getByText(/專案譯文/)).toHaveTextContent("非官方")
    // The keyword concept behind the name is emphasis only: no control to open.
    expect(screen.queryByRole("button")).not.toBeInTheDocument()
    act(() => {
      prefsStore.set({ termEmphasis: false })
    })
    await vi.waitFor(() => {
      expect(container.querySelector("strong")).toBeNull()
    })
    expect(screen.getByText("Synthetic")).toBeVisible()
    const value = await createAnnotatedTextResolver(client).resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
      "zh-Hant",
    )
    expect(value?.original.annotation?.["occurrences"]).toHaveLength(1)
  })
})
