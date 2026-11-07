import { useEffect, useMemo, useState } from "react"

import { useRecentSummaries } from "../../app/snapshot"
import { useQuickBarClasses } from "../../components/search/useQuickBarClasses"
import {
  appScanModels,
  type CardSummary,
  type ImageIndex,
  SCAN_PARTS,
  scanFaceImage,
  type ScanModelsStatus,
} from "../../data"
import { type DisplayedName, displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { toggleClass } from "../../domain/scan/class-pick"
import type { ScanCard } from "../../domain/scan/search"
import type { Region } from "../../domain/search"
import { currentUiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"
import { scanDownloadsFor, useScanDownloads } from "./downloads"

export const megabytes = (bytes: number) => (bytes / 1048576).toFixed(1)

/** Single and deck screens share one class pick for the page load. */
let pickedClasses: readonly string[] = []

export function useScanClassPick(): [readonly string[], (code: string) => void] {
  const [classes, setClasses] = useState(pickedClasses)
  const toggle = (code: string) => {
    const next = toggleClass(classes, code)
    pickedClasses = next
    setClasses(next)
  }
  return [classes, toggle]
}

export interface ScanClass {
  readonly code: string
  readonly label: string
}

/** The snapshot's classes in quick-bar order, named in the UI language; undefined while loading. */
export function useScanClasses(): readonly ScanClass[] | undefined {
  const prefs = usePrefs()
  return useQuickBarClasses(UI_TEXT_LANG[currentUiLanguage(prefs.uiLanguage)])
}

/** "Elf, Neutral" or "all classes", for hints. */
export function classSummary(
  picked: readonly string[],
  classes: readonly ScanClass[] | undefined,
  t: (key: "scan.allClasses" | "scan.listSeparator") => string,
): string {
  if (picked.length === 0) return t("scan.allClasses")
  return picked
    .map((code) => classes?.find((item) => item.code === code)?.label ?? code)
    .join(t("scan.listSeparator"))
}

export interface ScanResult {
  readonly card: ScanCard
  /** The site's summary of the printing's front, once the search index answered; none for other faces. */
  readonly summary: CardSummary | undefined
  readonly name: DisplayedName | undefined
  /** For a non-front face: its own image, since the summary describes the front. */
  readonly faceImage?: string
}

/** Resolves recognised cards to the site's summaries, names in the UI language and images. */
export function useScanResults(
  cards: readonly ScanCard[],
  edition: Region,
): { readonly results: readonly ScanResult[]; readonly images: ImageIndex | undefined } {
  const prefs = usePrefs()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const numbers = useMemo(() => cards.map((card) => card.no), [cards])
  const found = useRecentSummaries(numbers, edition, "card-no")
  const summaries = found.phase === "complete" ? found.recent : undefined
  const results = useMemo(
    () =>
      cards.map((card, at) => {
        // Summaries describe the front; another face shows its own image and printed name.
        if (card.face > 0)
          return {
            card,
            summary: undefined,
            name: undefined,
            faceImage: scanFaceImage(card.imageId, card.face),
          }
        const summary = summaries?.[at]?.summary
        return {
          card,
          summary,
          name: summary ? displayName(summary.name, uiLanguage, prefs.nameDisplay) : undefined,
        }
      }),
    [cards, summaries, uiLanguage, prefs.nameDisplay],
  )
  return { results, images: found.images }
}

export interface ScanReadiness {
  /** Null while checking the device. */
  readonly status: ScanModelsStatus | null
  /** Every part is on this device (an older copy counts). */
  readonly ready: boolean
  readonly missingBytes: number
  /** Downloaded / total bytes per part while downloading. */
  readonly progress: Partial<Record<(typeof SCAN_PARTS)[number], readonly [number, number]>>
  readonly downloading: boolean
  readonly failed: boolean
  readonly download: () => void
}

/** Whether the scanner can start on this device, and the in-place download when it cannot. */
export function useScanReadiness(edition: Region, store = appScanModels): ScanReadiness {
  const downloads = scanDownloadsFor(store)
  const { runs, finished } = useScanDownloads(downloads)
  const [status, setStatus] = useState<ScanModelsStatus | null>(null)
  useEffect(() => {
    let active = true
    void store.status(edition).then((next) => {
      if (active) setStatus(next)
    })
    return () => {
      active = false
    }
  }, [store, edition, finished])
  const ready =
    status !== null && SCAN_PARTS.every((part) => status.parts[part].state !== "missing")
  const missing = status ? SCAN_PARTS.filter((part) => status.parts[part].state === "missing") : []
  const progress: ScanReadiness["progress"] = {}
  for (const part of SCAN_PARTS) {
    const run = runs[part]
    if (run?.state === "running") progress[part] = [run.got, run.total]
  }
  return {
    status,
    ready,
    missingBytes: status
      ? missing.reduce((sum, part) => sum + status.parts[part].missingBytes, 0)
      : 0,
    progress,
    downloading: Object.keys(progress).length > 0,
    failed: SCAN_PARTS.some((part) => runs[part]?.state === "failed"),
    download: () => void downloads.start(missing, edition),
  }
}
