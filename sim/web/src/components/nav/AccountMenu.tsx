import { UserRound } from "lucide-react"
import { useId } from "react"
import { useTranslation } from "react-i18next"

import { AccountMenuPanel } from "./AccountMenuPanel"

export { AccentPicker } from "./AccountMenuPanel"

export function AccountMenuButton() {
  const { t } = useTranslation()
  const id = useId()
  const panelId = `${id}-account`
  const close = () => document.getElementById(panelId)?.hidePopover()
  return (
    <>
      <button
        type="button"
        popoverTarget={panelId}
        aria-controls={panelId}
        aria-label={t("nav.account")}
        className="flex size-11 shrink-0 items-center justify-center rounded-full text-text-2 hover:bg-surface-2"
      >
        <span className="flex size-8 items-center justify-center rounded-full border-2 border-dashed border-border-strong">
          <UserRound className="size-4" aria-hidden="true" />
        </span>
      </button>
      <div
        id={panelId}
        popover="auto"
        className="inset-auto top-14 right-[calc(1rem+var(--drawer-width,0px)*var(--drawer-on-right,0))] m-0 rounded-block border border-border bg-surface-1 p-0 text-text-1 shadow-lg lg:right-[calc(1.5rem+var(--drawer-width,0px)*var(--drawer-on-right,0))]"
      >
        <AccountMenuPanel onNavigate={close} />
      </div>
    </>
  )
}
