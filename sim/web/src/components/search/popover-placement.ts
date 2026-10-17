export interface PopoverBox {
  /** The anchor's horizontal edges in viewport coordinates. */
  readonly anchorLeft: number
  readonly anchorRight: number
  readonly panelWidth: number
  readonly viewportWidth: number
}

/**
 * Which anchor edge a popover should hang from: its own left edge when the panel then fits the
 * viewport, otherwise the right edge; when neither fits, the side with more room. Computed from
 * the anchor, never from a panel that may already be aligned the other way.
 */
export function popoverAlignment(box: PopoverBox): "start" | "end" {
  const startOverflow = box.anchorLeft + box.panelWidth - box.viewportWidth
  if (startOverflow <= 0) return "start"
  const endOverflow = box.panelWidth - box.anchorRight
  if (endOverflow <= 0) return "end"
  return endOverflow < startOverflow ? "end" : "start"
}
