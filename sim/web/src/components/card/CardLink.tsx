import type { ComponentPropsWithoutRef } from "react"
import { Link as RouterLink } from "react-router"

import { cn } from "../ui/cn"
import { type CardLinkTarget, useCardLinkClick } from "./card-link-context"

type CardLinkProps = Omit<ComponentPropsWithoutRef<typeof RouterLink>, "to" | "state"> & {
  readonly link: CardLinkTarget
}

/** The brand-coloured link to a card, which goes back instead of stacking when the target says so. */
export function CardLink({ link, className, ...rest }: CardLinkProps) {
  const linkClick = useCardLinkClick()
  return (
    <RouterLink
      to={link.to}
      state={link.state}
      onClick={linkClick(link)}
      className={cn("text-accent-text underline-offset-2 hover:underline", className)}
      {...rest}
    />
  )
}
