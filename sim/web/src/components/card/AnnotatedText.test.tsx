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
    const value = {
      unit: { text: "手牌の王", lang: "ja" },
      annotation: {
        occurrences: [
          {
            ordinal: 0,
            reference: { kind: "card_name", term_id: "king" },
            ranges: [{ start: 0, end: 4 }],
            bold: true,
          },
        ],
      },
    }
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
    expect(rendered.container.querySelector("strong")).toBeNull()
    expect(screen.getByText("手牌の王")).toBeVisible()
    expect(screen.queryByRole("button")).not.toBeInTheDocument()
    expect(value.annotation.occurrences[0]?.reference).toEqual({
      kind: "card_name",
      term_id: "king",
    })
  })

  it("opens linked cards with their image and name, tags other editions, and closes by × or Escape", async () => {
    const value = {
      unit: { text: "手牌の王", lang: "ja" },
      annotation: {
        occurrences: [
          {
            ordinal: 0,
            reference: { kind: "card_name", term_id: "king" },
            ranges: [{ start: 0, end: 4 }],
            bold: true,
          },
        ],
      },
    }
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
    prefsStore.set({ uiLanguage: "zh-TW" })
    await renderInRouter(
      <AnnotatedText
        value={value}
        context={context}
        emphasis={true}
        client={await client()}
        edition="jp"
      />,
    )
    await userEvent.click(screen.getByRole("button", { name: "手牌の王" }))
    const entry = screen.getByRole("complementary", { name: "查看術語說明" })
    expect(entry).toHaveTextContent("卡名引用")
    const links = within(entry).getAllByRole("link")
    expect(links.map((link) => link.getAttribute("href"))).toEqual([
      "/cards/BP01-001",
      "/cards/_provisional/42",
    ])
    // Each row carries the small image slot, the name as the preference shows it, and the number.
    expect(entry.querySelectorAll("[data-image-printing]")).toHaveLength(2)
    expect(links[0]).toHaveTextContent("手牌之王")
    expect(links[0]).not.toHaveTextContent("JP")
    expect(links[1]).toHaveTextContent("Hand King")
    expect(links[1]).toHaveTextContent("EN")
    expect(entry).not.toHaveTextContent("BP01-009")
    act(() => {
      prefsStore.set({ nameDisplay: "original" })
    })
    expect(within(entry).getAllByRole("link")[0]).toHaveTextContent("手牌の王")
    expect(within(entry).getAllByRole("link")[0]).not.toHaveTextContent("手牌之王")
    expect(within(entry).queryByText("關閉說明")).not.toBeInTheDocument()
    await userEvent.click(within(entry).getByRole("button", { name: "關閉說明" }))
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole("button", { name: "手牌の王" }))
    expect(screen.getByRole("complementary")).toBeVisible()
    await userEvent.keyboard("{Escape}")
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument()
  })

  it("lights only the references that have content and keeps a null bold unemphasised", async () => {
    const value = {
      unit: { text: "妖精妖精", lang: "ja" },
      annotation: {
        occurrences: [
          {
            ordinal: 0,
            reference: { kind: "card_name", term_id: "term:name.fairy" },
            ranges: [{ start: 0, end: 2 }],
            bold: true,
          },
          {
            ordinal: 1,
            reference: { kind: "glossary", key: "term:trait.pixie" },
            ranges: [{ start: 2, end: 4 }],
            bold: null,
          },
        ],
      },
    }
    const explanation = { kind: "keyword", id: "kw:pixie" }
    const context: SelectedText = {
      original: value,
      concepts: [
        { id: "term:name.fairy", category: "card_name", explanations: [], card_ids: [] },
        { id: "term:trait.pixie", category: "trait", explanations: [explanation], card_ids: [] },
      ],
      explanations: [
        { reference: explanation, row: {}, unit: { text: "合成類型說明", lang: "zh-Hant" } },
      ],
      cards: [],
    }
    const { container } = await renderInRouter(
      <AnnotatedText
        value={value}
        context={context}
        emphasis={true}
        client={await client()}
        edition="jp"
      />,
    )
    // The same word twice: the unlinked card name stays bold text; the explained trait opens.
    const buttons = screen.getAllByRole("button")
    expect(buttons).toHaveLength(1)
    expect(buttons[0]).toHaveTextContent("妖精")
    expect(container.querySelectorAll("strong")).toHaveLength(1)
    expect(buttons[0]).not.toContainElement(container.querySelector("strong"))
    await userEvent.click(buttons[0] as HTMLElement)
    const entry = screen.getByRole("complementary", { name: "查看術語說明" })
    expect(entry).toHaveTextContent("類型")
    expect(entry).toHaveTextContent("合成類型說明")
    expect(entry).not.toHaveTextContent("卡名引用")
  })

  it("renders the actual native C/P/R text while the preference changes only emphasis", async () => {
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
    await userEvent.click(screen.getByRole("button", { name: "Synthetic" }))
    expect(screen.getByRole("complementary", { name: "查看術語說明" })).toBeVisible()
    act(() => {
      prefsStore.set({ termEmphasis: false })
    })
    await vi.waitFor(() => {
      expect(container.querySelector("strong")).toBeNull()
    })
    expect(screen.getByRole("button", { name: "Synthetic" })).toBeVisible()
    const value = await createAnnotatedTextResolver(client).resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
      "zh-Hant",
    )
    expect(value?.original.annotation?.["occurrences"]).toHaveLength(1)
  })
})
