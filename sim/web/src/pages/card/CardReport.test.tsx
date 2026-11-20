import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, it, vi } from "vitest"

import { renderRoutes } from "../../test-utils"
import { CardReportDialog } from "./CardReport"

afterEach(() => vi.unstubAllGlobals())

it("submits a translation report with read-only card details and links to the created issue", async () => {
  const turnstile = {
    render: vi.fn((_container: HTMLElement, options: { callback(token: string): void }) => {
      options.callback("XXXX.DUMMY.TOKEN.XXXX")
      return "test-widget"
    }),
    remove: vi.fn(),
  }
  vi.stubGlobal("turnstile", turnstile)
  const send = vi
    .fn<typeof fetch>()
    .mockResolvedValue(Response.json({ issueNumber: 987 }, { status: 201 }))
  vi.stubGlobal("fetch", send)
  const user = userEvent.setup()

  await renderRoutes([
    {
      path: "/",
      element: (
        <CardReportDialog
          onClose={() => undefined}
          draft={{
            initialType: "translation",
            metadata: {
              cardNo: "BP03-LDⓈ01EN",
              printingIntId: null,
              edition: "en",
              uiLanguage: "zh-TW",
              dataVersion: "preview-20261121T010000Z-0001",
            },
          }}
        />
      ),
    },
  ])

  const dialog = within(screen.getByRole("dialog", { name: "回報這張卡的問題" }))
  expect(dialog.getByText("BP03-LDⓈ01EN")).toBeVisible()
  expect(dialog.getByRole("combobox", { name: "問題類型" })).toHaveValue("translation")
  const description = dialog.getByRole("textbox", { name: "問題說明（必填）" })
  expect(description).toHaveValue("")
  await user.type(description, "The translation is missing an instruction.")
  const button = dialog.getByRole("button", { name: "送出回報" })
  await waitFor(() => expect(button).toBeEnabled())
  await user.click(button)

  expect(await dialog.findByRole("link", { name: /前往 GitHub 查看 issue/ })).toHaveAttribute(
    "href",
    "https://github.com/gbaian10/sve-kit/issues/987",
  )
  const [path, init] = send.mock.calls[0] ?? []
  expect(path).toBe("/api/card-reports")
  expect(init?.method).toBe("POST")
  if (typeof init?.body !== "string") throw new Error("Missing JSON body")
  const payload: unknown = JSON.parse(init.body)
  expect(payload).toEqual({
    cardNo: "BP03-LDⓈ01EN",
    printingIntId: null,
    edition: "en",
    uiLanguage: "zh-TW",
    dataVersion: "preview-20261121T010000Z-0001",
    type: "translation",
    description: "The translation is missing an instruction.",
    contact: "",
    token: "XXXX.DUMMY.TOKEN.XXXX",
  })
})
