import { act, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"

import { createAnnotatedTextResolver, type SelectedText, textRuns } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { AnnotatedText } from "./AnnotatedText"
import { CardText } from "./CardText"

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("annotation presentation", () => {
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
  it("keeps the whole card-name reference and explanation entry when emphasis is off", async () => {
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
    const rendered = await renderInRouter(
      <AnnotatedText value={value} context={context} emphasis={true} />,
    )
    expect(rendered.container.querySelector("strong")?.textContent).toBe("手牌の王")
    rendered.rerender(<AnnotatedText value={value} context={context} emphasis={false} />)
    expect(rendered.container.querySelector("strong")).toBeNull()
    expect(screen.getByRole("button", { name: "手牌の王" })).toBeVisible()
    await userEvent.click(screen.getByRole("button", { name: "手牌の王" }))
    expect(screen.getByRole("complementary", { name: "查看術語說明" })).toHaveTextContent(
      "尚無公開說明",
    )
    expect(value.annotation.occurrences[0]?.reference).toEqual({
      kind: "card_name",
      term_id: "king",
    })
  })
  it("renders and copies the actual native C/P/R text while the preference changes only emphasis", async () => {
    const { client } = await annotatedOrigin()
    await client.load()
    const copy = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: copy },
    })
    const { container } = await renderInRouter(
      <CardText client={client} owner={{ kind: "face_revision", id: "revision" }} field="name" />,
    )
    expect(screen.getByRole("status")).toHaveTextContent("文字與標註載入中")
    await screen.findByText("translation")
    expect(container.querySelector("strong")?.textContent).toBe("Synthetic")
    expect(screen.getByText(/專案譯文/)).toHaveTextContent("非官方")
    await userEvent.click(screen.getByRole("button", { name: "複製文字" }))
    expect(copy).toHaveBeenLastCalledWith("Synthetic translation")
    await userEvent.click(screen.getByRole("button", { name: "Synthetic" }))
    expect(screen.getByRole("complementary", { name: "查看術語說明" })).toBeVisible()
    act(() => {
      prefsStore.set({ termEmphasis: false })
    })
    await vi.waitFor(() => {
      expect(container.querySelector("strong")).toBeNull()
    })
    expect(screen.getByRole("button", { name: "Synthetic" })).toBeVisible()
    await userEvent.click(screen.getByRole("button", { name: "複製文字" }))
    expect(copy).toHaveBeenLastCalledWith("Synthetic translation")
    const value = await createAnnotatedTextResolver(client).resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
      "zh-Hant",
    )
    expect(value?.original.annotation?.["occurrences"]).toHaveLength(1)
  })
})
