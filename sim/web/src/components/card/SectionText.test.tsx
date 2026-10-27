import { act, screen, waitFor, within } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"

import english from "../../../../../tests/fixtures/snapshot-contract/v3/source-native.json"
import { objectValue, type Row, sharedAnnotatedTextResolver } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { CardText } from "./CardText"

afterEach(() => prefsStore.set(DEFAULT_PREFS))

const owner = { kind: "face_revision", id: "revision" }
const SOURCE_UNIT = "t:ja:6a34655cc94a4524"
const LABEL_UNIT = "t:ja:b203904f7511cdf8"

// The synthetic revision has one section and a name translation (reused here for the effect);
// the section translation is shipped as its own row like a real snapshot.
async function sectionOrigin(options: {
  effect?: boolean
  blankEffect?: boolean
  section?: "same" | "low" | "none"
  label?: boolean
}) {
  const { client } = await annotatedOrigin()
  await client.load()
  const selection = (field: string, ordinal: number | null, translationId: string) => ({
    field,
    ordinal,
    target_lang: "zh-Hant",
    translation_id: translationId,
    basis: "own_source",
    source: { owner, field, ordinal },
    counterpart: null,
  })
  return {
    ...client,
    fragments: async (key: string) => {
      const fragments = (await client.fragments(key)).map((fragment) => ({
        ...fragment,
        rows: fragment.rows.map((row) =>
          fragment.table === "face_revision" && row["id"] === "revision"
            ? {
                ...row,
                ...(options.blankEffect && typeof row["effect_unit_id"] === "string"
                  ? { effect_unit_id: "t:ja:blank" }
                  : {}),
                ...(Array.isArray(row["sections"]) && options.label
                  ? {
                      sections: (row["sections"] as Row[]).map((section) => ({
                        ...section,
                        label_unit_id: LABEL_UNIT,
                      })),
                    }
                  : {}),
                translations: [
                  ...(row["translations"] as Row[]),
                  ...(options.effect ? [selection("effect", null, "translation")] : []),
                  ...(options.section && options.section !== "none"
                    ? [selection("section", 0, "translation-section")]
                    : []),
                ],
              }
            : row,
        ),
      }))
      const detail = fragments.find((fragment) => fragment.table === "face_revision")
      if (detail && options.blankEffect)
        fragments.push({
          ...detail,
          table: "text_unit",
          rows: [{ id: "t:ja:blank", lang: "ja", text: "" }],
        })
      if (detail && options.section && options.section !== "none")
        fragments.push(
          {
            ...detail,
            table: "text_unit",
            rows: [{ id: "t:zh-Hant:section", lang: "zh-Hant", text: "合成段落譯文" }],
          },
          {
            ...detail,
            table: "translation",
            rows: [
              {
                id: "translation-section",
                source_unit_id: SOURCE_UNIT,
                target_lang: "zh-Hant",
                text_unit_id: "t:zh-Hant:section",
                origin: options.section === "low" ? "machine" : "project",
                authority: "unofficial",
                low_confidence: options.section === "low",
                annotation_set_id: null,
                annotation_kind: "none",
              },
            ],
          },
        )
      return fragments
    },
  }
}

// Paragraphs outside the framed originals, or inside them when `original` is set.
const paragraphs = (root: HTMLElement, lang: string, original = false) =>
  [...root.querySelectorAll(`p[lang="${lang}"]`)]
    .filter((p) => (p.closest("[data-original]") !== null) === original)
    .map((p) => p.textContent)
const originals = (root: HTMLElement) => [...root.querySelectorAll<HTMLElement>("[data-original]")]
const effectRegion = () => screen.getByRole("region", { name: "效果" })
// Sections resolve a frame after the effect and may replace the region's subtree, so wait on a
// fresh lookup until nothing in it is still loading and the section has appeared.
const settledRegion = async (name: string) => {
  await screen.findByRole("region", { name })
  await waitFor(() => {
    const region = screen.getByRole("region", { name })
    expect(region.querySelector("[role=status]")).toBeNull()
    expect(region.querySelector("[data-section]")).not.toBeNull()
  })
  return screen.getByRole("region", { name })
}

