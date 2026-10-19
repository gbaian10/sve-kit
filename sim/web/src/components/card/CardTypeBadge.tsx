import { useTranslation } from "react-i18next"

import amulet from "../../assets/card-labels/amulet.svg"
import follower from "../../assets/card-labels/follower.svg"
import spell from "../../assets/card-labels/spell.svg"

const ASSETS = { follower, spell, amulet }

export function CardTypeBadge({
  code,
  label,
  decorative = false,
  fallback = false,
}: {
  readonly code: string | null | undefined
  readonly label?: string | undefined
  readonly decorative?: boolean
  readonly fallback?: boolean
}) {
  const { t } = useTranslation()
  if (code !== "follower" && code !== "spell" && code !== "amulet")
    return fallback && code ? <span>{label ?? code}</span> : null
  const name = label ?? t(`cardType.${code}`)
  return (
    <img
      src={ASSETS[code]}
      alt={decorative ? "" : name}
      title={decorative ? undefined : name}
      aria-hidden={decorative || undefined}
      className={
        decorative ? "h-5 w-auto shrink-0 align-middle" : "h-4 w-auto shrink-0 align-middle"
      }
    />
  )
}
