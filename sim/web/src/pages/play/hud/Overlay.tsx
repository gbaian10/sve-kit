import { type ReactNode, useEffect, useRef } from "react"

import { cn } from "../../../components/ui/cn"

/**
 * A layer over the battle screen, dimming it. It is the native modal dialog, so everything behind
 * it is inert and Escape closes it; `front` puts it above another open layer.
 */
export function Overlay({
  label,
  front = false,
  onClose,
  children,
}: {
  readonly label: string
  readonly front?: boolean
  readonly onClose: () => void
  readonly children: ReactNode
}) {
  const ref = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    const dialog = ref.current
    if (dialog === null) return
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
    dialog.showModal()
    return () => {
      dialog.close()
      opener?.focus()
    }
  }, [])
  return (
    // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-noninteractive-element-interactions -- a click on the dimmed area is a pointer-only shortcut; keyboard users have Escape and the close button
    <dialog
      ref={ref}
      className={cn("bp-scrim", front && "bp-zoom-scrim")}
      aria-label={label}
      onCancel={(event) => {
        event.preventDefault()
        onClose()
      }}
      onClick={(event) => {
        if (event.target === ref.current) onClose()
      }}
    >
      {children}
    </dialog>
  )
}
