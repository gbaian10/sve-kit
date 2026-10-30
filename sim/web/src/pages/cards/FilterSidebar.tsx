import { ChevronsLeft, ChevronsRight } from "lucide-react"
import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

import { cn } from "../../components/ui/cn"
import type { SidebarEdge } from "./sidebar-layout"

export interface FilterSidebarProps {
  readonly edge: SidebarEdge
  readonly collapsed: boolean
  readonly onToggle: () => void
  /** Facets set beyond the search text; shown on the toggle while expanded. */
  readonly filterCount: number
  readonly children: ReactNode
}

/**
 * The filter sidebar of the cards page: pinned below the top bar and scrolling on its own, so the
 * filters stay reachable however far the results are scrolled. It folds into a thin strip.
 */
export function FilterSidebar({
  edge,
  collapsed,
  onToggle,
  filterCount,
  children,
}: FilterSidebarProps) {
  const { t } = useTranslation()
  // The arrows point to the window edge the sidebar folds toward, then back out once folded.
  const Arrow = (edge === "left") !== collapsed ? ChevronsLeft : ChevronsRight
  const label = collapsed ? t("filters.sidebarExpand") : t("filters.sidebarCollapse")
  return (
    <aside
      aria-label={t("filters.panel")}
      data-edge={edge}
      className={cn(
        // Let the scroll area extend past the results grid when the page footer enters view.
        "sticky top-13 z-40 -mb-[calc(100dvh-3.25rem)] flex h-[calc(100dvh-3.25rem)] min-w-0 flex-col gap-4 overflow-y-auto py-3 pr-1",
        edge === "right" && "order-last",
      )}
    >
      <button
        type="button"
        aria-expanded={!collapsed}
        aria-label={label}
        title={label}
        onClick={onToggle}
        className={cn(
          "flex h-9 shrink-0 items-center gap-2 rounded-button border border-border bg-surface-1 px-2 text-13 text-text-1 hover:bg-surface-2",
          collapsed ? "justify-center" : "justify-between",
          !collapsed && edge === "right" && "flex-row-reverse",
        )}
      >
        <Arrow aria-hidden="true" className="size-4 shrink-0" />
        {!collapsed && (
          <span className="font-semibold">
            {t("search.filters")}
            {filterCount > 0 && ` (${String(filterCount)})`}
          </span>
        )}
      </button>
      {!collapsed && children}
    </aside>
  )
}
