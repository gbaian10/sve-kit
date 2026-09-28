import type { ComponentPropsWithoutRef } from "react"

import { cn } from "./cn"

type ButtonVariant = "primary" | "secondary" | "danger" | "ghost"
type ButtonSize = "md" | "lg" | "icon"

export interface ButtonProps extends ComponentPropsWithoutRef<"button"> {
  readonly variant?: ButtonVariant
  readonly size?: ButtonSize
}

// Design §7: 44px controls (48 for the main action), radius 12; brand colour only on the primary
// variant; disabled = surface-3 with tertiary text.
const VARIANT: Record<ButtonVariant, string> = {
  primary:
    "bg-accent text-accent-ink hover:brightness-105 disabled:bg-surface-3 disabled:text-text-3",
  secondary:
    "border border-border-strong bg-transparent text-text-1 hover:bg-surface-2 disabled:text-text-3",
  danger:
    "border border-danger bg-transparent text-danger hover:bg-danger-soft disabled:text-text-3",
  ghost: "bg-transparent text-text-1 hover:bg-surface-2 disabled:text-text-3",
}

const SIZE: Record<ButtonSize, string> = {
  md: "h-11 min-w-11 px-4 text-15",
  lg: "h-12 min-w-12 px-5 text-15",
  icon: "size-11",
}

export function Button({
  variant = "secondary",
  size = "md",
  className,
  type = "button",
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex shrink-0 items-center justify-center gap-2 rounded-button font-semibold",
        "disabled:cursor-not-allowed",
        VARIANT[variant],
        SIZE[size],
        className,
      )}
      {...rest}
    />
  )
}
