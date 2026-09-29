import { useState } from "react"
import { useTranslation } from "react-i18next"
import { Outlet, ScrollRestoration } from "react-router"

import { BottomNav } from "../components/nav/BottomNav"
import { Footer } from "../components/nav/Footer"
import { SideRail } from "../components/nav/SideRail"
import { TopBar } from "../components/nav/TopBar"
import { cn } from "../components/ui/cn"
import { ToastProvider } from "../components/ui/Toast"
import { DevBadge } from "./DevBadge"
import { useUiLanguageSync } from "./language"
import { useActiveSnapshot } from "./snapshot"
import { useThemeAttributes } from "./theme-attributes"

// Layout switches on content width (design §11): < lg bottom bar, ≥ lg left rail; content is
// capped at 1280 and centred.
export function AppShell() {
  const { t } = useTranslation()
  const [railExpanded, setRailExpanded] = useState(false)
  useThemeAttributes()
  useUiLanguageSync()
  const { client, status } = useActiveSnapshot()
  return (
    <ToastProvider>
      <div
        className={cn(
          "flex min-h-dvh flex-col pb-21 lg:pb-0 phone-landscape:pb-0 phone-landscape:pl-16",
          railExpanded ? "lg:pl-18 xl:pl-50" : "lg:pl-18",
        )}
      >
        <a
          href="#main"
          className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:rounded-button focus:bg-surface-1 focus:px-4 focus:py-2"
        >
          {t("nav.skipToContent")}
        </a>
        <SideRail
          expanded={railExpanded}
          onToggle={() => {
            setRailExpanded((value) => !value)
          }}
        />
        <TopBar />
        <main id="main" className="mx-auto w-full max-w-320 flex-1 px-4 lg:px-6">
          <Outlet />
        </main>
        <Footer dataVersion={status.state === "ready" ? status.dataVersion : undefined} />
        <BottomNav />
        {import.meta.env.DEV && <DevBadge client={client} />}
      </div>
      <ScrollRestoration getKey={(location) => location.pathname + location.search} />
    </ToastProvider>
  )
}
