import { screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { renderInRouter } from "../../test-utils"
import { RarityBadges } from "./RarityBadges"

describe("accessible rarity badges", () => {
  it.each([
    ["zh-TW", "官方稀有度：SP"],
    ["ja", "公式レアリティ：SP"],
    ["en", "Official rarity: SP"],
  ] as const)(
    "describes the official rarity in %s even when the face says PR",
    async (language, label) => {
      await renderInRouter(
        <RarityBadges region="jp" rarityCode="sp" rarityBadge={{ style: "sp", text: "PR" }} />,
        { language },
      )
      expect(screen.getByRole("img", { name: label })).toHaveAttribute("title", label)
      expect(screen.getByRole("img")).toHaveAttribute("aria-label", label)
    },
  )

  it("shows only the base badge and official rarity for a Premium printing", async () => {
    await renderInRouter(<RarityBadges region="en" rarityCode="br" premium />, { language: "en" })
    const badge = screen.getByRole("img")
    expect(badge).toHaveAttribute("title", "Official rarity: BR")
    expect(badge).toHaveAttribute("width", "56")
    expect(document.querySelectorAll("img")).toHaveLength(1)
  })

  it.each([null, "-"])("shows no badge for uncoded Premium (%s)", async (rarityCode) => {
    await renderInRouter(<RarityBadges region="en" rarityCode={rarityCode} premium />)
    expect(screen.queryByRole("img")).not.toBeInTheDocument()
  })

  it.each([
    ["zh-TW", "官方稀有度：PR"],
    ["ja", "公式レアリティ：PR"],
    ["en", "Official rarity: PR"],
  ] as const)("shows unconfirmed PR as text in %s", async (language, label) => {
    await renderInRouter(<RarityBadges region="jp" rarityCode="pr" rarityBadge={null} />, {
      language,
    })
    const text = screen.getByRole("img", { name: label })
    expect(text).toHaveTextContent(/^PR$/u)
    expect(text).toHaveAttribute("title", label)
    expect(document.querySelector("img")).toBeNull()
  })

  it("shows only plain PR when a Premium printing has no appearance", async () => {
    await renderInRouter(<RarityBadges region="en" rarityCode="pr" premium />, { language: "en" })
    expect(screen.getByText("PR")).toBeVisible()
    expect(screen.getByRole("img")).toHaveAttribute("title", "Official rarity: PR")
    expect(document.querySelector("img")).toBeNull()
  })

  it("retains a tooltip target for an explicitly absent face label", async () => {
    await renderInRouter(
      <RarityBadges region="en" rarityCode="sl" rarityBadge={{ style: "none" }} />,
      { language: "en" },
    )
    expect(screen.getByRole("img", { name: "Official rarity: SL" })).toHaveAttribute(
      "title",
      "Official rarity: SL",
    )
    expect(document.querySelector("img")).toBeNull()
  })

  it("does not substitute plain PR for an explicitly absent label", async () => {
    await renderInRouter(
      <RarityBadges region="jp" rarityCode="pr" rarityBadge={{ style: "none" }} premium />,
    )
    expect(screen.queryByText("PR")).not.toBeInTheDocument()
    expect(document.querySelector("img")).toBeNull()
  })

  it("renders no image for official '-'", async () => {
    await renderInRouter(<RarityBadges region="jp" rarityCode="-" />)
    expect(screen.queryByRole("img")).not.toBeInTheDocument()
  })
})

it.each([
  ["zh-TW", "官方稀有度：宣傳"],
  ["ja", "公式レアリティ：プロモ"],
  ["en", "Official rarity: Promo"],
] as const)(
  "uses vocabulary in %s for the official rarity even when the badge says BR",
  async (language, expected) => {
    await renderInRouter(
      <RarityBadges
        region="en"
        rarityCode="pr"
        rarityBadge={{ style: "br", text: "BR" }}
        rarity={{
          original: { lang: "en", text: "Promo" },
          translations: { "zh-Hant": "宣傳", ja: "プロモ" },
        }}
      />,
      { language },
    )
    expect(screen.getByRole("img", { name: expected })).toHaveAttribute("title", expected)
  },
)

it("falls back to the vocabulary original when an interface translation is missing", async () => {
  await renderInRouter(
    <RarityBadges
      region="en"
      rarityCode="br"
      rarity={{ original: { lang: "en", text: "Bronze" }, translations: {} }}
    />,
    { language: "ja" },
  )
  expect(screen.getByRole("img", { name: "公式レアリティ：Bronze" })).toBeVisible()
})
