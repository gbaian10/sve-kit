import { type ReactNode, useState } from "react"
import { useTranslation } from "react-i18next"

import { AccentPicker } from "../../components/nav/AccountMenu"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import { Dialog } from "../../components/ui/Dialog"
import { HelpTip } from "../../components/ui/HelpTip"
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
  type FieldDisplay,
  prefsStore,
  TEXT_DISPLAYS,
  usePrefs,
} from "../../settings"
import { PageTitle } from "../PageTitle"
import { DownloadSettings } from "./DownloadSettings"
import {
  FieldPreview,
  type PreviewField,
  type PreviewToggle,
  TogglePreview,
} from "./SettingPreview"
import { StorageSettings } from "./StorageSettings"

const ROW = "min-h-14 border-b border-border py-2"

function FieldRow({
  field,
  label,
  pref,
  options,
}: {
  readonly field: PreviewField
  readonly label: string
  readonly pref: "nameDisplay" | "effectLanguage" | "flavorDisplay" | "qaDisplay"
  readonly options: readonly FieldDisplay[]
}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  return (
    <SettingRow
      className={ROW}
      label={label}
      help={
        <HelpTip label={t("settings.preview.help", { name: label })}>
          <FieldPreview field={field} />
        </HelpTip>
      }
    >
      <Segmented
        size="sm"
        label={label}
        options={options.map((value) => ({ value, label: t(`settings.fieldDisplay.${value}`) }))}
        value={prefs[pref]}
        onChange={(value) => prefsStore.set({ [pref]: value })}
      />
    </SettingRow>
  )
}

function ToggleRow({
  setting,
  label,
  hint,
}: {
  readonly setting: PreviewToggle
  readonly label: string
  readonly hint: string
}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  return (
    <SettingRow
      className={ROW}
      label={label}
      hint={hint}
      help={
        <HelpTip label={t("settings.preview.help", { name: label })}>
          <TogglePreview setting={setting} />
        </HelpTip>
      }
    >
      <Switch
        label={label}
        checked={prefs[setting]}
        onChange={(value) => prefsStore.set({ [setting]: value })}
      />
    </SettingRow>
  )
}

// One settings group per purpose, each with its own heading; downloads and storage bring theirs.
function Group({
  id,
  title,
  first = false,
  children,
}: {
  readonly id: string
  readonly title: string
  readonly first?: boolean
  readonly children: ReactNode
}) {
  return (
    <section
      aria-labelledby={`settings-${id}`}
      className={cn("rounded-block bg-surface-1 px-4", !first && "mt-6")}
    >
      <h2 id={`settings-${id}`} className="pt-3 text-13 font-semibold text-text-3">
        {title}
      </h2>
      {children}
    </section>
  )
}

// R1 has no sign-in: settings stay local to this browser.
export function SettingsPage() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const [resetting, setResetting] = useState(false)
  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-x-4">
        <PageTitle>{t("settings.title")}</PageTitle>
        <Button
          onClick={() => {
            setResetting(true)
          }}
        >
          {t("settings.resetDefaults")}
        </Button>
      </div>
      <Group id="interface" title={t("settings.groups.interface")} first>
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
      </Group>
      <Group id="data" title={t("settings.groups.data")}>
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
        <SettingRow className={ROW} label={t("settings.banRegion")}>
          <Segmented
            size="sm"
            label={t("settings.banRegion")}
            options={CARD_EDITIONS.map((value) => ({ value, label: t(`options.region.${value}`) }))}
            value={prefs.banRegion}
            onChange={(banRegion) => prefsStore.set({ banRegion })}
          />
        </SettingRow>
      </Group>
      <Group id="text" title={t("settings.groups.text")}>
        <FieldRow
          field="name"
          label={t("account.nameDisplay")}
          pref="nameDisplay"
          options={TEXT_DISPLAYS}
        />
        <FieldRow
          field="effect"
          label={t("settings.effectLanguage")}
          pref="effectLanguage"
          options={TEXT_DISPLAYS}
        />
        <FieldRow
          field="flavor"
          label={t("settings.flavorDisplay")}
          pref="flavorDisplay"
          options={FIELD_DISPLAYS}
        />
        <FieldRow
          field="qa"
          label={t("settings.qaDisplay")}
          pref="qaDisplay"
          options={FIELD_DISPLAYS}
        />
      </Group>
      <Group id="style" title={t("settings.groups.style")}>
        <ToggleRow
          setting="termEmphasis"
          label={t("settings.termEmphasis")}
          hint={t("settings.termEmphasisHint")}
        />
        <ToggleRow
          setting="symbolLabels"
          label={t("settings.symbolLabels")}
          hint={t("settings.symbolLabelsHint")}
        />
      </Group>
      <Group id="browse" title={t("settings.groups.browse")}>
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
      </Group>
      <Group id="preload" title={t("settings.groups.preload")}>
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
      </Group>
      <DownloadSettings />
      <StorageSettings />
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
                // Background downloads are independent of the other preferences, so a reset keeps them.
                prefsStore.set({
                  ...DEFAULT_PREFS,
                  backgroundDownloads: prefsStore.get().backgroundDownloads,
                })
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
    </>
  )
}
