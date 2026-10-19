import { useTranslation } from "react-i18next"

import advance from "../../assets/card-labels/advance.svg"
import evolve from "../../assets/card-labels/evolve.svg"
import leader from "../../assets/card-labels/leader.svg"
import token from "../../assets/card-labels/token.svg"
import type { BadgeLocation } from "../../domain/deckLocation"

const ASSETS = { evolve, advance, leader, token }

export function DeckLocationBadges({
  locations = [],
  decorative = false,
}: {
  readonly locations?: readonly BadgeLocation[] | undefined
  readonly decorative?: boolean
}) {
  const { t } = useTranslation()
  if (locations.length === 0) return null
  return (
    <span
      data-location-badges=""
      className="inline-flex max-w-full flex-wrap items-center gap-1 align-middle"
    >
      {locations.map((location) => (
        <img
          key={location}
          src={ASSETS[location]}
          alt={decorative ? "" : t(`locationFilter.${location}`)}
          title={decorative ? undefined : t(`locationFilter.${location}`)}
          aria-hidden={decorative || undefined}
          className={
            location === "leader"
              ? decorative
                ? "h-7 w-auto shrink-0"
                : "h-6 w-auto shrink-0"
              : decorative
                ? "h-5 w-auto shrink-0"
                : "h-4 w-auto shrink-0"
          }
        />
      ))}
    </span>
  )
}
