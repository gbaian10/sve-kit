import { screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { StrictMode } from "react"
import { describe, expect, it, vi } from "vitest"

import type { KeywordInfo } from "../../data"
import type { CardTextVocabulary } from "../../domain/cardText"
import { renderInRouter } from "../../test-utils"
import { CardText } from "./CardText"

const vocabulary: CardTextVocabulary = {
  spellings: [
    {
      symbolId: "sym:fanfare",
      code: "fanfare",
      lang: "ja",
      prefix: "ファンファーレ",
      suffix: "",
      parse: "literal",
      variables: [],
    },
  ],
  keywords: [
    { keywordId: "kw:ward", lang: "ja", name: "守護" },
    { keywordId: "kw:ward", lang: "zh-Hant", name: "守護" },
  ],
}

function render(definition: KeywordInfo["definition"]) {
  const keyword = (id: string): KeywordInfo | undefined =>
    id === "kw:ward" ? { id, name: () => "守護", definition } : undefined
  return renderInRouter(
    <StrictMode>
      <CardText
        text="{ファンファーレ}【守護】を得る。"
        lang="ja"
        vocabulary={vocabulary}
        symbolLocalization={() => ({ name: "入場曲" })}
        keyword={keyword}
        uiLang="zh-Hant"
        symbolLabels={false}
      />
    </StrictMode>,
  )
}

describe("CardText", () => {
  it("draws the icon with its localized name and explains a keyword in place, even under StrictMode", async () => {
    const definition = vi.fn(() => Promise.resolve("守護を持つフォロワーがいる間…"))
    await render(definition)
    expect(screen.getByRole("img", { name: "入場曲" })).toBeInTheDocument()
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "守護" }))
    expect(await screen.findByText("守護を持つフォロワーがいる間…")).toBeInTheDocument()
    expect(screen.getByRole("link", { name: "找有【守護】的卡" })).toHaveAttribute(
      "href",
      "/cards?mech=kw%3Award",
    )
    expect(definition).toHaveBeenCalledTimes(1)
  })

  it("tells a failed fetch apart from a missing definition and retries on the next open", async () => {
    const definition = vi
      .fn()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce(undefined)
    await render(definition)
    const user = userEvent.setup()
    const chip = screen.getByRole("button", { name: "守護" })
    await user.click(chip)
    expect(await screen.findByText("解說載入失敗，再點一次重試")).toBeInTheDocument()
    await user.click(chip)
    await user.click(chip)
    await waitFor(() => {
      expect(screen.getByText("尚無解說")).toBeInTheDocument()
    })
    expect(definition).toHaveBeenCalledTimes(2)
  })
})
