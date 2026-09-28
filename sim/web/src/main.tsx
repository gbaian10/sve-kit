import "./styles/index.css"

import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { I18nextProvider } from "react-i18next"

import { App } from "./App"
import { currentUiLanguage } from "./app/language"
import { installOverflowAudit } from "./app/overflow-audit"
import { createI18n } from "./i18n"
import { prefsStore } from "./settings"

const root = document.getElementById("root")
if (!root) throw new Error("#root is missing from index.html")

const language = currentUiLanguage(prefsStore.get().uiLanguage)
document.documentElement.lang = language

// Dev only: text that overflows becomes a console error, and `?pseudo` stretches every string.
const pseudo = import.meta.env.DEV && new URLSearchParams(window.location.search).has("pseudo")
if (import.meta.env.DEV) installOverflowAudit()

void createI18n(language, { pseudo }).then((i18n) => {
  createRoot(root).render(
    <StrictMode>
      <I18nextProvider i18n={i18n}>
        <App />
      </I18nextProvider>
    </StrictMode>,
  )
})
