import { useEffect, useSyncExternalStore } from "react"
import { useTranslation } from "react-i18next"

import { SettingRow } from "../../components/ui/SettingRow"
import { Switch } from "../../components/ui/Switch"
import { appBackgroundDownloads } from "../../data"
import { formatDataVersion } from "../../domain/dataVersion"
import { prefsStore, usePrefs } from "../../settings"

export function DownloadSettings() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const status = useSyncExternalStore(
    appBackgroundDownloads.subscribe,
    appBackgroundDownloads.getStatus,
  )
  const { progress, selection } = status
  const state = !prefs.backgroundDownloads
    ? "disabled"
    : status.state === "paused"
      ? "paused"
      : status.state === "running"
        ? progress && progress.phase !== "checking"
          ? "running"
          : "checking"
        : status.state === "settled"
          ? progress?.phase === "complete"
            ? "complete"
            : "partial"
          : "idle"
  useEffect(() => {
    void appBackgroundDownloads.recheck()
    const recheck = () => {
      void appBackgroundDownloads.recheck()
    }
    window.addEventListener("focus", recheck)
    return () => {
      window.removeEventListener("focus", recheck)
    }
  }, [])
  return (
    <section
      aria-labelledby="settings-downloads"
      className="mt-6 rounded-block bg-surface-1 px-4 pb-4"
    >
      <h2 id="settings-downloads" className="pt-3 text-13 font-semibold text-text-3">
        {t("settings.downloads")}
      </h2>
      <SettingRow
        className="min-h-14 border-b border-border py-2"
        label={t("settings.downloadAll")}
        hint={t("settings.downloadAllHint")}
      >
        <Switch
          label={t("settings.downloadAll")}
          checked={prefs.backgroundDownloads}
          onChange={(backgroundDownloads) => prefsStore.set({ backgroundDownloads })}
        />
      </SettingRow>
      <div data-download-state={state} className="mt-3 space-y-2">
        <div className="flex flex-wrap items-center justify-between gap-2" aria-live="polite">
          <span className="text-14 font-semibold">{t(`settings.downloadStatus.${state}`)}</span>
          {progress && (
            <span className="text-13 text-text-2">
              {t("settings.downloadProgress", {
                done: progress.total - progress.missing,
                total: progress.total,
              })}
            </span>
          )}
        </div>
        {progress && (
          <progress
            className="h-2 w-full accent-accent"
            aria-label={t("settings.downloads")}
            max={Math.max(1, progress.total)}
            value={progress.total - progress.missing}
          />
        )}
        {selection && (
          <p className="text-13 text-text-2">
            {t("settings.downloadVersion", {
              version: formatDataVersion(selection.dataVersion),
              edition: t(`options.edition.${selection.edition}`),
            })}
          </p>
        )}
        <p className="text-13 text-text-3">
          {state === "partial"
            ? t(`settings.downloadReason.${progress?.reason ?? "manifest"}`)
            : state === "paused"
              ? t("settings.downloadPausedHint")
              : state === "disabled"
                ? t("settings.downloadOffHint")
                : t("settings.downloadScope")}
        </p>
      </div>
    </section>
  )
}
