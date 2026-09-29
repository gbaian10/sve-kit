import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it } from "vitest"

import { DEFAULT_PREFS, prefsStore, readPrefs } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { AccountMenuButton, AccountMenuPanel } from "./AccountMenu"

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("AccountMenuPanel", () => {
  it("writes every choice to the preferences", async () => {
    await renderInRouter(<AccountMenuPanel />)
    await userEvent.click(screen.getByRole("radio", { name: "英版" }))
    await userEvent.click(
      within(screen.getByRole("radiogroup", { name: "卡名顯示" })).getByRole("radio", {
        name: "原文",
      }),
    )
    await userEvent.click(screen.getByRole("radio", { name: "深色" }))
    await userEvent.click(screen.getByRole("radio", { name: "紅" }))
    expect(readPrefs()).toMatchObject({
      cardEdition: "en",
      nameDisplay: "original",
      theme: "dark",
      accent: "red",
    })
    expect(screen.getByRole("radio", { name: "英版" })).toHaveAttribute("aria-checked", "true")
  })

  it("offers the effect-text setting too", async () => {
    await renderInRouter(<AccountMenuPanel />)
    await userEvent.click(
      within(screen.getByRole("radiogroup", { name: "效果文字" })).getByRole("radio", {
        name: "兩者",
      }),
    )
    expect(readPrefs().effectLanguage).toBe("both")
  })

  it("shows the detected language when none is stored (jsdom reports en-US)", async () => {
    await renderInRouter(<AccountMenuPanel />)
    expect(
      within(screen.getByRole("radiogroup", { name: "介面語言" })).getByRole("radio", {
        name: "English",
      }),
    ).toHaveAttribute("aria-checked", "true")
  })

  it("marks the accent the page actually uses before any choice (light system → teal)", async () => {
    await renderInRouter(<AccountMenuPanel />)
    const group = screen.getByRole("radiogroup", { name: "強調色" })
    expect(within(group).getByRole("radio", { name: "青綠" })).toHaveAttribute(
      "aria-checked",
      "true",
    )
    expect(within(group).getByRole("radio", { name: "青綠" })).toHaveAttribute("tabindex", "0")
    expect(within(group).getByRole("radio", { name: "琥珀" })).toHaveAttribute("tabindex", "-1")
    within(group).getByRole("radio", { name: "青綠" }).focus()
    await userEvent.keyboard("{ArrowRight}")
    expect(readPrefs().accent).toBe("red")
    expect(within(group).getByRole("radio", { name: "紅" })).toHaveFocus()
  })

  it("falls back to a plain toggle when the Popover API is missing (as in jsdom)", async () => {
    await renderInRouter(<AccountMenuButton />)
    const button = screen.getByRole("button", { name: "帳號與設定" })
    expect(button).toHaveAttribute("aria-expanded", "false")
    expect(screen.queryByRole("radiogroup", { name: "外觀" })).not.toBeInTheDocument()
    await userEvent.click(button)
    expect(button).toHaveAttribute("aria-expanded", "true")
    expect(screen.getByRole("radiogroup", { name: "外觀" })).toBeInTheDocument()
  })

  it("changes the UI language and the menu re-renders in it", async () => {
    await renderInRouter(<AccountMenuPanel />)
    await userEvent.click(screen.getByRole("radio", { name: "English" }))
    expect(readPrefs().uiLanguage).toBe("en")
  })

  it("links to the full settings page", async () => {
    const { router } = await renderInRouter(<AccountMenuPanel />, { initialEntries: ["/cards"] })
    await userEvent.click(screen.getByRole("button", { name: "所有設定" }))
    expect(router.state.location.pathname).toBe("/settings")
  })
})
