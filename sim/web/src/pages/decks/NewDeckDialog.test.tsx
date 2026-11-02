import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"

import type { Region } from "../../domain/search"
import { prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { NewDeckDialog } from "./NewDeckDialog"

const titles = [
  { code: "umamusume", count: 3 },
  { code: "vanguard", count: 2 },
]

function renderDialog(titlesEdition: Region) {
  prefsStore.set({ uiLanguage: "zh-TW" })
  const onEditionChange = vi.fn()
  const onCreate = vi.fn()
  const element = (
    <NewDeckDialog
      open
      onClose={() => undefined}
      defaultEdition="jp"
      onEditionChange={onEditionChange}
      defaultName="新牌組 1"
      classOptions={[{ code: "elf", label: "精靈" }]}
      titles={titles}
      titlesEdition={titlesEdition}
      titleName={(code) => code}
      onCreate={onCreate}
    />
  )
  return { onEditionChange, onCreate, element }
}

describe("NewDeckDialog", () => {
  it("offers titles only once they belong to the edition chosen in the dialog", async () => {
    const { onEditionChange, onCreate, element } = renderDialog("jp")
    await renderRoutes([{ path: "/", element }])
    const user = userEvent.setup()
    const dialog = screen.getByRole("dialog", { name: "新牌組" })
    await user.click(within(dialog).getByRole("radio", { name: "作品名" }))
    expect(within(dialog).getByRole("button", { name: /umamusume/u })).toBeInTheDocument()

    await user.click(within(dialog).getByRole("radio", { name: "英版" }))
    expect(onEditionChange).toHaveBeenCalledWith("en")
    // The page still holds the JP titles: none is offered and nothing can be created yet.
    expect(within(dialog).queryByRole("button", { name: /umamusume/u })).not.toBeInTheDocument()
    expect(within(dialog).getByRole("button", { name: "建立" })).toBeDisabled()
    expect(onCreate).not.toHaveBeenCalled()
  })

  it("creates the deck in the dialog's edition with the chosen title", async () => {
    const { onCreate, element } = renderDialog("en")
    await renderRoutes([{ path: "/", element }])
    const user = userEvent.setup()
    const dialog = screen.getByRole("dialog", { name: "新牌組" })
    await user.click(within(dialog).getByRole("radio", { name: "英版" }))
    await user.click(within(dialog).getByRole("radio", { name: "作品名" }))
    await user.click(within(dialog).getByRole("button", { name: /vanguard/u }))
    await user.click(within(dialog).getByRole("button", { name: "建立" }))
    expect(onCreate).toHaveBeenCalledWith({
      name: "新牌組 1",
      edition: "en",
      basis: { kind: "title", titleCode: "vanguard" },
    })
  })
})
