import { useEffect, useMemo, useState } from "react"

import { useRecentSummaries } from "../../app/snapshot"
import { useQuickBarClasses } from "../../components/search/useQuickBarClasses"
import {
  appScanModels,
  type CardSummary,
  type ImageIndex,
  mergeImageIndexes,
  SCAN_PARTS,
  scanCardImage,
  type ScanBackend,
  type ScanModelsStatus,
} from "../../data"
import { type DisplayedName, displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { toggleClass } from "../../domain/scan/class-pick"
import type { ScanCard } from "../../domain/scan/search"
import type { Region } from "../../domain/search"
import { currentUiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"
import { scanDownloadsFor, useScanDownloads } from "./downloads"
import { currentBackend, neededParts } from "./runtime-choice"

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

// The worker answers up to 20 cards per request; a deck photo can hold up to 60 different cards.
const CHUNK = 20

/** Resolves recognised cards to the site's summaries, names in the UI language and images. */
export function useScanResults(
  cards: readonly ScanCard[],
  edition: Region,
): { readonly results: readonly ScanResult[]; readonly images: ImageIndex | undefined } {
  const prefs = usePrefs()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const numbers = useMemo(() => [...new Set(cards.map((card) => card.no))], [cards])
  const first = useRecentSummaries(numbers.slice(0, CHUNK), edition, "card-no")
  const second = useRecentSummaries(numbers.slice(CHUNK, 2 * CHUNK), edition, "card-no")
  const third = useRecentSummaries(numbers.slice(2 * CHUNK, 3 * CHUNK), edition, "card-no")
  const results = useMemo(() => {
    const byNumber = new Map<string, CardSummary>()
    for (const [offset, found] of [first, second, third].entries()) {
      if (found.phase !== "complete") continue
      found.recent?.forEach((item, at) => {
        const no = numbers[offset * CHUNK + at]
        if (item.summary && no !== undefined) byNumber.set(no, item.summary)
      })
    }
    return cards.map((card) => {
      // Summaries describe the front; another face shows its own image and printed name.
      if (card.face > 0)
        return {
          card,
          summary: undefined,
          name: undefined,
          faceImage: scanCardImage(card.imageId, card.face),
        }
      const summary = byNumber.get(card.no)
      return {
        card,
        summary,
        name: summary ? displayName(summary.name, uiLanguage, prefs.nameDisplay) : undefined,
      }
    })
  }, [cards, numbers, first, second, third, uiLanguage, prefs.nameDisplay])
  const images = useMemo(
    () => mergeImageIndexes([first.images, second.images, third.images]),
    [first.images, second.images, third.images],
  )
  return { results, images }
}

export interface ScanReadiness {
  /** Null while checking the device. */
  readonly status: ScanModelsStatus | null
  /** Models may be older, but the runtime must match this site's JavaScript. */
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
  const [backend, setBackend] = useState<ScanBackend | null>(null)
  useEffect(() => {
    let active = true
    void store.status(edition).then((next) => {
      if (active) setStatus(next)
    })
    void currentBackend().then((next) => {
      if (active) setBackend(next)
    })
    return () => {
      active = false
    }
  }, [store, edition, finished])
  // Only the chosen engine's runtime counts; the other one is not needed to scan.
  const needed = backend ? neededParts(backend) : []
  const missing = status
    ? needed.filter((part) =>
        part.startsWith("runtime-")
          ? status.parts[part].state !== "ready"
          : status.parts[part].state === "missing",
      )
    : []
  const ready = status !== null && backend !== null && missing.length === 0
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
