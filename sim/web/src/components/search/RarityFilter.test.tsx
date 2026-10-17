import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { describe, expect, it } from "vitest"

import { renderInRouter } from "../../test-utils"
import { RarityFilter } from "./RarityFilter"

function Harness({ initial = [] }: { readonly initial?: string[] }) {
  const [selected, onChange] = useState(initial)
  return (
    <>
      <RarityFilter
        rarities={[
          { code: "sp", label: "SP" },
          { code: "pr", label: "PR" },
        ]}
        options={{
          styles: ["br", "none", "unknown"],
          counts: { sp: 1, pr: 6, "pr:br": 3, "pr:none": 2, "pr:unknown": 1, unrated: 4 },
        }}
        selected={selected}
        onChange={onChange}
      />
      <output aria-label="selection">{selected.join(",")}</output>
    </>
  )
}

describe("rarity checkbox tree", () => {
  it("supports parent, mixed children, unions and canonical PR values", async () => {
    await renderInRouter(<Harness />)
    await userEvent.click(screen.getByRole("button", { name: /稀有度/u }))
    const group = screen.getByRole("group", { name: "稀有度" })
    const parent = within(group).getByRole("checkbox", { name: "PR" })
    expect(parent).toHaveAccessibleDescription("6")
    expect(within(group).queryByRole("checkbox", { name: "PR（SR）" })).not.toBeInTheDocument()
    await userEvent.click(parent)
    expect(screen.getByLabelText("selection")).toHaveTextContent(/^pr$/u)
    expect(within(group).getByRole("checkbox", { name: "PR（BR）" })).toBeChecked()
    await userEvent.click(within(group).getByRole("checkbox", { name: "PR（無標籤）" }))
    expect(parent).toBePartiallyChecked()
    expect(parent).toHaveAttribute("aria-checked", "mixed")
    await userEvent.click(within(group).getByRole("checkbox", { name: "SP" }))
    await userEvent.click(within(group).getByRole("checkbox", { name: "無分類" }))
    expect(screen.getByLabelText("selection")).toHaveTextContent("pr:br,pr:unknown,sp,unrated")
    await userEvent.click(parent)
    expect(screen.getByLabelText("selection")).toHaveTextContent("pr,sp,unrated")
    await userEvent.click(parent)
    expect(screen.getByLabelText("selection")).toHaveTextContent("sp,unrated")
  })
  it("supports keyboard expansion and checkbox toggling from a partial selection", async () => {
    await renderInRouter(<Harness initial={["pr:br"]} />)
    const button = screen.getByRole("button", { name: /稀有度/u })
    button.focus()
    await userEvent.keyboard("{Enter}")
    expect(button).toHaveAttribute("aria-expanded", "true")
    await userEvent.tab()
    expect(screen.getByRole("checkbox", { name: "SP" })).toHaveFocus()
    await userEvent.keyboard(" ")
    expect(screen.getByLabelText("selection")).toHaveTextContent("pr:br,sp")
    expect(screen.getByRole("checkbox", { name: "PR" })).toBePartiallyChecked()
  })
})
