import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

import { InlineText } from "../../components/card/InlineText"
import { OriginalBox } from "../../components/card/OriginalBox"
import { cn } from "../../components/ui/cn"
import { isUiLanguage, type UiLanguage } from "../../i18n/languages"
import {
  CARDS_LAYOUTS,
  type CardsLayout,
  FIELD_DISPLAYS,
  type FieldDisplay,
  TEXT_DISPLAYS,
  usePrefs,
} from "../../settings"
import {
  ORIGINAL_SAMPLE,
  type Segments,
  type SettingSample,
  termSample,
  translatedSample,
} from "./settingSample"

const ORIGINAL_LANG = "ja"

export type PreviewField = "name" | "effect" | "flavor" | "qa"
export type PreviewToggle = "termEmphasis" | "symbolLabels"

const PREF_OF = {
  name: "nameDisplay",
  effect: "effectLanguage",
  flavor: "flavorDisplay",
  qa: "qaDisplay",
} as const

/** One option of a setting: the real display pieces with that option applied. */
function Option({
  label,
  current,
  children,
}: {
  readonly label: string
  readonly current: boolean
  readonly children: ReactNode
}) {
  const { t } = useTranslation()
  return (
    <li
      aria-current={current ? "true" : undefined}
      className={cn(
        "min-w-0 rounded-control border p-2",
        current ? "border-accent bg-accent-soft" : "border-border",
      )}
    >
      <p className="mb-1 flex flex-wrap items-baseline gap-x-2 text-12 font-semibold text-text-2">
        {label}
        {current && (
          <span className="font-normal text-text-3">{t("settings.preview.current")}</span>
        )}
      </p>
      {children}
    </li>
  )
}

/** Card text the way the card page lays it out: bold only where the data marks a term. */
function Text({
  text,
  lang,
  bold,
  labels,
}: {
  readonly text: string | Segments
  readonly lang: string
  readonly bold: boolean
  readonly labels?: boolean
}) {
  const segments: Segments = typeof text === "string" ? [[text, false]] : text
  return (
    <p lang={lang} className="whitespace-pre-wrap">
      {segments.map(([part, term], index) => {
        const content = <InlineText text={part} labels={labels} />
        return term && bold ? (
          <strong key={index}>{content}</strong>
        ) : (
          <span key={index}>{content}</span>
        )
      })}
    </p>
  )
}

function Body({
  field,
  mode,
  translated,
  lang,
}: {
  readonly field: PreviewField
  readonly mode: FieldDisplay
  /** Null when the card has no translation in this interface language: the original stands in. */
  readonly translated: SettingSample | null
  readonly lang: string
}) {
  const { t } = useTranslation()
  const { termEmphasis } = usePrefs()
  if (mode === "none") return <p className="text-13 text-text-3">{t("settings.preview.hidden")}</p>
  const render = (sample: SettingSample, language: string) => {
    if (field === "qa")
      return (
        <div className="space-y-1">
          <p lang={language}>
            <span className="font-semibold">{t("qa.question")} </span>
            <InlineText text={sample.question} />
          </p>
          <p lang={language}>
            <span className="font-semibold">{t("qa.answer")} </span>
            <InlineText text={sample.answer} />
          </p>
        </div>
      )
    return <Text text={sample[field]} lang={language} bold={termEmphasis} />
  }
  const big = field === "name" ? "text-16 font-bold" : "text-14 leading-effect"
  const original = render(ORIGINAL_SAMPLE, ORIGINAL_LANG)
  // Without a translation the card page shows the original and says so (except in Japanese).
  if (!translated)
    return (
      <div className={cn(big, "space-y-1")}>
        {original}
        {mode !== "original" && lang !== ORIGINAL_LANG && (
          <p className="text-13 text-text-2">{t("card.translationMissing")}</p>
        )}
      </div>
    )
  if (mode === "original") return <div className={big}>{original}</div>
  if (mode === "translated") return <div className={big}>{render(translated, lang)}</div>
  // Names show the original large with the translation under it; texts box the original.
  if (field === "name")
    return (
      <div>
        <div className="text-16 font-bold">{original}</div>
        <div className="text-14 text-text-2">{render(translated, lang)}</div>
      </div>
    )
  return (
    <div className="space-y-2 text-14 leading-effect">
      {render(translated, lang)}
      <OriginalBox>{original}</OriginalBox>
    </div>
  )
}

