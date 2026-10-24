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
  it("remembers comparison choices separately for the route's edition", async () => {
    const { router } = await renderInRouter(<AccountMenuPanel />, { initialEntries: ["/cards"] })
    const jp = () => screen.getByRole("switch", { name: "顯示英版資料" })
    const en = () => screen.getByRole("switch", { name: "顯示日版資料" })
    expect(jp()).toHaveAttribute("aria-checked", "false")
    await userEvent.click(jp())
    await userEvent.click(jp())
    expect(readPrefs().showOtherEdition).toEqual({ jp: false, en: true })
    await userEvent.click(jp())
    expect(jp()).toHaveAttribute("aria-checked", "true")
    await userEvent.click(screen.getByRole("radio", { name: "英版" }))
    expect(router.state.location.search).toBe("?ed=en")
    expect(en()).toHaveAttribute("aria-checked", "true")
    await userEvent.click(en())
    expect(readPrefs().showOtherEdition).toEqual({ jp: true, en: false })
    await userEvent.click(screen.getByRole("radio", { name: "日版" }))
    expect(jp()).toHaveAttribute("aria-checked", "true")
    await userEvent.click(jp())
    await userEvent.click(screen.getByRole("radio", { name: "英版" }))
    expect(en()).toHaveAttribute("aria-checked", "false")
    expect(readPrefs().showOtherEdition).toEqual({ jp: false, en: false })
  })

  it.each([
    ["zh-TW", "顯示英版資料"],
    ["ja", "英語版のデータを表示"],
    ["en", "Show English edition data"],
  ] as const)("labels the comparison switch in %s", async (language, label) => {
    await renderInRouter(<AccountMenuPanel />, { language })
    expect(screen.getByRole("switch", { name: label })).toBeInTheDocument()
  })

  it("writes every choice to the preferences", async () => {
    await renderInRouter(<AccountMenuPanel />)
    await userEvent.click(screen.getByRole("radio", { name: "英版" }))
    await userEvent.click(
      within(screen.getByRole("radiogroup", { name: "卡片翻譯" })).getByRole("radio", {
        name: "原文",
      }),
    )
    await userEvent.click(screen.getByRole("radio", { name: "深色" }))
    await userEvent.click(screen.getByRole("radio", { name: "紅" }))
    expect(readPrefs()).toMatchObject({
      cardEdition: "en",
      nameDisplay: "original",
      effectLanguage: "original",
      flavorDisplay: "original",
      qaDisplay: "original",
      theme: "dark",
      accent: "red",
    })
    expect(screen.getByRole("radio", { name: "英版" })).toHaveAttribute("aria-checked", "true")
  })

  it("sets the four text fields together and says custom when they differ", async () => {
    prefsStore.set({ qaDisplay: "none" })
    await renderInRouter(<AccountMenuPanel />)
    const group = screen.getByRole("radiogroup", { name: "卡片翻譯" })
    expect(screen.getByText("自訂")).toBeVisible()
    expect(within(group).queryByRole("radio", { checked: true })).toBeNull()
    await userEvent.click(within(group).getByRole("radio", { name: "翻譯" }))
    expect(readPrefs()).toMatchObject({
      nameDisplay: "translated",
      effectLanguage: "translated",
      flavorDisplay: "translated",
      qaDisplay: "translated",
    })
    expect(screen.queryByText("自訂")).toBeNull()
  })

  it("shows the detected language when none is stored (jsdom reports en-US)", async () => {
    await renderInRouter(<AccountMenuPanel />)
    expect(
      within(screen.getByRole("radiogroup", { name: "介面語言" })).getByRole("radio", {
        name: "English",
      }),
    ).toHaveAttribute("aria-checked", "true")
  })

  it("can enter the mixed translation setting by keyboard and select all four fields", async () => {
    await renderInRouter(<AccountMenuPanel />)
    const group = screen.getByRole("radiogroup", { name: "卡片翻譯" })
    const radios = within(group).getAllByRole("radio")
    expect(radios.map((radio) => radio.tabIndex)).toEqual([0, -1, -1])
    expect(within(group).queryByRole("radio", { checked: true })).toBeNull()
    within(group).getByRole("radio", { name: "原文" }).focus()
    await userEvent.keyboard("{ArrowLeft}")
    const both = within(group).getByRole("radio", { name: "兩者" })
    expect(both).toHaveFocus()
    expect(both).toHaveAttribute("aria-checked", "true")
    expect(readPrefs()).toMatchObject({
      nameDisplay: "both",
      effectLanguage: "both",
      flavorDisplay: "both",
      qaDisplay: "both",
    })
    await userEvent.keyboard("{Home}")
    expect(within(group).getByRole("radio", { name: "原文" })).toHaveFocus()
    expect(within(group).getByRole("radio", { name: "原文" })).toHaveAttribute(
      "aria-checked",
      "true",
    )
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
