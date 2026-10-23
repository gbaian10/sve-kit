import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, it } from "vitest"

import { formatQuery, parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { renderInRouter } from "../../test-utils"
import { LocationFilter } from "./LocationFilter"

const TYPES = [
  ["follower", "從者"],
  ["spell", "法術"],
  ["amulet", "護符"],
  ["crest", "紋章"],
  ["equipment", "裝備"],
  ["leader", "主戰者"],
  ["ep", "EP"],
  ["sep", "SEP"],
] as const

function Harness({ initial, counts }: { initial: string; counts: Record<string, number> }) {
  const [selected, onChange] = useState(parseQuery(new URLSearchParams(initial)).locations)
  return (
    <>
      <LocationFilter
        textLang="zh-Hant"
        selected={selected}
        onChange={onChange}
        options={{
          types: TYPES.map(([code, label]) => ({
            id: code,
            code,
            name: { original: { lang: "ja", text: code }, translations: { "zh-Hant": label } },
          })),
          counts,
        }}
      />
      <output aria-label="url">
        {formatQuery({ ...DEFAULT_QUERY, locations: selected }).toString()}
      </output>
    </>
  )
}

const url = () => decodeURIComponent(screen.getByLabelText("url").textContent)

it("orders the row, keeps Token kinds and leader frames behind popovers and reads old URLs", async () => {
  await renderInRouter(
    <Harness
      initial="type=token:spell,leader:white"
      counts={{
        main: 9,
        evolve: 4,
        advance: 1,
        leader: 3,
        "leader:gold": 2,
        "leader:white": 1,
        token: 5,
        "token:follower": 1,
        "token:spell": 2,
        "token:amulet": 1,
        "token:crest": 1,
        ep: 2,
        sep: 1,
      }}
    />,
  )
  const row = screen.getByRole("group", { name: "牌組位置" })
  expect(
    within(row)
      .getAllByRole("button")
      .map((button) => button.getAttribute("aria-label")),
  ).toEqual([
    "清除牌組位置",
    "主牌組",
    "進化",
    "進階",
    "Token",
    "Token的細項",
    "主戰者",
    "主戰者的細項",
    "EP",
    "SEP",
  ])
  for (const label of ["進化", "進階", "主戰者", "Token"]) {
    const chip = within(row).getByRole("button", { name: label })
    expect(chip.querySelector("img")).toHaveAttribute("alt", "")
    expect(chip).toHaveAttribute("title", expect.stringContaining(label))
  }
  // The older `type=` tokens arrive as locations and keep their sub-value selections.
  expect(url()).toBe("loc=leader:white,token:spell")
  const token = within(row).getByRole("button", { name: "Token" })
  const leader = within(row).getByRole("button", { name: "主戰者" })
  expect(token).toHaveAttribute("aria-pressed", "mixed")
  expect(leader).toHaveAttribute("aria-pressed", "mixed")
  await userEvent.click(within(row).getByRole("button", { name: "Token的細項" }))
  const kinds = screen.getByRole("group", { name: "Token的細項" })
  expect(
    within(kinds)
      .getAllByRole("checkbox")
      .map((checkbox) => checkbox.getAttribute("aria-label")),
  ).toEqual(["從者", "法術", "護符", "紋章"])
  expect(within(kinds).getByRole("checkbox", { name: "法術" })).toBeChecked()
  await userEvent.click(token)
  expect(token).toHaveAttribute("aria-pressed", "true")
  expect(url()).toBe("loc=leader:white,token")
  await userEvent.click(within(row).getByRole("button", { name: "主戰者的細項" }))
  const frames = screen.getByRole("group", { name: "主戰者的細項" })
  expect(within(frames).getByRole("checkbox", { name: "金框" })).toHaveAccessibleDescription("2")
  await userEvent.click(within(frames).getByRole("checkbox", { name: "金框" }))
  expect(leader).toHaveAttribute("aria-pressed", "true")
  expect(url()).toBe("loc=leader,token")
  await userEvent.click(within(row).getByRole("button", { name: "主牌組" }))
  await userEvent.click(within(row).getByRole("button", { name: "EP" }))
  expect(parseQuery(new URLSearchParams(url())).locations).toEqual([
    "ep",
    "leader",
    "main",
    "token",
  ])
  await userEvent.click(within(row).getByRole("button", { name: "清除牌組位置" }))
  expect(screen.getByLabelText("url")).toBeEmptyDOMElement()
})

it("hides locations the edition has no printings for, but always offers the main deck", async () => {
  await renderInRouter(<Harness initial="" counts={{ main: 3, "leader:gold": 1, leader: 1 }} />)
  const row = screen.getByRole("group", { name: "牌組位置" })
  expect(
    within(row)
      .getAllByRole("button")
      .map((button) => button.getAttribute("aria-label")),
  ).toEqual(["主牌組", "主戰者", "主戰者的細項"])
})
