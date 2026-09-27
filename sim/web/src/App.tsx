import { useTranslation } from "react-i18next"

export function App() {
  const { t } = useTranslation()
  return (
    <main className="flex min-h-dvh flex-col items-center justify-center gap-2 bg-surface p-6 text-text">
      <h1 className="text-2xl font-bold">{t("app.title")}</h1>
      <p className="text-text-muted">{t("app.tagline")}</p>
    </main>
  )
}
