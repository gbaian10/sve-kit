import { useState } from "react"
import { useTranslation } from "react-i18next"

import { AccentPicker } from "../../components/nav/AccountMenu"
import { Button } from "../../components/ui/Button"
import { Dialog } from "../../components/ui/Dialog"
import { Segmented } from "../../components/ui/Segmented"
import { SettingRow } from "../../components/ui/SettingRow"
import { Switch } from "../../components/ui/Switch"
import { THEME_PREFS } from "../../domain/theme"
import { currentUiLanguage, UI_LANGUAGES } from "../../i18n"
import {
  CARD_EDITIONS,
  CARD_OPEN_MODES,
  DEFAULT_PREFS,
  FIELD_DISPLAYS,
  prefsStore,
  TEXT_DISPLAYS,
  usePrefs,
} from "../../settings"
import { PageTitle } from "../PageTitle"
import { DownloadSettings } from "./DownloadSettings"
import { StorageSettings } from "./StorageSettings"

const ROW = "min-h-14 border-b border-border py-2"

// R1 has no sign-in: settings stay local to this browser.
export function SettingsPage() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const [resetting, setResetting] = useState(false)
  return (
    <>
      <PageTitle>{t("settings.title")}</PageTitle>
      <section aria-labelledby="settings-display" className="rounded-block bg-surface-1 px-4">
        <h2 id="settings-display" className="pt-3 text-13 font-semibold text-text-3">
          {t("settings.display")}
        </h2>
        <SettingRow className={ROW} label={t("account.uiLanguage")}>
          <Segmented
            size="sm"
            label={t("account.uiLanguage")}
            options={UI_LANGUAGES.map((value) => ({
              value,
              label: t(`options.language.${value}`),
            }))}
            value={currentUiLanguage(prefs.uiLanguage)}
            onChange={(uiLanguage) => prefsStore.set({ uiLanguage })}
          />
        </SettingRow>
        <SettingRow className={ROW} label={t("account.cardEdition")}>
          <Segmented
            size="sm"
            label={t("account.cardEdition")}
            options={CARD_EDITIONS.map((value) => ({
              value,
              label: t(`options.edition.${value}`),
            }))}
            value={prefs.cardEdition}
            onChange={(cardEdition) => prefsStore.set({ cardEdition })}
          />
        </SettingRow>
        <SettingRow className={ROW} label={t("account.nameDisplay")}>
          <Segmented
            size="sm"
            label={t("account.nameDisplay")}
            options={TEXT_DISPLAYS.map((value) => ({
              value,
              label: t(`settings.fieldDisplay.${value}`),
            }))}
            value={prefs.nameDisplay}
            onChange={(nameDisplay) => prefsStore.set({ nameDisplay })}
          />
        </SettingRow>
        <SettingRow className={ROW} label={t("settings.effectLanguage")}>
          <Segmented
            size="sm"
            label={t("settings.effectLanguage")}
            options={FIELD_DISPLAYS.map((value) => ({
              value,
              label: t(`settings.fieldDisplay.${value}`),
            }))}
            value={prefs.effectLanguage}
            onChange={(effectLanguage) => prefsStore.set({ effectLanguage })}
          />
        </SettingRow>
        <SettingRow className={ROW} label={t("settings.flavorDisplay")}>
          <Segmented
            size="sm"
            label={t("settings.flavorDisplay")}
            options={FIELD_DISPLAYS.map((value) => ({
              value,
              label: t(`settings.fieldDisplay.${value}`),
            }))}
            value={prefs.flavorDisplay}
            onChange={(flavorDisplay) => prefsStore.set({ flavorDisplay })}
          />
        </SettingRow>
        <SettingRow className={ROW} label={t("settings.qaDisplay")}>
          <Segmented
            size="sm"
            label={t("settings.qaDisplay")}
            options={FIELD_DISPLAYS.map((value) => ({
              value,
              label: t(`settings.fieldDisplay.${value}`),
            }))}
            value={prefs.qaDisplay}
            onChange={(qaDisplay) => prefsStore.set({ qaDisplay })}
          />
        </SettingRow>
        <SettingRow
          className={ROW}
          label={t("settings.termEmphasis")}
          hint={t("settings.termEmphasisHint")}
        >
          <Switch
            label={t("settings.termEmphasis")}
            checked={prefs.termEmphasis}
            onChange={(termEmphasis) => prefsStore.set({ termEmphasis })}
          />
        </SettingRow>
        <SettingRow
          className={ROW}
          label={t("settings.symbolLabels")}
          hint={t("settings.symbolLabelsHint")}
        >
          <Switch
            label={t("settings.symbolLabels")}
            checked={prefs.symbolLabels}
            onChange={(symbolLabels) => prefsStore.set({ symbolLabels })}
          />
        </SettingRow>
        <SettingRow
          className={ROW}
          label={t("settings.cardOpenMode")}
          hint={t("settings.cardOpenModeHint")}
        >
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
        <SettingRow className={ROW} label={t("account.appearance")}>
          <Segmented
            size="sm"
            label={t("account.appearance")}
            options={THEME_PREFS.map((value) => ({ value, label: t(`options.theme.${value}`) }))}
            value={prefs.theme}
            onChange={(theme) => prefsStore.set({ theme })}
          />
        </SettingRow>
        <SettingRow className={ROW} label={t("account.accent")}>
          <AccentPicker label={t("account.accent")} />
        </SettingRow>
        <SettingRow className={ROW} label={t("settings.banRegion")}>
          <Segmented
            size="sm"
            label={t("settings.banRegion")}
            options={CARD_EDITIONS.map((value) => ({ value, label: t(`options.region.${value}`) }))}
            value={prefs.banRegion}
            onChange={(banRegion) => prefsStore.set({ banRegion })}
          />
        </SettingRow>
      </section>
      <section aria-labelledby="settings-advanced" className="mt-6 rounded-block bg-surface-1 px-4">
        <h2 id="settings-advanced" className="pt-3 text-13 font-semibold text-text-3">
          {t("settings.advanced")}
        </h2>
        <SettingRow
          className={ROW}
          label={t("settings.infiniteScroll")}
          hint={t("settings.infiniteScrollHint")}
        >
          <Switch
            label={t("settings.infiniteScroll")}
            checked={prefs.infiniteScroll}
            onChange={(infiniteScroll) => prefsStore.set({ infiniteScroll })}
          />
        </SettingRow>
        <SettingRow
          className={ROW}
          label={t("settings.preloadPrintings")}
          hint={t("settings.preloadPrintingsHint")}
        >
          <Switch
            label={t("settings.preloadPrintings")}
            checked={prefs.preloadPrintings}
            onChange={(preloadPrintings) => prefsStore.set({ preloadPrintings })}
          />
        </SettingRow>
        <SettingRow
          className={ROW}
          label={t("settings.preloadNeighbours")}
          hint={t("settings.preloadNeighboursHint")}
        >
          <Switch
            label={t("settings.preloadNeighbours")}
            checked={prefs.preloadNeighbours}
            onChange={(preloadNeighbours) => prefsStore.set({ preloadNeighbours })}
          />
        </SettingRow>
      </section>
      <DownloadSettings />
      <div className="mt-4">
        <Button
          onClick={() => {
            setResetting(true)
          }}
        >
          {t("settings.resetDefaults")}
        </Button>
      </div>
      <Dialog
        open={resetting}
        onClose={() => {
          setResetting(false)
        }}
        title={t("settings.resetTitle")}
        variant="center"
        footer={
          <div className="flex justify-end gap-2">
            <Button
              onClick={() => {
                setResetting(false)
              }}
            >
              {t("settings.resetCancel")}
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                prefsStore.set(DEFAULT_PREFS)
                setResetting(false)
              }}
            >
              {t("settings.resetDefaults")}
            </Button>
          </div>
        }
      >
        <p className="text-14 text-text-2">{t("settings.resetHint")}</p>
      </Dialog>
      <StorageSettings />
    </>
  )
}
