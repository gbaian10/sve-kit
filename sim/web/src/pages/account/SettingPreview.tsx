import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

import { InlineText } from "../../components/card/InlineText"
import { OriginalBox } from "../../components/card/OriginalBox"
import { cn } from "../../components/ui/cn"
import { isUiLanguage, type UiLanguage } from "../../i18n/languages"
import { FIELD_DISPLAYS, type FieldDisplay, TEXT_DISPLAYS, usePrefs } from "../../settings"
import { ORIGINAL_SAMPLE, type SettingSample, translatedSample } from "./settingSample"

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

const Text = ({
  text,
  lang,
  bold,
  labels,
}: {
  readonly text: string
  readonly lang: string
  readonly bold?: boolean
  readonly labels?: boolean
}) => (
  <p lang={lang} className="whitespace-pre-wrap">
    <InlineText text={text} emphasis bold={bold} labels={labels} />
  </p>
)

function Body({
  field,
  mode,
  translated,
  lang,
}: {
  readonly field: PreviewField
  readonly mode: FieldDisplay
  readonly translated: SettingSample
  readonly lang: string
}) {
  const { t } = useTranslation()
  if (mode === "none") return <p className="text-13 text-text-3">{t("settings.preview.hidden")}</p>
  const pick = (sample: SettingSample) => {
    if (field === "qa")
      return (
        <div className="space-y-1">
          <p>
            <span className="font-semibold">{t("qa.question")} </span>
            <InlineText text={sample.question} />
          </p>
          <p>
            <span className="font-semibold">{t("qa.answer")} </span>
            <InlineText text={sample.answer} />
          </p>
        </div>
      )
    return field === "name" ? sample.name : sample[field]
  }
  const render = (sample: SettingSample, language: string) => {
    const value = pick(sample)
    return typeof value === "string" ? <Text text={value} lang={language} /> : value
  }
  if (mode === "original")
    return (
      <div className={field === "name" ? "text-16 font-bold" : "text-14 leading-effect"}>
        {render(ORIGINAL_SAMPLE, ORIGINAL_LANG)}
      </div>
    )
  if (mode === "translated")
    return (
      <div className={field === "name" ? "text-16 font-bold" : "text-14 leading-effect"}>
        {render(translated, lang)}
      </div>
    )
  // Names show the original large with the translation under it; texts box the original.
  if (field === "name")
    return (
      <div>
        <div className="text-16 font-bold">{render(ORIGINAL_SAMPLE, ORIGINAL_LANG)}</div>
        <div className="text-14 text-text-2">{render(translated, lang)}</div>
      </div>
    )
  return (
    <div className="space-y-2 text-14 leading-effect">
      {render(translated, lang)}
      <OriginalBox>{render(ORIGINAL_SAMPLE, ORIGINAL_LANG)}</OriginalBox>
    </div>
  )
}

// The language the interface is actually showing, which decides the sample's translation.
function useSampleLanguage(): UiLanguage {
  const { i18n } = useTranslation()
  return isUiLanguage(i18n.resolvedLanguage) ? i18n.resolvedLanguage : "ja"
}

/** The three or four ways to show a card field, side by side, with the chosen one marked. */
export function FieldPreview({ field }: { readonly field: PreviewField }) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const language = useSampleLanguage()
  const translated = translatedSample(language)
  const lang = language === "zh-TW" ? "zh-Hant" : language
  const modes: readonly FieldDisplay[] =
    field === "flavor" || field === "qa" ? FIELD_DISPLAYS : TEXT_DISPLAYS
  const chosen = prefs[PREF_OF[field]]
  return (
    <ul className="grid gap-2 md:grid-cols-[repeat(auto-fit,minmax(14rem,1fr))]">
      {modes.map((mode) => (
        <Option key={mode} label={t(`settings.fieldDisplay.${mode}`)} current={chosen === mode}>
          <Body field={field} mode={mode} translated={translated} lang={lang} />
        </Option>
      ))}
    </ul>
  )
}

/** An on/off setting shown on a sample line that has the bold terms and the keyword icons. */
export function TogglePreview({ setting }: { readonly setting: PreviewToggle }) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const language = useSampleLanguage()
  const sample = language === "ja" ? ORIGINAL_SAMPLE : translatedSample(language)
  const lang = language === "zh-TW" ? "zh-Hant" : language
  return (
    <ul className="grid gap-2 md:grid-cols-[repeat(auto-fit,minmax(14rem,1fr))]">
      {([true, false] as const).map((value) => (
        <Option
          key={String(value)}
          label={value ? t("settings.preview.on") : t("settings.preview.off")}
          current={prefs[setting] === value}
        >
          <div className="text-14 leading-effect">
            <Text
              text={sample.effect}
              lang={lang}
              bold={setting === "termEmphasis" ? value : prefs.termEmphasis}
              labels={setting === "symbolLabels" ? value : prefs.symbolLabels}
            />
          </div>
        </Option>
      ))}
    </ul>
  )
}
