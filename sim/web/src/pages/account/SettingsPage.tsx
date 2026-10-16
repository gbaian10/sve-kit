import { useTranslation } from "react-i18next"

import { AccentPicker } from "../../components/nav/AccountMenu"
import { Segmented } from "../../components/ui/Segmented"
import { SettingRow } from "../../components/ui/SettingRow"
import { Switch } from "../../components/ui/Switch"
import { THEME_PREFS } from "../../domain/theme"
import { currentUiLanguage, UI_LANGUAGES } from "../../i18n"
import { CARD_EDITIONS, prefsStore, TEXT_DISPLAYS, usePrefs } from "../../settings"
import { PageTitle } from "../PageTitle"
import { StorageSettings } from "./StorageSettings"

const ROW = "min-h-14 border-b border-border py-2"

// R1 has no sign-in: settings stay local to this browser.
export function SettingsPage() {
  const { t } = useTranslation()
  const prefs = usePrefs()
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
              label: t(`options.nameDisplay.${value}`),
            }))}
            value={prefs.nameDisplay}
            onChange={(nameDisplay) => prefsStore.set({ nameDisplay })}
          />
        </SettingRow>
        <SettingRow className={ROW} label={t("settings.effectLanguage")}>
          <Segmented
            size="sm"
            label={t("settings.effectLanguage")}
            options={TEXT_DISPLAYS.map((value) => ({
              value,
              label: t(`options.nameDisplay.${value}`),
            }))}
            value={prefs.effectLanguage}
            onChange={(effectLanguage) => prefsStore.set({ effectLanguage })}
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
          label={t("settings.prefetchPrintings")}
          hint={t("settings.prefetchPrintingsHint")}
        >
          <Switch
            label={t("settings.prefetchPrintings")}
            checked={prefs.prefetchPrintings}
            onChange={(prefetchPrintings) => prefsStore.set({ prefetchPrintings })}
          />
        </SettingRow>
        <SettingRow
          className={ROW}
          label={t("settings.prefetchNeighbours")}
          hint={t("settings.prefetchNeighboursHint")}
        >
          <Switch
            label={t("settings.prefetchNeighbours")}
            checked={prefs.prefetchNeighbours}
            onChange={(prefetchNeighbours) => prefsStore.set({ prefetchNeighbours })}
          />
        </SettingRow>
      </section>
      <StorageSettings />
    </>
  )
}
