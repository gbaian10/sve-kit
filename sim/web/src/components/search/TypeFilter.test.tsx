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
  await userEvent.click(screen.getByRole("button", { name: /類型/u }))
  const group = screen.getByRole("group", { name: "類型" })
  const parent = within(group).getByRole("checkbox", { name: "主戰者" })
  expect(parent).toBePartiallyChecked()
  expect(within(group).getByRole("checkbox", { name: "白框" })).toBeChecked()
  expect(within(group).getByRole("checkbox", { name: "金框" })).toHaveAccessibleDescription("2")
  await userEvent.click(parent)
  expect(parent).toBeChecked()
  expect(screen.getByLabelText("url")).toHaveTextContent(/^type=leader$/u)
  await userEvent.click(within(group).getByRole("checkbox", { name: "白框" }))
  expect(parent).toBePartiallyChecked()
  expect(screen.getByLabelText("url")).toHaveTextContent(/^type=leader%3Agold$/u)
  const restored = parseQuery(new URLSearchParams(screen.getByLabelText("url").textContent))
  expect(restored.types).toEqual(["leader:gold"])
  await userEvent.click(within(group).getByRole("checkbox", { name: "金框" }))
  expect(parent).not.toBeChecked()
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
    ["token", "Token（詞表）"],
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

it("groups Token children, supports both parent trees and restores union URLs", async () => {
  await renderInRouter(<GroupHarness />)
  await userEvent.click(screen.getByRole("button", { name: /類型/u }))
  const group = screen.getByRole("group", { name: "類型" })
  const lists = within(group)
    .getAllByRole("list")
    .filter((list) => !list.parentElement?.closest("ul"))
  expect(
    lists.map((list) =>
      within(list)
        .getAllByRole("checkbox")
        .map((checkbox) => checkbox.getAttribute("aria-label")),
    ),
  ).toEqual([
    ["從者", "法術", "護符", "Token（詞表）", "從者", "法術", "護符", "紋章", "裝備"],
    ["主戰者", "金框", "白框"],
    ["EP", "SEP"],
  ])
  const parent = within(group).getByRole("checkbox", { name: "Token（詞表）" })
  const children = within(group).getByRole("list", { name: "Token（詞表）" })
  expect(parent).toBePartiallyChecked()
  expect(within(children).getByRole("checkbox", { name: "法術" })).toBeChecked()
  await userEvent.click(parent)
  expect(parent).toBeChecked()
  expect(screen.getByLabelText("url")).toHaveTextContent(/^type=token$/u)
  await userEvent.click(within(children).getByRole("checkbox", { name: "護符" }))
  expect(parent).toBePartiallyChecked()
  await userEvent.click(parent)
  const follower = within(group).getAllByRole("checkbox", { name: "從者" })[0]
  if (!follower) throw new Error("missing follower")
  await userEvent.click(follower)
  expect(parseQuery(new URLSearchParams(screen.getByLabelText("url").textContent)).types).toEqual([
    "follower",
    "token",
  ])
  await userEvent.click(parent)
  await userEvent.click(within(children).getByRole("checkbox", { name: "法術" }))
  expect(parseQuery(new URLSearchParams(screen.getByLabelText("url").textContent)).types).toEqual([
    "follower",
    "token:spell",
  ])
})
