import { Check, UserRound } from "lucide-react"
import { type KeyboardEvent, useEffect, useId, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { useLocation, useNavigate } from "react-router"

import { routeState } from "../../app/listEntryState"
import { useRouteEdition } from "../../app/snapshot"
import { useSystemDark } from "../../app/useSystemDark"
import { locateEdition, snapshotClient, useSnapshotRoot } from "../../data"
import { CardRouteError, printingPath } from "../../domain/cardRoute"
import type { Region } from "../../domain/search"
import { type Accent, ACCENTS, resolveTheme, THEME_PREFS } from "../../domain/theme"
import { currentUiLanguage, UI_LANGUAGES } from "../../i18n"
import {
  CARD_EDITIONS,
  CARD_OPEN_MODES,
  cardTranslation,
  cardTranslationPatch,
  prefsStore,
  showsOtherEdition,
  TEXT_DISPLAYS,
  usePrefs,
} from "../../settings"
import { cn } from "../ui/cn"
import { Segmented } from "../ui/Segmented"
import { SettingRow } from "../ui/SettingRow"
import { Switch } from "../ui/Switch"
import { useToast } from "../ui/toast-context"
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
  const location = useLocation()
  const edition = useRouteEdition()
  const root = useSnapshotRoot()
  const toast = useToast()
  const [transition, setTransition] = useState<{
    key: string
    edition: Region
    state: "loading" | "error"
  } | null>(null)
  const switchKey = `${root}/${location.key}`
  const currentTransition = transition?.key === switchKey ? transition : null
  const pending = useRef<AbortController | null>(null)
  useEffect(
    () => () => {
      pending.current?.abort()
    },
    [location.key, root],
  )
  const changeEdition = async (cardEdition: Region) => {
    pending.current?.abort()
    setTransition(null)
    if (!location.pathname.startsWith("/cards/")) {
      prefsStore.set({ cardEdition })
      if (
        location.pathname === "/cards" ||
        location.pathname === "/sets" ||
        location.pathname.startsWith("/sets/")
      ) {
        const params = new URLSearchParams(location.search)
        if (cardEdition === "en") params.set("ed", "en")
        else params.delete("ed")
        void navigate({ pathname: location.pathname, search: params.toString() })
      }
      return
    }
    if (cardEdition === edition) {
      setTransition(null)
      return
    }
    const abort = new AbortController()
    pending.current = abort
    setTransition({ key: switchKey, edition: cardEdition, state: "loading" })
    try {
      const client = snapshotClient(root)
      const segments = location.pathname.split("/")
      const provisional = segments[2] === "_provisional"
      const target = await locateEdition(
        client,
        {
          namespace: provisional ? "provisional" : "official",
          key: decodeURIComponent(segments[provisional ? 3 : 2] ?? ""),
          edition,
        },
        cardEdition,
        abort.signal,
      )
      abort.signal.throwIfAborted()
      setTransition(null)
      prefsStore.set({ cardEdition })
      if (!target) {
        void navigate(cardEdition === "en" ? "/cards?ed=en" : "/cards")
        toast.show({ message: t(`cardPage.notCollected.${cardEdition}`) })
      } else {
        void navigate(printingPath(target), { state: routeState(target) })
      }
    } catch (error) {
      if (abort.signal.aborted) return
      // A URL that names no card has no card to keep; edition browsing starts at its list.
      if (
        error instanceof CardRouteError &&
        (error.kind === "not-found" || error.kind === "invalid")
      ) {
        setTransition(null)
        prefsStore.set({ cardEdition })
        void navigate(cardEdition === "en" ? "/cards?ed=en" : "/cards")
        return
      }
      setTransition({ key: switchKey, edition: cardEdition, state: "error" })
    }
  }
  const translation = cardTranslation(prefs)
  const textDisplayOptions = TEXT_DISPLAYS.map((value) => ({
    value,
    label: t(`options.textDisplay.${value}`),
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
          value={edition}
          onChange={(cardEdition) => {
            void changeEdition(cardEdition)
          }}
        />
      </SettingRow>
      {currentTransition?.state === "loading" && (
        <p role="status" className="text-13 text-text-2">
          {t("cardPage.switchingEdition")}
        </p>
      )}
      {currentTransition?.state === "error" && (
        <div role="alert" className="text-13 text-text-2">
          {t("cardPage.otherEditionFailed")}
          <button
            type="button"
            className="h-11 px-2 text-accent-text"
            onClick={() => {
              void changeEdition(currentTransition.edition)
            }}
          >
            {t("status.retry")}
          </button>
        </div>
      )}
      <SettingRow size="sm" label={t(`account.showOtherEdition.${edition}`)}>
        <Switch
          size="sm"
          label={t(`account.showOtherEdition.${edition}`)}
          checked={showsOtherEdition(prefs, edition)}
          onChange={(showOtherEdition) =>
            prefsStore.set({
              showOtherEdition: { ...prefs.showOtherEdition, [edition]: showOtherEdition },
            })
          }
        />
      </SettingRow>
      <SettingRow size="sm" label={t("account.cardTranslation")}>
        {translation === "custom" && (
          <span className="me-2 text-12 text-text-3">{t("account.cardTranslationCustom")}</span>
        )}
        <Segmented
          size="sm"
          label={t("account.cardTranslation")}
          options={textDisplayOptions}
          value={translation === "custom" ? null : translation}
          onChange={(value) => prefsStore.set(cardTranslationPatch(value))}
        />
      </SettingRow>
      <SettingRow size="sm" label={t("settings.cardOpenMode")}>
        <Segmented
          size="sm"
          label={t("settings.cardOpenMode")}
          options={CARD_OPEN_MODES.map((value) => ({
            value,
            label: t(`options.cardOpenMode.${value}`),
          }))}
          value={prefs.cardOpenMode}
          onChange={(cardOpenMode) => prefsStore.set({ cardOpenMode })}
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

// Avatar = menu entry (design 03d), always at the right end of the top bar (the design's rail-bottom
// placement on desktop was unified with the other layouts, user 2026-09-29). Not signed in: dashed
// outline person. Uses the Popover API for open/close and light dismiss; browsers without it
// (Safari < 17) get a plain toggle instead.
