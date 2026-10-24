import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

import { Dialog } from "../../components/ui/Dialog"

export interface FilterSheetProps {
  readonly open: boolean
  readonly onClose: () => void
  readonly children: ReactNode
}

export function FilterSheet({ open, onClose, children }: FilterSheetProps) {
  const { t } = useTranslation()
  // Keep the filters mounted between uses while Dialog handles focus and background isolation.
  return (
    <Dialog open={open} onClose={onClose} title={t("filters.panel")} variant="bottom">
      <div className="flex flex-col gap-3 [&_.sticky]:bottom-0">{children}</div>
    </Dialog>
  )
}
