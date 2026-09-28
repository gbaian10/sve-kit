import { useTranslation } from "react-i18next"

import { useThemeAttributes } from "./app/theme-attributes"

export function App() {
  const { t } = useTranslation()
  useThemeAttributes()
  return (
    <main className="flex min-h-dvh flex-col items-center justify-center gap-2 bg-bg p-6 text-text-1">
      <h1 className="text-24 font-bold">{t("app.title")}</h1>
      <p className="text-text-2">{t("app.tagline")}</p>
    </main>
  )
}
