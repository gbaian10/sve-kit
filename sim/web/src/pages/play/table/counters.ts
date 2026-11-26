import { useTranslation } from "react-i18next"

// The authority supplies the Japanese rules name when local wording is unavailable.
const COUNTERS = {
  スタックカウンター: "stack",
  融合カウンター: "fusion",
  返戻カウンター: "refund",
} as const

type CounterKey = (typeof COUNTERS)[keyof typeof COUNTERS]

/** Null for a rules name the screen has no wording for yet. */
function counterKey(name: string): CounterKey | null {
  return Object.hasOwn(COUNTERS, name) ? COUNTERS[name as keyof typeof COUNTERS] : null
}

/** Display the localized counter name when available and its current amount. */
export function useCounterText(): (counter: {
  readonly name: string
  readonly amount: number
}) => string {
  const { t } = useTranslation()
  return ({ name, amount }) => {
    const key = counterKey(name)
    return t("play.counter.count", {
      name: key === null ? name : t(`play.counter.names.${key}`),
      n: amount,
    })
  }
}
