import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { describe, expect, it } from "vitest"

import { formatQuery, parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY, type QueryState } from "../../domain/query/model"
import { renderInRouter } from "../../test-utils"
import { ConditionChips } from "./ConditionChips"

const GROUPS = {
  pr: ["pr:br", "pr:gr", "pr:unknown"],
  token: ["token:follower", "token:spell"],
  leader: ["leader:gold", "leader:white"],
}

function Harness({ initial }: { readonly initial: string }) {
  const [query, setQuery] = useState<QueryState>(() => parseQuery(new URLSearchParams(initial)))
  return (
    <>
      <ConditionChips
        query={query}
        edition="jp"
        classLabel={(code) => code}
        rarityLabel={(code) => code.toUpperCase()}
        typeLabel={(code) => ({ leader: "主戰者", spell: "法術", follower: "從者" })[code] ?? code}
        setLabel={(code) => code}
        groups={GROUPS}
        onChange={setQuery}
        onClear={() => {
          setQuery(DEFAULT_QUERY)
        }}
      />
      <output aria-label="url">{formatQuery(query).toString()}</output>
    </>
  )
}

const chips = () =>
  within(screen.getByRole("group", { name: "目前條件" }))
    .getAllByRole("button", { name: /^移除條件：/u })
    .map((button) => button.getAttribute("aria-label")?.replace(/^移除條件：/u, ""))
const url = () => decodeURIComponent(screen.getByLabelText("url").textContent)

describe("condition chips for grouped values", () => {
  it("shows one chip for a complete set of sub-values and clears the whole group from it", async () => {
    await renderInRouter(
      <Harness initial="type=leader:gold,leader:white&rarity=pr:br,pr:gr,pr:unknown" />,
    )
    expect(chips()).toEqual(["PR", "主戰者"])
    await userEvent.click(screen.getByRole("button", { name: "移除條件：主戰者" }))
    expect(chips()).toEqual(["PR"])
    expect(url()).toBe("rarity=pr:br,pr:gr,pr:unknown")
    await userEvent.click(screen.getByRole("button", { name: "移除條件：PR" }))
    expect(screen.queryByRole("group", { name: "目前條件" })).not.toBeInTheDocument()
    expect(url()).toBe("")
  })

  it("shows one chip when the parent and sub-values coexist, and its × clears both", async () => {
    await renderInRouter(<Harness initial="rarity=pr,pr:gr,br&type=token,token:spell,follower" />)
    expect(chips()).toEqual(["BR", "PR", "從者", "Token"])
    await userEvent.click(screen.getByRole("button", { name: "移除條件：Token" }))
    expect(chips()).toEqual(["BR", "PR", "從者"])
    expect(url()).toBe("type=follower&rarity=br,pr,pr:gr")
    await userEvent.click(screen.getByRole("button", { name: "移除條件：PR" }))
    expect(url()).toBe("type=follower&rarity=br")
  })

  it("keeps one chip per sub-value while the selection is partial, each removable alone", async () => {
    await renderInRouter(<Harness initial="type=leader:gold,token:spell&rarity=pr:gr" />)
    expect(chips()).toEqual(["PR（GR）", "Token・法術", "主戰者・金框"])
    await userEvent.click(screen.getByRole("button", { name: "移除條件：Token・法術" }))
    expect(chips()).toEqual(["PR（GR）", "主戰者・金框"])
    expect(url()).toBe("type=leader:gold&rarity=pr:gr")
    await userEvent.click(screen.getByRole("button", { name: "移除條件：PR（GR）" }))
    expect(url()).toBe("type=leader:gold")
  })
})
