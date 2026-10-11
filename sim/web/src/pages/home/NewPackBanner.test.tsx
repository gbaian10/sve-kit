import { screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { renderInRouter } from "../../test-utils"
import { NewPackBanner } from "./NewPackBanner"

// Positive state awaiting data: the real snapshot does not link products to sets yet, so the banner
// with counts is only shown here, with synthetic values.
const pack = {
  code: "syn01",
  publicCode: "SYN01",
  name: { text: "Synthetic set", lang: "en" as const },
  releasedOn: "2026-05-01",
}
const counts = {
  phase: "complete" as const,
  counts: { cards: 1234, translated: 3, proofreading: 1 },
}

describe("new pack banner", () => {
  it("shows real counts without a denominator or a percentage", async () => {
    await renderInRouter(
      <NewPackBanner pack={pack} counts={counts} countTranslations locale="zh-TW" />,
    )
    const banner = screen.getByRole("region", { name: "Synthetic set" })
    expect(banner).toHaveTextContent("已收錄 1,234 種")
    expect(banner).toHaveTextContent("有繁中譯名 3")
    expect(banner).toHaveTextContent("其中 1 個待校對")
    expect(banner.textContent).not.toMatch(/%|／|\//u)
    expect(screen.getByRole("link", { name: "看卡表" })).toHaveAttribute("href", "/sets/SYN01")
  })

  it("leaves Traditional Chinese counts out of other interfaces", async () => {
    await renderInRouter(
      <NewPackBanner pack={pack} counts={counts} countTranslations={false} locale="ja" />,
      { language: "ja" },
    )
    const banner = screen.getByRole("region", { name: "Synthetic set" })
    expect(banner).toHaveTextContent("収録 1,234 種")
    expect(banner).not.toHaveTextContent("繁体字")
  })

  it("says loading instead of zero until the set's cards are complete", async () => {
    await renderInRouter(
      <NewPackBanner pack={pack} counts={{ phase: "pending" }} countTranslations locale="zh-TW" />,
    )
    expect(screen.getByRole("status")).toHaveTextContent("收錄數計算中")
    expect(screen.getByRole("region", { name: "Synthetic set" }).textContent).not.toMatch(/\d+ 種/u)
  })
})
