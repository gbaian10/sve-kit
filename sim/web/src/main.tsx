import "./styles/index.css"

import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { I18nextProvider } from "react-i18next"

import { App } from "./App"
import { currentUiLanguage } from "./app/language"
import { installOverflowAudit } from "./app/overflow-audit"
import { registerImageCache } from "./data"
import { createI18n } from "./i18n"
import { prefsStore, runRequestedColdStart } from "./settings"

const root = document.getElementById("root")
if (!root) throw new Error("#root is missing from index.html")

async function start(container: HTMLElement): Promise<void> {
  // A cold start asked for from the dev badge clears local data before anything caches again.
  await runRequestedColdStart()
  registerImageCache()

  const language = currentUiLanguage(prefsStore.get().uiLanguage)
  document.documentElement.lang = language

  // Dev only: text that overflows becomes a console error, and `?pseudo` stretches every string.
  const pseudo = import.meta.env.DEV && new URLSearchParams(window.location.search).has("pseudo")
  if (import.meta.env.DEV) installOverflowAudit()

  const i18n = await createI18n(language, { pseudo })
  createRoot(container).render(
    <StrictMode>
      <I18nextProvider i18n={i18n}>
        <App />
      </I18nextProvider>
    </StrictMode>,
  )
}

void start(root)
