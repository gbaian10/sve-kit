import { render, screen } from "@testing-library/react"
import { I18nextProvider } from "react-i18next"
import { describe, expect, it } from "vitest"

import { App } from "./App"
import { createI18n } from "./i18n"

describe("App", () => {
  it("renders the translated heading and tagline", async () => {
    const i18n = await createI18n("en")
    render(
      <I18nextProvider i18n={i18n}>
        <App />
      </I18nextProvider>,
    )
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("sve-kit")
    expect(screen.getByText(/Unofficial card list/)).toBeInTheDocument()
  })

  it("re-renders after an asynchronous language change", async () => {
    const i18n = await createI18n("zh-TW")
    render(
      <I18nextProvider i18n={i18n}>
        <App />
      </I18nextProvider>,
    )
    expect(screen.getByText(/非官方卡表/)).toBeInTheDocument()

    await i18n.changeLanguage("ja")

    expect(await screen.findByText(/非公式カードリスト/)).toBeInTheDocument()
    expect(screen.queryByText(/非官方卡表/)).not.toBeInTheDocument()
  })
})
