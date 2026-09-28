import type { ReactNode } from "react"

import { cn } from "../ui/cn"
import { useKeyboardOpen } from "./useKeyboardOpen"

export interface BottomBarProps {
  readonly children: ReactNode
  readonly className?: string
}

/** Every bottom-pinned element (nav, card actions, sheet buttons) sits in this: safe-area padding, side background. */
export function BottomBar({ children, className }: BottomBarProps) {
  const keyboardOpen = useKeyboardOpen()
  if (keyboardOpen) return null
  return (
    <div
      className={cn(
        "fixed inset-x-0 bottom-0 z-40 border-t border-border bg-side pb-[env(safe-area-inset-bottom)]",
        className,
      )}
    >
      {children}
    </div>
  )
}
