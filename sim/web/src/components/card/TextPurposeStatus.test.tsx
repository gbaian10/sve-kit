import { screen, within } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import fixture from "../../../../../tests/fixtures/snapshot-contract/v3/whole-name-native.json"
import type { AnnotatedTextState } from "../../app/snapshot"
import { sharedAnnotatedTextResolver } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { CardNameHeading } from "./CardNameHeading"
import { CardText } from "./CardText"

const boundary = vi.hoisted(() => ({ state: null as AnnotatedTextState | null }))
vi.mock("../../app/snapshot", () => ({
  useAnnotatedText: () => boundary.state,
  useAnnotatedTexts: () => [],
}))
const owner = { kind: "face_revision", id: "revision" }
const { client } = await annotatedOrigin(fixture)
await client.load()
const value = await sharedAnnotatedTextResolver(client).resolve(
  { owner, field: "name", ordinal: null },
  "zh-Hant",
)
const readiness: AnnotatedTextState["readiness"] = {
  root: "cdn",
  generation: 1,
  owner,
  ordinal: null,
  edition: "jp",
  field: "name",
  language: "zh-Hant",
  basic: "ready",
  source: "pending",
  annotations: "pending",
}

beforeEach(() => {
  prefsStore.set({ uiLanguage: "zh-TW", nameDisplay: "translated" })
  boundary.state = { readiness, value }
})
afterEach(() => prefsStore.set(DEFAULT_PREFS))

describe("partial text consumers", () => {
  it.each(["heading", "text"] as const)(
    "keeps basic text usable without exposing unverified references in %s",
    async (consumer) => {
      await renderInRouter(
        consumer === "heading" ? (
          <CardNameHeading
            client={client}
            resolver={null}
            owner={owner}
            edition="jp"
            fallback={{ primary: { lang: "ja", text: "Fallback" }, missingTranslation: false }}
          />
        ) : (
          <CardText client={client} owner={owner} edition="jp" field="name" />
        ),
      )
      // Unverified annotations stay inert: the keyword concept is not emphasised yet.
      expect(screen.getByText("Synthetic translation")).toBeVisible()
      expect(screen.getByText("Synthetic translation").tagName).not.toBe("STRONG")
      expect(screen.getByText("日文依據載入中")).toHaveAttribute("role", "status")
      expect(screen.getByText("標註載入中")).toHaveAttribute("role", "status")
    },
  )

  it("shows closure errors explicitly and keeps unverified annotations inert", async () => {
    boundary.state = {
      value,
      readiness: {
        ...readiness,
        source: "error",
        annotations: "error",
        error: { kind: "validation", code: "public-annotation/reference", message: "synthetic" },
      },
    }
    await renderInRouter(<CardText client={client} owner={owner} edition="jp" field="name" />)
    expect(screen.getAllByRole("alert")).toHaveLength(2)
    for (const alert of screen.getAllByRole("alert"))
      expect(alert).toHaveTextContent("文字資料無法通過驗證")
    expect(screen.getByText("Synthetic translation").tagName).not.toBe("STRONG")
  })

  it("restores the explanation entry after annotation readiness", async () => {
    boundary.state = { value, readiness: { ...readiness, source: "none", annotations: "ready" } }
    await renderInRouter(<CardText client={client} owner={owner} edition="jp" field="name" />)
    expect(screen.getByText("Synthetic translation").tagName).toBe("STRONG")
    expect(screen.queryByText("標註載入中")).not.toBeInTheDocument()
  })

  it("does not hide a basic name validation failure behind the catalog fallback", async () => {
    boundary.state = {
      value: null,
      readiness: {
        ...readiness,
        basic: "error",
        error: { kind: "validation", message: "synthetic" },
      },
    }
    await renderInRouter(
      <CardNameHeading
        client={client}
        resolver={null}
        owner={owner}
        edition="jp"
        fallback={{ primary: { lang: "ja", text: "Fallback" }, missingTranslation: false }}
      />,
    )
    expect(within(screen.getByRole("heading", { level: 2 })).getByText("Fallback")).toBeVisible()
    expect(screen.getByRole("alert")).toHaveTextContent("文字資料無法通過驗證")
  })
})
