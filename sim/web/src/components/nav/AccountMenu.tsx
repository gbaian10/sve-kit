import { Check, UserRound } from "lucide-react"
import { type KeyboardEvent, useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { useNavigate } from "react-router"

import { useSystemDark } from "../../app/useSystemDark"
import { type Accent, ACCENTS, resolveTheme, THEME_PREFS } from "../../domain/theme"
import { currentUiLanguage, UI_LANGUAGES } from "../../i18n"
import { CARD_EDITIONS, prefsStore, TEXT_DISPLAYS, usePrefs } from "../../settings"
import { cn } from "../ui/cn"
import { Segmented } from "../ui/Segmented"
import { SettingRow } from "../ui/SettingRow"
import { rovingRadioKeyDown } from "../ui/useRovingRadio"

const ACCENT_SWATCH: Record<Accent, string> = {
  amber: "bg-class-sword",
  teal: "bg-success",
  red: "bg-danger",
}

/** Three colour radios; the checked one is what the page actually shows, even before a choice. */
export function AccentPicker({ label }: { readonly label: string }) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const systemDark = useSystemDark()
  const id = useId()
  const { accent } = resolveTheme({ theme: prefs.theme, accent: prefs.accent }, systemDark)
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    rovingRadioKeyDown(event, ACCENTS, accent, (next) => prefsStore.set({ accent: next }), id)
  }
  return (
    <div role="radiogroup" aria-label={label} className="flex">
      {ACCENTS.map((value) => {
        const checked = value === accent
        return (
          <button
            key={value}
            id={`${id}-${value}`}
            type="button"
            role="radio"
            aria-checked={checked}
            aria-label={t(`options.accent.${value}`)}
            tabIndex={checked ? 0 : -1}
            onKeyDown={onKeyDown}
            onClick={() => prefsStore.set({ accent: value })}
            className="flex size-11 items-center justify-center"
          >
            <span
              aria-hidden="true"
              className={cn(
                "flex size-8 items-center justify-center rounded-full",
                ACCENT_SWATCH[value],
                checked && "ring-2 ring-text-1 ring-offset-2 ring-offset-surface-1",
              )}
            >
              {checked && <Check className="size-4 text-accent-ink" />}
            </span>
          </button>
        )
      })}
    </div>
  )
}

/** The menu body; also rendered on its own in tests, where the Popover API is unavailable. */
export function AccountMenuPanel({ onNavigate }: { readonly onNavigate?: () => void }) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const navigate = useNavigate()
  const textDisplayOptions = TEXT_DISPLAYS.map((value) => ({
    value,
    label: t(`options.nameDisplay.${value}`),
  }))
  return (
    <div className="flex w-80 max-w-[calc(100vw-2rem)] flex-col gap-1 p-4">
      <p className="flex items-center gap-2 text-13 text-text-3">
        <UserRound className="size-4" aria-hidden="true" />
        {t("account.notSignedIn")}
      </p>
      <SettingRow size="sm" label={t("account.cardEdition")}>
        <Segmented
          size="sm"
          label={t("account.cardEdition")}
          options={CARD_EDITIONS.map((value) => ({ value, label: t(`options.edition.${value}`) }))}
          value={prefs.cardEdition}
          onChange={(cardEdition) => prefsStore.set({ cardEdition })}
        />
      </SettingRow>
      <SettingRow size="sm" label={t("account.nameDisplay")}>
        <Segmented
          size="sm"
          label={t("account.nameDisplay")}
          options={textDisplayOptions}
          value={prefs.nameDisplay}
          onChange={(nameDisplay) => prefsStore.set({ nameDisplay })}
        />
      </SettingRow>
      <SettingRow size="sm" label={t("settings.effectLanguage")}>
        <Segmented
          size="sm"
          label={t("settings.effectLanguage")}
          options={textDisplayOptions}
          value={prefs.effectLanguage}
          onChange={(effectLanguage) => prefsStore.set({ effectLanguage })}
        />
      </SettingRow>
      <SettingRow size="sm" label={t("account.uiLanguage")}>
        <Segmented
          size="sm"
          label={t("account.uiLanguage")}
          options={UI_LANGUAGES.map((value) => ({ value, label: t(`options.language.${value}`) }))}
          value={currentUiLanguage(prefs.uiLanguage)}
          onChange={(uiLanguage) => prefsStore.set({ uiLanguage })}
        />
      </SettingRow>
      <SettingRow size="sm" label={t("account.appearance")}>
        <Segmented
          size="sm"
          label={t("account.appearance")}
          options={THEME_PREFS.map((value) => ({ value, label: t(`options.theme.${value}`) }))}
          value={prefs.theme}
          onChange={(theme) => prefsStore.set({ theme })}
        />
      </SettingRow>
      <SettingRow size="sm" label={t("account.accent")}>
        <AccentPicker label={t("account.accent")} />
      </SettingRow>
      <button
        type="button"
        onClick={() => {
          onNavigate?.()
          void navigate("/settings")
        }}
        className="mt-1 flex h-11 items-center rounded-button px-2 text-14 font-semibold text-accent-text hover:bg-surface-2"
      >
        {t("account.allSettings")}
      </button>
    </div>
  )
}

const POPOVER_SUPPORTED = typeof HTMLElement !== "undefined" && "popover" in HTMLElement.prototype

// Avatar = menu entry (design 03d), always at the right end of the top bar (the design's rail-bottom
// placement on desktop was unified with the other layouts, user 2026-09-29). Not signed in: dashed
// outline person. Uses the Popover API for open/close and light dismiss; browsers without it
// (Safari < 17) get a plain toggle instead.
export function AccountMenuButton() {
  const { t } = useTranslation()
  const id = useId()
  const panelId = `${id}-account`
  const [fallbackOpen, setFallbackOpen] = useState(false)
  const close = () => {
    if (POPOVER_SUPPORTED) document.getElementById(panelId)?.hidePopover()
    else setFallbackOpen(false)
  }
  return (
    <>
      <button
        type="button"
        popoverTarget={POPOVER_SUPPORTED ? panelId : undefined}
        aria-controls={panelId}
        aria-expanded={POPOVER_SUPPORTED ? undefined : fallbackOpen}
        onClick={
          POPOVER_SUPPORTED
            ? undefined
            : () => {
                setFallbackOpen((open) => !open)
              }
        }
        aria-label={t("nav.account")}
        className="flex size-11 items-center justify-center rounded-full text-text-2 hover:bg-surface-2"
      >
        <span className="flex size-8 items-center justify-center rounded-full border-2 border-dashed border-border-strong">
          <UserRound className="size-4" aria-hidden="true" />
        </span>
      </button>
      <div
        id={panelId}
        popover={POPOVER_SUPPORTED ? "auto" : undefined}
        hidden={POPOVER_SUPPORTED ? undefined : !fallbackOpen}
        className={cn(
          "m-0 rounded-block border border-border bg-surface-1 p-0 text-text-1 shadow-lg",
          POPOVER_SUPPORTED ? "inset-auto" : "fixed z-50",
          "top-14 right-4 lg:right-6",
        )}
      >
        <AccountMenuPanel onNavigate={close} />
      </div>
    </>
  )
}
