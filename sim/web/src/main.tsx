import "./styles/index.css"

import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { I18nextProvider } from "react-i18next"

import { App } from "./App"
import { createI18n, detectUiLanguage, preferredLanguages } from "./i18n"
import { loadUiLanguage } from "./settings"

const root = document.getElementById("root")
if (!root) throw new Error("#root is missing from index.html")

const language = loadUiLanguage() ?? detectUiLanguage(preferredLanguages(globalThis.navigator))
document.documentElement.lang = language

void createI18n(language).then((i18n) => {
  createRoot(root).render(
    <StrictMode>
      <I18nextProvider i18n={i18n}>
        <App />
      </I18nextProvider>
    </StrictMode>,
  )
})
