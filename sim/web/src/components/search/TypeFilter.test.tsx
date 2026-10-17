import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, it } from "vitest"

import { formatQuery, parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { renderInRouter } from "../../test-utils"
import { TypeFilter } from "./TypeFilter"

function Harness() {
  const [selected, onChange] = useState(parseQuery(new URLSearchParams("type=leader:white")).types)
  return (
    <>
      <TypeFilter
        textLang="zh-Hant"
        selected={selected}
        onChange={onChange}
        options={{
          types: [
            {
              id: "type:leader",
              code: "leader",
              name: {
                original: { lang: "ja", text: "リーダー" },
                translations: { "zh-Hant": "主戰者" },
              },
            },
          ],
          typeCounts: { leader: 3, "leader:gold": 2, "leader:white": 1 },
        }}
      />
      <output aria-label="url">
        {formatQuery({ ...DEFAULT_QUERY, types: selected }).toString()}
      </output>
    </>
  )
}

it("restores frame URLs and supports mixed, parent and individual selections", async () => {
  await renderInRouter(<Harness />)
  const parent = screen.getByRole("button", { name: "主戰者" })
  expect(parent).toHaveAttribute("aria-pressed", "mixed")
  await userEvent.click(screen.getByRole("button", { name: "主戰者的細項" }))
  const group = screen.getByRole("group", { name: "主戰者" })
  expect(within(group).getByRole("checkbox", { name: "白框" })).toBeChecked()
  expect(within(group).getByRole("checkbox", { name: "金框" })).toHaveAccessibleDescription("2")
  await userEvent.click(parent)
  expect(parent).toHaveAttribute("aria-pressed", "true")
  expect(screen.getByLabelText("url")).toHaveTextContent(/^type=leader$/u)
  await userEvent.click(within(group).getByRole("checkbox", { name: "白框" }))
  expect(parent).toHaveAttribute("aria-pressed", "mixed")
  expect(screen.getByLabelText("url")).toHaveTextContent(/^type=leader%3Agold$/u)
  const restored = parseQuery(new URLSearchParams(screen.getByLabelText("url").textContent))
  expect(restored.types).toEqual(["leader:gold"])
  await userEvent.click(within(group).getByRole("checkbox", { name: "金框" }))
  expect(parent).toHaveAttribute("aria-pressed", "false")
  expect(screen.getByLabelText("url")).toBeEmptyDOMElement()
})

function GroupHarness() {
  const [selected, onChange] = useState(parseQuery(new URLSearchParams("type=token:spell")).types)
  const labels = [
    ["equipment", "裝備"],
    ["sep", "SEP"],
    ["leader", "主戰者"],
    ["amulet", "護符"],
    ["crest", "紋章"],
    ["ep", "EP"],
    ["token", "衍生物"],
    ["spell", "法術"],
    ["follower", "從者"],
  ] as const
  return (
    <>
      <TypeFilter
        textLang="zh-Hant"
        selected={selected}
        onChange={onChange}
        options={{
          types: labels.map(([code, label]) => ({
            id: code,
            code,
            name: { original: { lang: "ja", text: code }, translations: { "zh-Hant": label } },
          })),
          typeCounts: {
            follower: 3,
            spell: 2,
            amulet: 1,
            token: 7,
            "token:follower": 1,
            "token:spell": 2,
            "token:amulet": 1,
            "token:crest": 1,
            "token:equipment": 2,
            crest: 1,
            leader: 2,
            "leader:gold": 1,
            "leader:white": 1,
            ep: 1,
            sep: 1,
            equipment: 1,
          },
        }}
      />
      <output aria-label="url">
        {formatQuery({ ...DEFAULT_QUERY, types: selected }).toString()}
      </output>
    </>
  )
}

it("orders the chips, names the token chip Token and keeps its kinds behind the popover", async () => {
  await renderInRouter(<GroupHarness />)
  const row = screen.getByRole("group", { name: "類型" })
  expect(
    within(row)
      .getAllByRole("button")
      .map((button) => button.getAttribute("aria-label")),
  ).toEqual(["從者", "法術", "護符", "Token", "Token的細項", "主戰者", "主戰者的細項", "EP", "SEP"])
  expect(screen.queryByText("衍生物")).not.toBeInTheDocument()
  const parent = within(row).getByRole("button", { name: "Token" })
  expect(parent).toHaveAttribute("aria-pressed", "mixed")
  await userEvent.click(within(row).getByRole("button", { name: "Token的細項" }))
  const kinds = screen.getByRole("group", { name: "Token" })
  expect(
    within(kinds)
      .getAllByRole("checkbox")
      .map((checkbox) => checkbox.getAttribute("aria-label")),
  ).toEqual(["從者", "法術", "護符", "紋章", "裝備"])
  expect(within(kinds).getByRole("checkbox", { name: "法術" })).toBeChecked()
  await userEvent.click(parent)
  expect(parent).toHaveAttribute("aria-pressed", "true")
  expect(screen.getByLabelText("url")).toHaveTextContent(/^type=token$/u)
  await userEvent.click(within(kinds).getByRole("checkbox", { name: "護符" }))
  expect(parent).toHaveAttribute("aria-pressed", "mixed")
  await userEvent.click(parent)
  // A click on another chip is outside the popover and dismisses it.
  await userEvent.click(within(row).getByRole("button", { name: "從者" }))
  expect(kinds).not.toBeVisible()
  expect(parseQuery(new URLSearchParams(screen.getByLabelText("url").textContent)).types).toEqual([
    "follower",
    "token",
  ])
  await userEvent.click(parent)
  await userEvent.click(within(row).getByRole("button", { name: "Token的細項" }))
  await userEvent.click(within(kinds).getByRole("checkbox", { name: "法術" }))
  expect(parseQuery(new URLSearchParams(screen.getByLabelText("url").textContent)).types).toEqual([
    "follower",
    "token:spell",
  ])
})
