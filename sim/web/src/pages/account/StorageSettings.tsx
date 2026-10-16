import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"

import { Button } from "../../components/ui/Button"
import { Dialog } from "../../components/ui/Dialog"
import { SettingRow } from "../../components/ui/SettingRow"
import { estimateStorage, type StorageEstimate } from "../../data"
import { getCacheClearResult, requestColdStart } from "../../settings"

export function StorageSettings() {
  const { t, i18n } = useTranslation()
  const [usage, setUsage] = useState<StorageEstimate>()
  const [busy, setBusy] = useState(true)
  const [confirming, setConfirming] = useState(false)
  const [clearResult, setClearResult] = useState(getCacheClearResult)

  useEffect(() => {
    let active = true
    void estimateStorage().then((value) => {
      if (active) {
        setUsage(value)
        setBusy(false)
      }
    })
    return () => {
      active = false
    }
  }, [])

  const refresh = async () => {
    setBusy(true)
    setUsage(await estimateStorage())
    setBusy(false)
  }
  const clear = () => {
    try {
      requestColdStart("caches")
      window.location.reload()
    } catch {
      setClearResult("failed")
    }
    setConfirming(false)
  }
  const format = (bytes: number | undefined) => {
    if (busy) return t("settings.storageEstimating")
    if (bytes === undefined) return t("settings.storageUnavailable")
    return t("settings.storageMiB", {
      value: new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 2 }).format(
        bytes / (1024 * 1024),
      ),
    })
  }

  return (
    <section
      aria-labelledby="settings-storage"
      className="mt-4 rounded-block bg-surface-1 px-4 pb-4"
    >
      <h2 id="settings-storage" className="pt-3 text-13 font-semibold text-text-3">
        {t("settings.storage")}
      </h2>
      <div aria-live="polite" aria-busy={busy}>
        <SettingRow className="border-b border-border py-2" label={t("settings.storageTotal")}>
          <span>{format(usage?.total)}</span>
        </SettingRow>
        <SettingRow className="border-b border-border py-2" label={t("settings.storageImages")}>
          <span>{format(usage?.images)}</span>
        </SettingRow>
        <SettingRow className="border-b border-border py-2" label={t("settings.storageData")}>
          <span>{format(usage?.data)}</span>
        </SettingRow>
      </div>
      <p className="mt-3 text-13 text-text-3">{t("settings.storageHint")}</p>
      {clearResult && (
        <p role="status" className="mt-3 text-14">
          {t(clearResult === "failed" ? "settings.storageClearFailed" : "settings.storageCleared")}
        </p>
      )}
      <div className="mt-3 flex flex-wrap gap-2">
        <Button
          onClick={() => {
            void refresh()
          }}
          disabled={busy}
        >
          {t("settings.storageRefresh")}
        </Button>
        <Button
          variant="danger"
          onClick={() => {
            setConfirming(true)
          }}
        >
          {t("settings.storageClear")}
        </Button>
      </div>
      <Dialog
        open={confirming}
        onClose={() => {
          setConfirming(false)
        }}
        title={t("settings.storageClear")}
        variant="center"
        footer={
          <div className="flex flex-wrap justify-end gap-2">
            <Button
              onClick={() => {
                setConfirming(false)
              }}
            >
              {t("settings.storageCancel")}
            </Button>
            <Button variant="danger" onClick={clear}>
              {t("settings.storageConfirm")}
            </Button>
          </div>
        }
      >
        <p>{t("settings.storageConfirmHint")}</p>
      </Dialog>
    </section>
  )
}
