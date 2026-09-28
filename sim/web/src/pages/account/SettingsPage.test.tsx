import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it } from "vitest"

import { DEFAULT_PREFS, prefsStore, readPrefs } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { SettingsPage } from "./SettingsPage"

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("SettingsPage", () => {
  it("shows the display settings and writes them to the preferences", async () => {
    await renderInRouter(<SettingsPage />)
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("帳號與設定")
    await userEvent.click(screen.getByRole("switch", { name: "省流量" }))
    await userEvent.click(screen.getByRole("switch", { name: "圖示旁附名稱" }))
    await userEvent.click(screen.getByRole("radio", { name: "英文圈" }))
    await userEvent.click(
      within(screen.getByRole("radiogroup", { name: "效果文字" })).getByRole("radio", {
        name: "兩者",
      }),
    )
    expect(readPrefs()).toMatchObject({
      dataSaver: true,
      symbolLabels: true,
      banRegion: "en",
      effectLanguage: "both",
    })
    expect(screen.getByRole("switch", { name: "省流量" })).toHaveAttribute("aria-checked", "true")
  })
})
