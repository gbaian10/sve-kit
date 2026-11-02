import { useSyncExternalStore } from "react"

import type { Deck } from "../domain/deck/types"
import { deleteDeck, readDecks, writeDeck } from "./decks-db"

/** `unavailable`: the browser has no usable IndexedDB (private mode, blocked storage). */
export type DecksState =
  | { readonly status: "loading" | "unavailable" }
  | { readonly status: "ready"; readonly decks: readonly Deck[] }

const CHANNEL = "sve-kit-decks"

type Change = (decks: readonly Deck[]) => readonly Deck[]

const newestFirst = (a: Deck, b: Deck) =>
  b.updatedAt.localeCompare(a.updatedAt) || a.id.localeCompare(b.id)

function createDecksStore() {
  let state: DecksState = { status: "loading" }
  let loading: Promise<void> | undefined
  let channel: BroadcastChannel | undefined
  const listeners = new Set<() => void>()

  const publish = (next: DecksState) => {
    state = next
    for (const listener of listeners) listener()
  }
  // The list is what storage last confirmed with the changes still on their way laid over it.
  // Each change replaces or removes whole decks, so laying one over a read that already holds it
  // changes nothing; a read in the middle of a run of edits therefore never shows an older deck.
  let confirmed: readonly Deck[] = []
  const pending: Change[] = []
  const show = () => {
    const decks = pending.reduce<readonly Deck[]>((list, change) => change(list), confirmed)
    publish({ status: "ready", decks: decks.toSorted(newestFirst) })
  }
  const refresh = async () => {
    try {
      confirmed = await readDecks()
      show()
    } catch {
      publish({ status: "unavailable" })
    }
  }
  const load = () => {
    if (loading) return loading
    if (typeof BroadcastChannel !== "undefined") {
      channel = new BroadcastChannel(CHANNEL)
      channel.onmessage = () => {
        void refresh()
      }
    }
    loading = refresh()
    return loading
  }
  // Writes reach storage one at a time in the order they were made; a failed one leaves storage as
  // it was, so its change is simply dropped from the list.
  let writes: Promise<void> = Promise.resolve()
  const write = async (change: Change, store: () => Promise<void>) => {
    if (state.status === "loading") await load()
    if (state.status !== "ready") throw new Error("Decks cannot be saved in this browser")
    pending.push(change)
    show()
    const done = writes.then(store)
    writes = done.catch(() => undefined)
    let failure: { readonly error: unknown } | undefined
    try {
      await done
      confirmed = change(confirmed)
    } catch (error) {
      failure = { error }
    }
    pending.splice(pending.indexOf(change), 1)
    show()
    if (failure) throw failure.error
    channel?.postMessage("changed")
  }

  return {
    get: () => state,
    subscribe: (listener: () => void) => {
      listeners.add(listener)
      void load()
      return () => {
        listeners.delete(listener)
      }
    },
    /** Adds the deck or replaces the stored one with the same id. */
    save: (deck: Deck) =>
      write(
        (decks) => [...decks.filter((item) => item.id !== deck.id), deck],
        () => writeDeck(deck),
      ),
    remove: (id: string) =>
      write(
        (decks) => decks.filter((item) => item.id !== id),
        () => deleteDeck(id),
      ),
    /** Reads storage again; for a browser where another tab changed it without telling. */
    reload: refresh,
  }
}

export const decksStore = createDecksStore()

export function useDecks(): DecksState {
  return useSyncExternalStore(decksStore.subscribe, decksStore.get, decksStore.get)
}
