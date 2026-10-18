import { screen, within } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"

import { createAnnotatedTextResolver } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import {
  invalidLanguageCases,
  invalidLanguageSnapshot,
  languageCases,
  languageSnapshot,
  languageTexts,
} from "../../test-utils/language-matrix"
import { CardText } from "./CardText"
import { textLanguage } from "./text-language"

afterEach(() => prefsStore.set(DEFAULT_PREFS))

const paragraphText = (text: string) => (_content: string, node: Element | null) =>
  node?.tagName === "P" && node.textContent === text

describe("original plus available translation in every language matrix cell", () => {
  it.each(languageCases)("$id", async (row) => {
    const { client } = await annotatedOrigin(languageSnapshot, row.region)
    await client.load()
    expect(client.status().state).toBe("ready")
    const resolver = createAnnotatedTextResolver(client)
    prefsStore.set({ effectLanguage: "both" })
    const { i18n } = await renderRoutes(
      [
        {
          path: "*",
          element: (
            <CardText
              client={client}
              edition={row.region}
              owner={row.receiver.owner}
              field="effect"
              sharedResolver={resolver}
            />
          ),
        },
      ],
      { language: row.ui_language },
    )
    expect(textLanguage(i18n.resolvedLanguage)).toBe(row.text_language)
    await screen.findByText(paragraphText(row.expected.translated ?? row.expected.original))
    const selected = resolver.peek(row.receiver, row.text_language)
    expect(selected?.selection?.["basis"] ?? null).toBe(row.expected.basis)
    expect(selected?.translation?.["low_confidence"] ?? false).toBe(row.expected.low_confidence)
    const effect = screen.getByRole("region", { name: i18n.t("card.field.effect") })
    const textRows = [...effect.querySelectorAll("p[lang]")]
    for (const text of row.expected.displayed) {
      const paragraph = within(effect).getByText(paragraphText(text))
      expect(paragraph).toBeVisible()
      expect(paragraph.textContent).toBe(text)
    }
    for (const paragraph of textRows)
      if (!row.expected.displayed.includes(paragraph.textContent))
        expect(paragraph).not.toBeVisible()
    expect(textRows.filter((p) => row.expected.displayed.includes(p.textContent))).toHaveLength(
      row.expected.displayed.length,
    )
    const missing = within(effect).queryByText(i18n.t("card.translationMissing"))
    if (row.expected.missing) expect(missing).toBeVisible()
    else expect(missing).not.toBeInTheDocument()
    const proofreading = within(effect).queryByText(i18n.t("card.translationProofreading"))
    if (row.expected.proofreading) expect(proofreading).toBeVisible()
    else expect(proofreading).not.toBeInTheDocument()
    if (row.expected.origin !== null && row.expected.authority !== null) {
      const label = `${i18n.t(`card.translationOrigin.${row.expected.origin}`)} · ${i18n.t(`card.translationAuthority.${row.expected.authority}`)}`
      expect(within(effect).getByText(label)).toBeVisible()
    } else {
      for (const origin of ["official", "project", "machine"] as const)
        expect(
          within(effect).queryByText(i18n.t(`card.translationOrigin.${origin}`), { exact: false }),
        ).not.toBeInTheDocument()
    }
    if (row.text_language !== "zh-Hant")
      expect(within(effect).queryByText(paragraphText(languageTexts.Z))).not.toBeInTheDocument()
    const sourceLabel = within(effect).queryByText(i18n.t("card.jpSource"))
    if (row.expected.basis === "jp_source") expect(sourceLabel).toBeVisible()
    else expect(sourceLabel).not.toBeInTheDocument()
  })

  it.each(invalidLanguageCases)("does not display $id", async (row) => {
    const { client } = await annotatedOrigin(await invalidLanguageSnapshot(row), row.region)
    await client.load()
    expect(client.status().state).toBe("ready")
    prefsStore.set({ effectLanguage: "both" })
    await renderRoutes(
      [
        {
          path: "*",
          element: (
            <CardText
              client={client}
              edition={row.region}
              owner={row.receiver.owner}
              field="effect"
            />
          ),
        },
      ],
      { language: row.text_language === "en" ? "en" : "ja" },
    )
    expect(await screen.findByRole("alert")).toBeVisible()
    expect(screen.queryByText(paragraphText(languageTexts.Ec))).not.toBeInTheDocument()
    expect(screen.queryByText(paragraphText(languageTexts.Jc))).not.toBeInTheDocument()
  })
})
