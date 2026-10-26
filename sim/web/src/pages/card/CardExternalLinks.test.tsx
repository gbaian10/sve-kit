import { act, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { I18nextProvider } from "react-i18next"
import { afterEach, describe, expect, it, vi } from "vitest"

import type { Catalog, SnapshotClient } from "../../data"
import { createI18n, type UiLanguage } from "../../i18n"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { CardExternalLinks } from "./CardExternalLinks"

const digitalCards = [
  { id: "d:1", game: "sv1", official_id: "900211010" },
  { id: "d:2", game: "sv1", official_id: "810144010" },
  { id: "d:3", game: "svwb", official_id: "90021110" },
]
const config = {
  digital_endpoints: [
    {
      game: "sv1",
      card_url_template: "https://portal.example/card/{official_id}?lang={provider_lang}",
      language_map: { en: "en", ja: "ja", "zh-Hant": "zh-tw" },
    },
    {
      game: "svwb",
      card_url_template: "https://wb.example/{provider_lang}/card/?card_id={official_id}",
      language_map: { en: "en", ja: "ja", "zh-Hant": "cht" },
    },
  ],
}
const catalog = {
  index: {
    card: (id: string) => (["c:one", "c:two"].includes(id) ? { home_set_id: "BP01" } : undefined),
    printing: () => undefined,
    printingsOf: () => [],
  },
} as unknown as Catalog

function catalogWithPrintings(printings: readonly Record<string, unknown>[]): Catalog {
  return {
    index: {
      card: () => ({
        home_set_id: "BP01",
        regions: [{ region: "en", default_printing_id: "p:en" }],
      }),
      printing: (id: string) => printings.find((printing) => printing["id"] === id),
      printingsOf: () => printings,
    },
  } as unknown as Catalog
}

const jpPrinting = (url = "https://shadowverse-evolve.com/cards/1/") => ({
  id: "p:jp",
  region: "jp",
  card_no: "BP01-001",
  faces: [{ observations: [{ source_url: url }] }],
})
const enPrinting = (url = "https://en.shadowverse-evolve.com/cards/1/") => ({
  id: "p:en",
  region: "en",
  card_no: "BP01-001",
  faces: [{ observations: [{ source_url: url }] }],
})

const linkRow = (digital: string, relation = "same_card") => ({
  id: `l:${digital}`,
  card_id: "c:one",
  face_id: null,
  digital_card_id: digital,
  digital_phase: null,
  relation,
  effect_similarity: null,
  review_level: "model_reviewed",
})

function makeClient(links: ReturnType<typeof linkRow>[], failures = { count: 0 }) {
  let snapshot = {
    manifestHash: "h1",
    config,
    files: new Map([
      ["links", { purpose: "engine", row_counts: [["digital_link", "home_set", "BP01"]] }],
      ["cards", { purpose: "engine", row_counts: [["digital_card", "global", null]] }],
    ]),
  }
  const fragments = vi.fn((key: string) => {
    if (failures.count > 0 && key === "links") {
      failures.count -= 1
      return Promise.reject(new Error("network"))
    }
    const rows = key === "links" ? links : digitalCards
    return Promise.resolve([
      { file: key, table: key === "links" ? "digital_link" : "digital_card", rows },
    ])
  })
  const client = { base: "/test", snapshot: () => snapshot, fragments } as unknown as SnapshotClient
  return {
    client,
    fragments,
    replaceSnapshot: (manifestHash: string) => {
      snapshot = { ...snapshot, manifestHash }
    },
  }
}

const show = async (
  client: SnapshotClient,
  language: UiLanguage = "zh-TW",
  cardId = "c:one",
  testCatalog = catalog,
  printingId = "p:jp",
  region: "jp" | "en" = "jp",
) => {
  prefsStore.set({ uiLanguage: language })
  const i18n = await createI18n(language)
  const element = (currentCard: string) => (
    <I18nextProvider i18n={i18n}>
      <CardExternalLinks
        client={client}
        catalog={testCatalog}
        cardId={currentCard}
        printingId={printingId}
        region={region}
      />
    </I18nextProvider>
  )
  const view = render(element(cardId))
  return {
    ...view,
    showCard: (currentCard: string) => {
      view.rerender(element(currentCard))
    },
  }
}
const hrefs = () => screen.getAllByRole("link").map((anchor) => anchor.getAttribute("href"))

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("CardExternalLinks", () => {
  it("shows the selected region first, then the other official card page", async () => {
    const { client } = makeClient([linkRow("d:1")])
    await show(
      client,
      "en",
      "c:one",
      catalogWithPrintings([jpPrinting(), enPrinting()]),
      "p:en",
      "en",
    )
    await waitFor(() => {
      expect(screen.getAllByRole("link")).toHaveLength(3)
    })
    const links = screen.getAllByRole("link")
    expect(links.map((link) => link.getAttribute("href"))).toEqual([
      "https://en.shadowverse-evolve.com/cards/1/",
      "https://shadowverse-evolve.com/cards/1/",
      "https://portal.example/card/900211010?lang=en",
    ])
    expect(links.slice(0, 2).map((link) => link.textContent)).toEqual([
      "Official card page (English edition)(opens in a new tab)",
      "Official card page (Japanese edition)(opens in a new tab)",
    ])
    const firstOfficialLink = links.at(0)
    if (!firstOfficialLink) throw new Error("expected an official card page link")
    expect(firstOfficialLink).toHaveAttribute("target", "_blank")
    expect(firstOfficialLink.getAttribute("rel")).toBe("noopener noreferrer")
  })

  it("puts the Japanese region first when the selected printing is Japanese", async () => {
    const { client } = makeClient([])
    await show(client, "zh-TW", "c:one", catalogWithPrintings([enPrinting(), jpPrinting()]), "p:jp")
    expect(hrefs()).toEqual([
      "https://shadowverse-evolve.com/cards/1/",
      "https://en.shadowverse-evolve.com/cards/1/",
    ])
  })

  it.each([
    ["zh-TW", "官方卡片頁（日文版）"],
    ["ja", "公式カードページ（日本語版）"],
    ["en", "Official card page (Japanese edition)"],
  ] as const)("labels the official page in the %s interface", async (language, label) => {
    const { client } = makeClient([])
    await show(client, language, "c:one", catalogWithPrintings([jpPrinting()]))
    expect(screen.getByRole("link").textContent).toContain(label)
  })

  it("does not borrow another printing's official URL when the selected printing has none", async () => {
    const { client } = makeClient([])
    const sibling = { ...jpPrinting(), id: "p:jp-2", card_no: "BP01-002" }
    await show(
      client,
      "zh-TW",
      "c:one",
      catalogWithPrintings([{ ...jpPrinting(), faces: [] }, sibling]),
    )
    expect(screen.queryByRole("link")).toBeNull()
  })

  it("shows only the Japanese official page when only a Japanese printing exists", async () => {
    const { client } = makeClient([])
    await show(client, "zh-TW", "c:one", catalogWithPrintings([jpPrinting()]))
    expect(hrefs()).toEqual(["https://shadowverse-evolve.com/cards/1/"])
  })

  it("shows no official link when the printing has no source URL", async () => {
    const { client } = makeClient([])
    await show(client, "zh-TW", "c:one", catalogWithPrintings([{ ...jpPrinting(), faces: [] }]))
    expect(screen.queryByRole("link")).toBeNull()
  })

  it.each([
    "https://shadowverse-evolve.com.evil.example/cards/1/",
    "http://shadowverse-evolve.com/cards/1/",
    "https://unofficial.example/cards/1/",
  ])("ignores non-official source URLs: %s", async (url) => {
    const { client } = makeClient([])
    await show(client, "zh-TW", "c:one", catalogWithPrintings([jpPrinting(url)]))
    expect(screen.queryByRole("link")).toBeNull()
  })

  it("keeps the official page visible when digital links are disabled", async () => {
    prefsStore.set({ showDigitalLinks: false })
    const { client, fragments } = makeClient([])
    await show(client, "zh-TW", "c:one", catalogWithPrintings([jpPrinting()]))
    expect(hrefs()).toEqual(["https://shadowverse-evolve.com/cards/1/"])
    expect(fragments).not.toHaveBeenCalled()
  })

  it.each([
    ["zh-TW", ["lang=zh-tw", "/cht/"]],
    ["ja", ["lang=ja", "/ja/"]],
    ["en", ["lang=en", "/en/"]],
  ] as const)("builds the %s interface language's URLs for both games", async (language, parts) => {
    const { client } = makeClient([linkRow("d:1"), linkRow("d:3")])
    await show(client, language)
    await waitFor(() => {
      expect(screen.getAllByRole("link")).toHaveLength(2)
    })
    const [sv1, svwb] = hrefs()
    expect(sv1).toBe(`https://portal.example/card/900211010?${parts[0]}`)
    expect(svwb).toBe(`https://wb.example${parts[1]}card/?card_id=90021110`)
  })

  it("follows the interface language and ignores the card edition", async () => {
    prefsStore.set({ cardEdition: "en" })
    const { client } = makeClient([linkRow("d:1")])
    await show(client, "zh-TW")
    await waitFor(() => {
      expect(hrefs()).toEqual(["https://portal.example/card/900211010?lang=zh-tw"])
    })
    act(() => {
      prefsStore.set({ cardEdition: "jp" })
    })
    expect(hrefs()).toEqual(["https://portal.example/card/900211010?lang=zh-tw"])
    act(() => {
      prefsStore.set({ uiLanguage: "en" })
    })
    expect(hrefs()).toEqual(["https://portal.example/card/900211010?lang=en"])
  })

  it("numbers several digital cards of one game and labels each relation", async () => {
    const { client } = makeClient([
      linkRow("d:1"),
      linkRow("d:2", "same_character"),
      linkRow("d:3"),
    ])
    await show(client, "en")
    await waitFor(() => {
      expect(screen.getAllByRole("link")).toHaveLength(3)
    })
    expect(screen.getAllByRole("link").map((anchor) => anchor.textContent)).toEqual([
      "Shadowverse 1(opens in a new tab)",
      "Shadowverse 2(opens in a new tab)",
      "Worlds Beyond(opens in a new tab)",
    ])
    expect(screen.getAllByText("Same card")).toHaveLength(2)
    expect(screen.getByText("Same character")).toBeInTheDocument()
    expect(screen.queryByText(/review/i)).toBeNull()
  })

  it("opens links in a new tab without leaking the opener", async () => {
    const { client } = makeClient([linkRow("d:1")])
    await show(client)
    const anchor = await screen.findByRole("link")
    expect(anchor).toHaveAttribute("target", "_blank")
    expect(anchor.getAttribute("rel")).toContain("noopener")
  })

  it("shows a card without links as not checked yet, never as having no counterpart", async () => {
    const { client } = makeClient([])
    await show(client)
    expect(await screen.findByText("尚未核對")).toBeInTheDocument()
    expect(screen.queryByRole("link")).toBeNull()
  })

  it("shows a failed read with a retry, and the links once the retry succeeds", async () => {
    const { client } = makeClient([linkRow("d:1")], { count: 1 })
    await show(client)
    expect(await screen.findByText("數位版連結載入失敗")).toBeInTheDocument()
    expect(screen.queryByText("尚未核對")).toBeNull()
    await userEvent.click(screen.getByRole("button", { name: "重試" }))
    expect(await screen.findByRole("link")).toBeInTheDocument()
  })

  it("hides the whole section and reads nothing when the setting is off", async () => {
    prefsStore.set({ showDigitalLinks: false })
    const { client, fragments } = makeClient([linkRow("d:1")])
    await show(client)
    expect(screen.queryByText("外部連結")).toBeNull()
    expect(fragments).not.toHaveBeenCalled()
  })

  it("shows the section when the setting is on and brings it back after being turned on", async () => {
    prefsStore.set({ showDigitalLinks: false })
    const { client } = makeClient([linkRow("d:1")])
    await show(client)
    act(() => {
      prefsStore.set({ showDigitalLinks: true })
    })
    expect(await screen.findByText("外部連結")).toBeInTheDocument()
    expect(await screen.findByRole("link")).toBeInTheDocument()
  })

  it("keeps the new card's result when the previous card's read finishes late", async () => {
    const { client, fragments } = makeClient([linkRow("d:1")])
    const previous = Promise.withResolvers<Awaited<ReturnType<typeof fragments>>>()
    fragments.mockImplementationOnce(() => previous.promise)
    const view = await show(client)
    expect(screen.getByText("載入中…")).toBeInTheDocument()
    expect(screen.queryByText("尚未核對")).toBeNull()
    view.showCard("c:two")
    expect(await screen.findByText("尚未核對")).toBeInTheDocument()
    await act(async () => {
      previous.resolve([{ file: "links", table: "digital_link", rows: [linkRow("d:1")] }])
      await previous.promise
    })
    expect(screen.getByText("尚未核對")).toBeInTheDocument()
    expect(screen.queryByRole("link")).toBeNull()
  })

  it("replaces visible links and ignores late reads from a replaced snapshot", async () => {
    const links = [linkRow("d:1")]
    const { client, fragments, replaceSnapshot } = makeClient(links)
    const view = await show(client)
    expect(await screen.findByRole("link")).toHaveAttribute(
      "href",
      "https://portal.example/card/900211010?lang=zh-tw",
    )

    const previous = Promise.withResolvers<Awaited<ReturnType<typeof fragments>>>()
    fragments.mockImplementationOnce(() => previous.promise)
    replaceSnapshot("h2")
    view.showCard("c:one")
    expect(screen.queryByRole("link")).toBeNull()
    expect(screen.getByText("載入中…")).toBeInTheDocument()

    replaceSnapshot("h3")
    links.splice(0, links.length, linkRow("d:3"))
    view.showCard("c:one")
    expect(await screen.findByRole("link")).toHaveAttribute(
      "href",
      "https://wb.example/cht/card/?card_id=90021110",
    )
    await act(async () => {
      previous.resolve([{ file: "links", table: "digital_link", rows: [linkRow("d:1")] }])
      await previous.promise
    })
    expect(hrefs()).toEqual(["https://wb.example/cht/card/?card_id=90021110"])
    expect(screen.queryByText("數位版連結載入失敗")).toBeNull()
  })

  it("restarts loading after being hidden and ignores the hidden read's late failure", async () => {
    const { client, fragments } = makeClient([linkRow("d:1")])
    const previous = Promise.withResolvers<Awaited<ReturnType<typeof fragments>>>()
    fragments.mockImplementationOnce(() => previous.promise)
    await show(client)
    expect(screen.getByText("載入中…")).toBeInTheDocument()
    act(() => {
      prefsStore.set({ showDigitalLinks: false })
    })
    expect(screen.queryByRole("region", { name: "外部連結" })).toBeNull()
    act(() => {
      prefsStore.set({ showDigitalLinks: true })
    })
    expect(await screen.findByRole("link")).toBeInTheDocument()
    await act(async () => {
      previous.reject(new Error("late network failure"))
      await expect(previous.promise).rejects.toThrow("late network failure")
    })
    expect(screen.getByRole("link")).toBeInTheDocument()
    expect(screen.queryByText("數位版連結載入失敗")).toBeNull()
  })
})
