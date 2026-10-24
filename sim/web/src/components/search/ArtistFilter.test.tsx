import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, it } from "vitest"

import { formatQuery, parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { renderInRouter } from "../../test-utils"
import { ArtistFilter } from "./ArtistFilter"

const id = (letter: string) => `y:${letter.repeat(32)}`
const EDITIONS = {
  jp: {
    artists: [
      { id: id("a"), name: "ぽんず" },
      { id: id("b"), name: "久方綜司" },
      { id: id("c"), name: "虹原" },
    ],
    counts: { [id("a")]: 5, [id("b")]: 4, [id("c")]: 3, credited: 12, uncredited: 8 },
  },
  en: {
    artists: [{ id: id("d"), name: "Ponzu" }],
    counts: { [id("d")]: 5, credited: 6, uncredited: 2 },
  },
}

function Harness({ initial = "" }: { initial?: string }) {
  const [edition, setEdition] = useState<"jp" | "en">("jp")
  const [selected, onChange] = useState(() => parseQuery(new URLSearchParams(initial)).artists)
  return (
    <>
      <button
        type="button"
        onClick={() => {
          setEdition("en")
        }}
      >
        en
      </button>
      <ArtistFilter
        textLang="zh-Hant"
        selected={selected}
        onChange={onChange}
        options={EDITIONS[edition]}
      />
      <output aria-label="url">
        {formatQuery({ ...DEFAULT_QUERY, artists: selected }).toString()}
      </output>
    </>
  )
}

const url = () => decodeURIComponent(screen.getByLabelText("url").textContent)
const open = async () => {
  const button = screen.getByRole("button", { name: /^繪師/u })
  await userEvent.click(button)
  return document.getElementById(button.getAttribute("aria-controls") ?? "") as HTMLElement
}
const box = (list: HTMLElement, name: string) =>
  within(list).getByRole<HTMLInputElement>("checkbox", { name })

it("puts the two summary rows above the list, with the whole edition's counts", async () => {
  await renderInRouter(<Harness />)
  const list = await open()
  const names = within(list)
    .getAllByRole("checkbox")
    .map((checkbox) => checkbox.getAttribute("aria-label"))
  // The artists follow the UI language's collation; the summary rows stay first.
  expect(names.slice(0, 2)).toEqual(["官方有標示", "官方未標示"])
  expect(names.slice(2).sort()).toEqual(["ぽんず", "久方綜司", "虹原"].sort())
  expect(within(list).getByText("12")).toBeVisible()
  expect(within(list).getByText("8")).toBeVisible()
  // The search narrows the artists below, never the summary rows.
  await userEvent.type(within(list).getByRole("searchbox"), "久方")
  expect(
    within(list)
      .getAllByRole("checkbox")
      .map((checkbox) => checkbox.getAttribute("aria-label")),
  ).toEqual(["官方有標示", "官方未標示", "久方綜司"])
})

it("selects every artist from the credited row and clears them again, keeping the uncredited one", async () => {
  await renderInRouter(<Harness initial="artist=uncredited" />)
  const list = await open()
  await userEvent.click(box(list, "官方有標示"))
  expect(url()).toBe("artist=credited,uncredited")
  for (const name of ["官方有標示", "ぽんず", "久方綜司", "虹原"])
    expect(box(list, name)).toBeChecked()
  expect(box(list, "官方未標示")).toBeChecked()
  await userEvent.click(box(list, "官方有標示"))
  expect(url()).toBe("artist=uncredited")
  for (const name of ["ぽんず", "久方綜司", "虹原"]) expect(box(list, name)).not.toBeChecked()
})

it("shows half when some artists are ticked, and turns full once all are", async () => {
  await renderInRouter(<Harness />)
  const list = await open()
  await userEvent.click(box(list, "ぽんず"))
  expect(box(list, "官方有標示").indeterminate).toBe(true)
  expect(box(list, "官方有標示")).not.toBeChecked()
  await userEvent.click(box(list, "久方綜司"))
  await userEvent.click(box(list, "虹原"))
  expect(url()).toBe("artist=credited")
  expect(box(list, "官方有標示")).toBeChecked()
  expect(box(list, "官方有標示").indeterminate).toBe(false)
})

it("lists the rest when one artist is unticked after all were selected", async () => {
  await renderInRouter(<Harness initial="artist=credited,uncredited" />)
  const list = await open()
  await userEvent.click(box(list, "久方綜司"))
  // The URL lists its values in code order, so the word comes before the ids.
  expect(url()).toBe(`artist=uncredited,${[id("a"), id("c")].join(",")}`)
  expect(box(list, "久方綜司")).not.toBeChecked()
  expect(box(list, "官方有標示").indeterminate).toBe(true)
  expect(box(list, "官方未標示")).toBeChecked()
})

it("takes the counts of the edition it is given", async () => {
  await renderInRouter(<Harness />)
  await userEvent.click(screen.getByRole("button", { name: "en" }))
  const list = await open()
  expect(within(list).getByText("6")).toBeVisible()
  expect(within(list).getByText("2")).toBeVisible()
  expect(
    within(list)
      .getAllByRole("checkbox")
      .map((checkbox) => checkbox.getAttribute("aria-label")),
  ).toEqual(["官方有標示", "官方未標示", "Ponzu"])
})
