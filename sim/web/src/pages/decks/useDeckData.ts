import { useCallback, useEffect, useMemo, useState } from "react"

import { useCardSnapshot, useImageIndex, useRecentSummaries } from "../../app/snapshot"
import {
  type CardRestriction,
  type ImageIndex,
  loadConstructionIndex,
  loadRestrictionSelection,
  type RestrictionStatus,
} from "../../data"
import type { ConstructionIndex } from "../../domain/deck/construction"
import type { Deck } from "../../domain/deck/types"
import type { Region } from "../../domain/search"
import { createDeckFacts, type DeckFacts } from "./deck-info"

export interface DeckData {
  readonly facts: DeckFacts | null
  readonly restrictions: ReadonlyMap<string, readonly CardRestriction[]>
  readonly restrictionStatus: RestrictionStatus | undefined
  /** The construction rules of the snapshot's edition; `failed` when the index is missing. */
  readonly construction: ConstructionState
  /** Image metadata of the main-thread snapshot (base printings). */
  readonly images: ImageIndex | undefined
  /**
   * Every printing of the edition is loaded, reprints included, so card numbers and permanent
   * printing integers resolve completely (share codes and imports wait for it).
   */
  readonly printingsReady: boolean
  /** The cards could not be loaded; `retry` asks again. */
  readonly failed: boolean
  readonly retry: () => void
}

export type ConstructionState =
  | { readonly state: "loading" }
  | { readonly state: "failed" }
  | { readonly state: "ready"; readonly index: ConstructionIndex }

const NO_RESTRICTIONS: ReadonlyMap<string, readonly CardRestriction[]> = new Map()

/** The main-thread snapshot (the card page's path) as the deck builder's card facts. */
export function useDeckData(edition: Region): DeckData {
  const { client, catalog, status } = useCardSnapshot()
  const snapshot = client.snapshot()
  // The previous region's catalog stays until the deck's edition has loaded.
  const current =
    catalog && (snapshot?.region === undefined || snapshot.region === edition) ? catalog : null
  const facts = useMemo(
    () => (current ? createDeckFacts(current, edition) : null),
    [current, edition],
  )
  const key = snapshot ? `${snapshot.manifestHash}\u0000${edition}` : ""
  const [selection, setSelection] = useState<{
    readonly key: string
    readonly map: ReadonlyMap<string, readonly CardRestriction[]>
    readonly status: RestrictionStatus | undefined
  }>({ key: "", map: NO_RESTRICTIONS, status: undefined })
  useEffect(() => {
    if (!snapshot || !current) return
    let active = true
    void loadRestrictionSelection(client, snapshot, edition).then(
      (result) => {
        if (active) setSelection({ key, map: result.details, status: result.status })
      },
      () => {
        if (active) setSelection({ key, map: NO_RESTRICTIONS, status: undefined })
      },
    )
    return () => {
      active = false
    }
  }, [client, snapshot, current, edition, key])
  const [construction, setConstruction] = useState<{
    readonly key: string
    readonly value: ConstructionState
  }>({ key: "", value: { state: "loading" } })
  useEffect(() => {
    if (!snapshot || !current) return
    let active = true
    loadConstructionIndex(client.base, snapshot, edition).then(
      (index) => {
        if (active) setConstruction({ key, value: { state: "ready", index } })
      },
      () => {
        if (active) setConstruction({ key, value: { state: "failed" } })
      },
    )
    return () => {
      active = false
    }
  }, [client, snapshot, current, edition, key])
  const images = useImageIndex(client, current !== null)
  // The basic catalog holds reprints only as locators; the full printing data replaces the
  // snapshot when it arrives, and the facts are rebuilt from it.
  const [attempt, setAttempt] = useState(0)
  const [printingFailure, setPrintingFailure] = useState<string | null>(null)
  useEffect(() => {
    if (!snapshot || !current || snapshot.printingReady === true) return
    let active = true
    client.prefetchPrintings?.().catch(() => {
      if (active) setPrintingFailure(snapshot.manifestHash)
    })
    return () => {
      active = false
    }
  }, [client, snapshot, current, attempt])
  const printingsReady =
    current !== null && (snapshot?.printingReady === true || client.prefetchPrintings === undefined)
  const failed =
    status.state === "error" ||
    (!printingsReady && printingFailure !== null && printingFailure === snapshot?.manifestHash)
  const snapshotFailed = status.state === "error"
  const retry = useCallback(() => {
    setPrintingFailure(null)
    if (snapshotFailed) void client.retry()
    else setAttempt((count) => count + 1)
  }, [client, snapshotFailed])
  return {
    printingsReady,
    failed,
    retry,
    facts,
    restrictions: selection.key === key ? selection.map : NO_RESTRICTIONS,
    restrictionStatus: selection.key === key ? selection.status : undefined,
    construction: construction.key === key ? construction.value : { state: "loading" },
    images,
  }
}

const CHUNK = 20

function mergeImageIndexes(indexes: readonly (ImageIndex | undefined)[]): ImageIndex {
  const present = indexes.flatMap((index) => (index ? [index] : []))
  const owner = (printingId: string, faceId: string) =>
    present.find(
      (index) =>
        index.known?.(printingId, faceId) === true || index.asset(printingId, faceId) !== undefined,
    )
  return {
    known: (printingId, faceId) => owner(printingId, faceId) !== undefined,
    asset: (printingId, faceId) => owner(printingId, faceId)?.asset(printingId, faceId),
    cardImage: (printingId, faceId) => owner(printingId, faceId)?.cardImage(printingId, faceId),
    artImage: (printingId, faceId) => owner(printingId, faceId)?.artImage?.(printingId, faceId),
    eager: (printingId, faceId) =>
      present.some((index) => index.eager?.(printingId, faceId) === true),
    failedFace: (printingId, faceId) =>
      owner(printingId, faceId) === undefined &&
      present.some((index) => index.failedFace?.(printingId, faceId) === true),
  }
}

/**
 * Image metadata for the deck's own printings. The main-thread snapshot carries only the base
 * printings (a reprint such as BP22-R23 is a locator without image rows there), so the deck asks
 * the search worker the way the recent list does, twenty printings per request, and falls back to
 * the main-thread index for the rest.
 */
export function useDeckImages(
  deck: Deck,
  facts: DeckFacts | null,
  fallback: ImageIndex | undefined,
): ImageIndex | undefined {
  const ids = useMemo(() => {
    if (!facts) return []
    const seen = new Set<string>()
    for (const entry of [
      ...(deck.leader ? [deck.leader] : []),
      ...deck.main,
      ...deck.evolve,
      ...deck.side,
    ]) {
      const summary = facts.summaryOf(entry.cardId, entry.printingId)
      if (summary) seen.add(summary.printingId)
    }
    return [...seen]
  }, [deck.leader, deck.main, deck.evolve, deck.side, facts])
  const first = useRecentSummaries(ids.slice(0, CHUNK), deck.edition)
  const second = useRecentSummaries(ids.slice(CHUNK, CHUNK * 2), deck.edition)
  const third = useRecentSummaries(ids.slice(CHUNK * 2, CHUNK * 3), deck.edition)
  return useMemo(
    () => mergeImageIndexes([first.images, second.images, third.images, fallback]),
    [first.images, second.images, third.images, fallback],
  )
}
