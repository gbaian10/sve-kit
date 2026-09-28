import "./styles/index.css"

import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { I18nextProvider } from "react-i18next"

import { App } from "./App"
import { currentUiLanguage } from "./app/language"
import { createI18n } from "./i18n"
import { prefsStore } from "./settings"

const root = document.getElementById("root")
if (!root) throw new Error("#root is missing from index.html")

const language = currentUiLanguage(prefsStore.get().uiLanguage)
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