// The language the interface is actually showing, which decides the sample's translation.
function useSampleLanguage(): UiLanguage {
  const { i18n } = useTranslation()
  return isUiLanguage(i18n.resolvedLanguage) ? i18n.resolvedLanguage : "ja"
}

const langOf = (language: UiLanguage) => (language === "zh-TW" ? "zh-Hant" : language)

/** The three or four ways to show a card field, side by side, with the chosen one marked. */
export function FieldPreview({ field }: { readonly field: PreviewField }) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const language = useSampleLanguage()
  const translated = translatedSample(language)
  const modes: readonly FieldDisplay[] =
    field === "flavor" || field === "qa" ? FIELD_DISPLAYS : TEXT_DISPLAYS
  const chosen = prefs[PREF_OF[field]]
  return (
    <ul className="grid gap-2 md:grid-cols-[repeat(auto-fit,minmax(14rem,1fr))]">
      {modes.map((mode) => (
        <Option key={mode} label={t(`settings.fieldDisplay.${mode}`)} current={chosen === mode}>
          <Body field={field} mode={mode} translated={translated} lang={langOf(language)} />
        </Option>
      ))}
    </ul>
  )
}

/** An on/off setting shown on a card line that has terms mid-sentence and a keyword icon. */
export function TogglePreview({ setting }: { readonly setting: PreviewToggle }) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const language = useSampleLanguage()
  const sample = termSample(language)
  const lang = language === "zh-TW" ? "zh-Hant" : ORIGINAL_LANG
  return (
    <div className="space-y-2">
      {setting === "termEmphasis" && (
        <p className="text-13 text-text-2">{t("settings.preview.termScope")}</p>
      )}
      <ul className="grid gap-2 md:grid-cols-2">
        {([true, false] as const).map((value) => (
          <Option
            key={String(value)}
            label={value ? t("settings.preview.on") : t("settings.preview.off")}
            current={prefs[setting] === value}
          >
            <div className="text-14 leading-effect">
              <Text
                text={sample}
                lang={lang}
                bold={setting === "termEmphasis" ? value : prefs.termEmphasis}
                labels={setting === "symbolLabels" ? value : prefs.symbolLabels}
              />
            </div>
          </Option>
        ))}
      </ul>
    </div>
  )
}

const LAYOUT_PANES: Readonly<Record<CardsLayout, readonly ("filters" | "results" | "card")[]>> = {
  cardLeft: ["card", "results", "filters"],
  filterLeft: ["filters", "results", "card"],
}
const PANE_WIDTH = { filters: "flex-[2]", results: "flex-[3]", card: "flex-[3]" } as const

/** Both arrangements of the cards page at desktop width as little maps, the chosen one marked. */
export function LayoutPreview() {
  const { t } = useTranslation()
  const { cardsLayout } = usePrefs()
  return (
    <ul className="grid gap-2 md:grid-cols-2">
      {CARDS_LAYOUTS.map((layout) => (
        <Option
          key={layout}
          label={t(`options.cardsLayout.${layout}`)}
          current={cardsLayout === layout}
        >
          <div className="flex h-16 gap-1" aria-hidden="true">
            {LAYOUT_PANES[layout].map((pane) => (
              <div
                key={pane}
                className={cn(
                  "flex min-w-0 items-center justify-center rounded-control border border-border bg-surface-2 text-11 text-text-2",
                  PANE_WIDTH[pane],
                )}
              >
                <span className="truncate px-1">{t(`settings.preview.layout.${pane}`)}</span>
              </div>
            ))}
          </div>
        </Option>
      ))}
    </ul>
  )
}
