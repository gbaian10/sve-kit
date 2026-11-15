import { Settings as SettingsIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"

import { Dialog } from "../../../components/ui/Dialog"
import { Segmented } from "../../../components/ui/Segmented"
import { SettingRow } from "../../../components/ui/SettingRow"
import { BATTLE_MATS, prefsStore, usePrefs } from "../../../settings"

/** The gear and the battle settings it opens. */
export function SettingsButton() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const [open, setOpen] = useState(false)
  return (
    <>
      <button
        type="button"
        className="bp-gear"
        onClick={() => {
          setOpen(true)
        }}
        aria-label={t("play.settings.title")}
        title={t("play.settings.title")}
      >
        <SettingsIcon aria-hidden="true" />
      </button>
      <Dialog
        open={open}
        onClose={() => {
          setOpen(false)
        }}
        title={t("play.settings.title")}
        variant="center"
      >
        <div className="p-4">
          <SettingRow label={t("play.settings.mat")} hint={t("play.settings.matHint")}>
            <Segmented
              label={t("play.settings.mat")}
              options={BATTLE_MATS.map((value) => ({
                value,
                label: t(`options.battleMat.${value}`),
              }))}
              value={prefs.battleMat}
              onChange={(battleMat) => {
                prefsStore.set({ battleMat })
              }}
            />
          </SettingRow>
        </div>
      </Dialog>
    </>
  )
}