describe("sections under the effect in the Traditional Chinese interface", () => {
  it("prints the labelled section after a rule in both boxes with one shared source line", async () => {
    const client = await sectionOrigin({ effect: true, section: "same", label: true })
    const section = await sharedAnnotatedTextResolver(client).resolve(
      { owner, field: "section", ordinal: 0 },
      "zh-Hant",
    )
    expect(section?.selection?.["basis"]).toBe("own_source")
    expect(section?.translated?.unit["text"]).toBe("合成段落譯文")
    await renderInRouter(<CardText edition="jp" client={client} owner={owner} field="effect" />)
    const region = await settledRegion("效果")
    const translated = region
    expect(paragraphs(translated, "zh-Hant")).toEqual(["Synthetic translation", "合成段落譯文"])
    expect(paragraphs(translated, "ja")).toEqual([])
    const rule = translated.querySelector("[data-section]")
    expect(rule).toHaveClass("border-t")
    expect(within(translated).getByText("Synthetic keyword")).toBeVisible()
    expect(within(translated).getAllByText(/譯文 · /)).toHaveLength(1)
    const line = within(translated).getByText(/專案譯文/)
    expect(line).toHaveTextContent("專案譯文 · 非官方")
    // The shared source line sits under the heading, before the effect and its sections.
    const first = translated.querySelector('p[lang="zh-Hant"]')
    if (!first) throw new Error("translation missing")
    expect(line.compareDocumentPosition(first)).toBe(Node.DOCUMENT_POSITION_FOLLOWING)
    if (!rule) throw new Error("section missing")
    expect(line.compareDocumentPosition(rule)).toBe(Node.DOCUMENT_POSITION_FOLLOWING)
    expect(rule).not.toContainElement(line)
    // Each unit has its own original box: the effect's, then the section's inside the section.
    expect(paragraphs(region, "ja", true)).toEqual(["Synthetic text", "Synthetic text"])
    expect(originals(region)).toHaveLength(2)
    expect(rule).toContainElement(originals(region)[1] ?? null)
    // The label is the section's, printed once above its translation, not repeated in the box.
    expect(within(originals(region)[1] as HTMLElement).queryByText("Synthetic keyword")).toBeNull()
    expect(screen.queryByText("附註")).not.toBeInTheDocument()
    expect(translated.querySelector("strong")).toHaveTextContent("Synthetic")
    // The language preference only changes what is shown, never which revision is resolved.
    act(() => {
      prefsStore.set({ effectLanguage: "original" })
    })
    expect(paragraphs(effectRegion(), "ja")).toEqual(["Synthetic text", "Synthetic text"])
    expect(paragraphs(effectRegion(), "zh-Hant")).toEqual([])
    act(() => {
      prefsStore.set({ effectLanguage: "translated" })
    })
    expect(paragraphs(effectRegion(), "zh-Hant")).toEqual(["Synthetic translation", "合成段落譯文"])
    expect(paragraphs(effectRegion(), "ja")).toEqual([])
  })

  it("gives a low-confidence section its own source line under the section", async () => {
    const client = await sectionOrigin({ effect: true, section: "low" })
    await renderInRouter(<CardText edition="jp" client={client} owner={owner} field="effect" />)
    const region = await settledRegion("效果")
    const translated = region
    expect(paragraphs(translated, "zh-Hant")).toEqual(["Synthetic translation", "合成段落譯文"])
    const rule = translated.querySelector("[data-section]")
    if (!(rule instanceof HTMLElement)) throw new Error("section missing")
    expect(within(rule).getByText(/機器譯文/)).toHaveTextContent("機器譯文 · 非官方")
    expect(within(rule).getByText("待校對")).toBeVisible()
    const effectLine = within(translated).getByText(/專案譯文/)
    expect(rule).not.toContainElement(effectLine)
    expect(effectLine.compareDocumentPosition(rule)).toBe(Node.DOCUMENT_POSITION_FOLLOWING)
    expect(within(translated).getAllByText("待校對")).toHaveLength(1)
  })

  it("falls back to the section's original with a missing note when only the effect is translated", async () => {
    const client = await sectionOrigin({ effect: true, section: "none" })
    await renderInRouter(<CardText edition="jp" client={client} owner={owner} field="effect" />)
    const region = await settledRegion("效果")
    const translated = region
    expect(paragraphs(translated, "zh-Hant")).toEqual(["Synthetic translation"])
    expect(paragraphs(translated, "ja")).toEqual(["Synthetic text"])
    const rule = translated.querySelector("[data-section]")
    if (!(rule instanceof HTMLElement)) throw new Error("section missing")
    expect(within(rule).getByText("缺少此語言譯文")).toBeVisible()
    expect(within(translated).getAllByText("缺少此語言譯文")).toHaveLength(1)
    expect(paragraphs(region, "ja", true)).toEqual(["Synthetic text"])
    expect(originals(region)).toHaveLength(1)
  })

  it("keeps an English card's own section in English instead of borrowing the Japanese source section", async () => {
    const receiver = objectValue(english.receiver.owner)
    const { client } = await annotatedOrigin(english, "en")
    await client.load()
    const patched = {
      ...client,
      fragments: async (key: string) =>
        (await client.fragments(key)).map((fragment) => ({
          ...fragment,
          rows: fragment.rows.map((row) =>
            fragment.table === "face_revision" &&
            row["id"] === receiver["id"] &&
            Array.isArray(row["sections"])
              ? {
                  ...row,
                  sections: [
                    { ordinal: 0, text_unit_id: row["effect_unit_id"] ?? null, kind: "unknown" },
                  ],
                }
              : row,
          ),
        })),
    }
    const value = await sharedAnnotatedTextResolver(patched).resolve(
      { owner: receiver, field: "section", ordinal: 0 },
      "zh-Hant",
    )
    expect(value?.original.unit["lang"]).toBe("en")
    expect(value?.translated).toBeUndefined()
    expect(value?.source).toBeUndefined()
    expect(value?.sourceSections).toBeUndefined()
    await renderInRouter(<CardText edition="en" client={patched} owner={receiver} field="effect" />)
    const region = await settledRegion("效果")
    const translated = region
    expect(paragraphs(translated, "zh-Hant")).toEqual(["Synthetic translated source effect"])
    const rule = translated.querySelector("[data-section]")
    if (!(rule instanceof HTMLElement)) throw new Error("section missing")
    expect(paragraphs(rule, "en")).toEqual(["Synthetic English counterpart effect"])
    expect(within(rule).getByText("缺少此語言譯文")).toBeVisible()
    expect(paragraphs(region, "en", true)).toEqual(["Synthetic English counterpart effect"])
    expect(screen.queryByText("Synthetic Japanese source paragraph")).not.toBeInTheDocument()
  })
})

