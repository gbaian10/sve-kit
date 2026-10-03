import { useTranslation } from "react-i18next"
import { Outlet, ScrollRestoration } from "react-router"

import { BottomNav } from "../components/nav/BottomNav"
import { Footer } from "../components/nav/Footer"
import { SideRail } from "../components/nav/SideRail"
import { TopBar } from "../components/nav/TopBar"
import { ToastProvider } from "../components/ui/Toast"
import { DevBadge } from "./DevBadge"
import { useUiLanguageSync } from "./language"
import { useActiveSnapshot } from "./snapshot"
import { SnapshotNotice } from "./SnapshotNotice"
import { useThemeAttributes } from "./theme-attributes"

// Layout switches on content width (design §11): < lg bottom bar, ≥ lg left rail; content is
// capped at 1280 and centred.
export function AppShell() {
  const { t } = useTranslation()
  useThemeAttributes()
  useUiLanguageSync()
  const { client, status } = useActiveSnapshot()
  return (
    <ToastProvider>
      <div className="flex min-h-dvh flex-col pb-21 lg:pb-0 lg:pl-18 phone-landscape:pb-0 phone-landscape:pl-16">
        <a
          href="#main"
          className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:rounded-button focus:bg-surface-1 focus:px-4 focus:py-2"
        >
          {t("nav.skipToContent")}
        </a>
        <SideRail />
        <TopBar />
        <SnapshotNotice client={client} status={status} />
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
