export type SidebarEdge = "left" | "right"

/** The two-column grid of the cards page: the sidebar track on its own edge, results in the rest. */
export function sidebarGridClass(edge: SidebarEdge, collapsed: boolean): string {
  if (collapsed) return "grid-cols-1"
  return edge === "left" ? "grid-cols-[320px_minmax(0,1fr)]" : "grid-cols-[minmax(0,1fr)_320px]"
}
