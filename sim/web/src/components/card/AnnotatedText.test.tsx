import { act, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { createAnnotatedTextResolver, type SelectedText, SnapshotError, textRuns } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { AnnotatedText } from "./AnnotatedText"
import { CardText } from "./CardText"

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("annotation presentation", () => {
  it("resolves a vocabulary reference by its kind and code", async () => {
    const value = {
      unit: { text: "手札", lang: "ja" },
      annotation: {
        occurrences: [
          {
            ordinal: 0,
            reference: { kind: "vocabulary", key: ["zone", "hand"] },
            ranges: [{ start: 0, end: 2 }],
            bold: false,
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
        emphasis={false}
      />,
    )
    await userEvent.click(screen.getByRole("button", { name: "手札" }))
    expect(screen.getByRole("complementary")).toHaveTextContent("原始區域標籤")
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
        client={client}
        sharedResolver={{ resolve }}
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
    const sharedResolver = { resolve }
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
            client={client}
            sharedResolver={sharedResolver}
            owner={{ kind: "face_revision", id: "revision" }}
            field="name"
          />
        </>
      )
    }
    await renderInRouter(<Harness />)
    await screen.findByText("原文字段未知")
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
