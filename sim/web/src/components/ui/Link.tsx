import type { ComponentPropsWithoutRef } from "react"
import { Link as RouterLink } from "react-router"

import { cn } from "./cn"

export type LinkProps = ComponentPropsWithoutRef<typeof RouterLink>

// Links are one of the few places the brand colour appears (design §8).
export function Link({ className, ...rest }: LinkProps) {
  return (
    <RouterLink
      className={cn("text-accent-text underline-offset-2 hover:underline", className)}
      {...rest}
    />
  )
}
