import { UserRound } from "lucide-react"
import { useId, useState } from "react"
import { useTranslation } from "react-i18next"

import { cn } from "../ui/cn"
import { POPOVER_SUPPORTED } from "./account-menu-popover"
import { AccountMenuPanel } from "./AccountMenuPanel"

export { AccentPicker } from "./AccountMenuPanel"

export function AccountMenuButton() {
  const { t } = useTranslation()
  const id = useId()
  const panelId = `${id}-account`
  const [fallbackOpen, setFallbackOpen] = useState(false)
  const close = () => {
    if (POPOVER_SUPPORTED) document.getElementById(panelId)?.hidePopover()
    else setFallbackOpen(false)
  }
  return (
    <>
      <button
        type="button"
        popoverTarget={POPOVER_SUPPORTED ? panelId : undefined}
        aria-controls={panelId}
        aria-expanded={POPOVER_SUPPORTED ? undefined : fallbackOpen}
        onClick={
          POPOVER_SUPPORTED
            ? undefined
            : () => {
                setFallbackOpen((open) => !open)
              }
        }
        aria-label={t("nav.account")}
        className="flex size-11 shrink-0 items-center justify-center rounded-full text-text-2 hover:bg-surface-2"
      >
        <span className="flex size-8 items-center justify-center rounded-full border-2 border-dashed border-border-strong">
          <UserRound className="size-4" aria-hidden="true" />
        </span>
      </button>
      <div
        id={panelId}
        popover={POPOVER_SUPPORTED ? "auto" : undefined}
        hidden={POPOVER_SUPPORTED ? undefined : !fallbackOpen}
        className={cn(
          "m-0 rounded-block border border-border bg-surface-1 p-0 text-text-1 shadow-lg",
          POPOVER_SUPPORTED ? "inset-auto" : "fixed z-50",
          "top-14 right-[calc(1rem+var(--drawer-width,0px)*var(--drawer-on-right,0))] lg:right-[calc(1.5rem+var(--drawer-width,0px)*var(--drawer-on-right,0))]",
        )}
      >
        <AccountMenuPanel onNavigate={close} />
      </div>
    </>
  )
}