describe("a translated section under an untranslated effect", () => {
  it.each([
    ["both", true],
    ["translated", false],
  ] as const)(
    "keeps the section's translation and quality in the %s preference",
    async (effectLanguage, withOriginalBox) => {
      const client = await sectionOrigin({ section: "low" })
      prefsStore.set({ effectLanguage })
      await renderInRouter(<CardText edition="jp" client={client} owner={owner} field="effect" />)
      const region = await settledRegion("效果")
      const translated = region
      expect(paragraphs(translated, "ja")).toEqual(["Synthetic text"])
      expect(paragraphs(translated, "zh-Hant")).toEqual(["合成段落譯文"])
      const rule = translated.querySelector("[data-section]")
      if (!(rule instanceof HTMLElement)) throw new Error("section missing")
      const missing = within(translated).getByText("缺少此語言譯文")
      expect(rule).not.toContainElement(missing)
      expect(missing.compareDocumentPosition(rule)).toBe(Node.DOCUMENT_POSITION_FOLLOWING)
      expect(within(rule).getByText(/機器譯文/)).toHaveTextContent("機器譯文 · 非官方")
      expect(within(rule).getByText("待校對")).toBeVisible()
      expect(within(region).getAllByText(/譯文 · /)).toHaveLength(1)
      expect(within(region).queryByText(/專案譯文/)).not.toBeInTheDocument()
      if (withOriginalBox) {
        // Only the translated section gets a box; the untranslated effect is its own original.
        expect(originals(region)).toHaveLength(1)
        expect(rule).toContainElement(originals(region)[0] ?? null)
        expect(paragraphs(region, "ja", true)).toEqual(["Synthetic text"])
        expect(paragraphs(region, "zh-Hant", true)).toEqual([])
      } else {
        expect(within(region).queryByRole("button")).not.toBeInTheDocument()
        expect(paragraphs(region, "ja")).toEqual(["Synthetic text"])
      }
    },
  )

  it("shows only the originals in the original preference", async () => {
    const client = await sectionOrigin({ section: "low" })
    prefsStore.set({ effectLanguage: "original" })
    await renderInRouter(<CardText edition="jp" client={client} owner={owner} field="effect" />)
    const region = await settledRegion("效果")
    expect(paragraphs(region, "ja")).toEqual(["Synthetic text", "Synthetic text"])
    expect(paragraphs(region, "zh-Hant")).toEqual([])
    expect(within(region).queryByText("缺少此語言譯文")).not.toBeInTheDocument()
    expect(within(region).queryByText("待校對")).not.toBeInTheDocument()
    expect(within(region).queryByText(/譯文 · /)).not.toBeInTheDocument()
  })

  it.each(["both", "translated"] as const)(
    "reports the missing section translation under a blank effect in the %s preference",
    async (effectLanguage) => {
      const client = await sectionOrigin({ blankEffect: true, section: "none" })
      prefsStore.set({ effectLanguage })
      await renderInRouter(<CardText edition="jp" client={client} owner={owner} field="effect" />)
      const region = await settledRegion("效果")
      expect(screen.queryByText("這張卡沒有效果文字")).not.toBeInTheDocument()
      expect(within(region).queryByRole("button")).not.toBeInTheDocument()
      expect(paragraphs(region, "ja")).toEqual(["Synthetic text"])
      expect(paragraphs(region, "zh-Hant")).toEqual([])
      const rule = region.querySelector("[data-section]")
      if (!(rule instanceof HTMLElement)) throw new Error("section missing")
      expect(rule).not.toHaveClass("border-t")
      // Nothing in the box is translated: one note at the end instead of one per section.
      const note = within(region).getByText("缺少此語言譯文")
      expect(rule).not.toContainElement(note)
      expect(rule.compareDocumentPosition(note)).toBe(Node.DOCUMENT_POSITION_FOLLOWING)
      act(() => {
        prefsStore.set({ effectLanguage: "original" })
      })
      expect(paragraphs(effectRegion(), "ja")).toEqual(["Synthetic text"])
      expect(within(effectRegion()).queryByText("缺少此語言譯文")).not.toBeInTheDocument()
    },
  )

  it("prints only the sections when the effect itself is blank", async () => {
    const client = await sectionOrigin({ blankEffect: true, section: "same" })
    await renderInRouter(<CardText edition="jp" client={client} owner={owner} field="effect" />)
    const region = await settledRegion("效果")
    expect(screen.queryByText("這張卡沒有效果文字")).not.toBeInTheDocument()
    const translated = region
    expect(paragraphs(translated, "zh-Hant")).toEqual(["合成段落譯文"])
    expect(paragraphs(translated, "ja")).toEqual([])
    expect(translated.querySelector("[data-section]")).not.toHaveClass("border-t")
    expect(within(translated).getByText(/專案譯文/)).toHaveTextContent("專案譯文 · 非官方")
    expect(within(translated).queryByText("缺少此語言譯文")).not.toBeInTheDocument()
    expect(paragraphs(region, "ja", true)).toEqual(["Synthetic text"])
    expect(originals(region)).toHaveLength(1)
  })
})

describe("sections in the other interfaces", () => {
  it.each([
    ["ja", "効果", null],
    ["en", "Effect", "Translation missing in this language"] as const,
  ] as const)(
    "shows the original effect and section only in %s even when translations exist",
    async (language, heading, missing) => {
      const client = await sectionOrigin({ effect: true, section: "same" })
      await renderInRouter(<CardText edition="jp" client={client} owner={owner} field="effect" />, {
        language,
      })
      const region = await settledRegion(heading)
      expect(paragraphs(region, "ja")).toEqual(["Synthetic text", "Synthetic text"])
      expect(paragraphs(region, "zh-Hant")).toEqual([])
      expect(region.querySelector("[data-section]")).toHaveClass("border-t")
      expect(within(region).queryByRole("button")).not.toBeInTheDocument()
      if (missing === null)
        expect(within(region).queryByText(/訳はありません/)).not.toBeInTheDocument()
      else {
        // Nothing in the box is translated, so the box says so once, after the section.
        const note = within(region).getByText(missing)
        const rule = region.querySelector("[data-section]")
        expect(rule).not.toContainElement(note)
        expect(rule?.compareDocumentPosition(note)).toBe(Node.DOCUMENT_POSITION_FOLLOWING)
      }
    },
  )
})
