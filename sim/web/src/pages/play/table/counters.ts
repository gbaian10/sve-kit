import { useTranslation } from "react-i18next"

// Counters as the authority names them: the rules names of the keyword registry's counter entries
// (authored/rules/keywords.yaml). The screen names a counter's kind in the reader's language.
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

/** A counter in words, kind and amount; a kind without a name here is only counted. */
export function useCounterText(): (counter: {
  readonly name: string
  readonly amount: number
}) => string {
  const { t } = useTranslation()
  return ({ name, amount }) => {
    const key = counterKey(name)
    return key === null
      ? t("play.counter.unnamed", { n: amount })
      : t("play.counter.count", { name: t(`play.counter.names.${key}`), n: amount })
  }
}
