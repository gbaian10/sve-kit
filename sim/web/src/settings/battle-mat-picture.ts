// The player's own picture for the battle mat: one image, kept whole with the frame the player put
// on it, in this browser's IndexedDB and never sent anywhere. It lives in a database of its own,
// which clearing the local data removes with the other caches. Storage that is missing or refuses
// (private windows, blocked site data) is not an error for the table: a picture that could not be
// read back is no picture, and one that could not be written leaves what was there before.
import { useSyncExternalStore } from "react"

import { type BattleMatFraming, type BattleMatSize, isBattleMatFraming } from "./prefs"

const DB_NAME = "sve-kit-battle-mat"
const STORE = "picture"
const KEY = "mat"

let connection: Promise<IDBDatabase> | undefined

function open(): Promise<IDBDatabase> {
  connection ??= new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, 1)
    request.onupgradeneeded = () => {
      request.result.createObjectStore(STORE)
    }
    request.onsuccess = () => {
      const database = request.result
      // Another tab deleting the database (clearing local data) needs this connection gone.
      database.onversionchange = () => {
        database.close()
        connection = undefined
      }
      resolve(database)
    }
    request.onerror = () => {
      reject(request.error ?? new Error("Cannot open the mat picture database"))
    }
    request.onblocked = () => {
      reject(new Error("The mat picture database is blocked by another tab"))
    }
  }).catch((error: unknown) => {
    connection = undefined
    throw error
  })
  return connection
}

async function transact<T>(
  mode: IDBTransactionMode,
  work: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  const database = await open()
  return new Promise<T>((resolve, reject) => {
    const transaction = database.transaction(STORE, mode)
    const request = work(transaction.objectStore(STORE))
    transaction.oncomplete = () => {
      resolve(request.result)
    }
    transaction.onerror = transaction.onabort = () => {
      reject(transaction.error ?? new Error("The mat picture transaction failed"))
    }
  })
}

/** The picture as it is kept: the whole of it, its size, and where the player framed it. */
interface Kept {
  readonly picture: Blob
  readonly size: BattleMatSize
  readonly framing: BattleMatFraming
}

/** The kept picture as the table shows it: `url` is an address an image element can load. */
export interface BattleMatPicture {
  readonly url: string
  readonly picture: Blob
  readonly size: BattleMatSize
  readonly framing: BattleMatFraming
}

const positive = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value) && value > 0

/** A record read back whole; anything else (an older shape, a damaged one) counts as no picture. */
function parse(stored: unknown): Kept | null {
  if (typeof stored !== "object" || stored === null) return null
  const { picture, size, framing } = stored as Partial<Record<keyof Kept, unknown>>
  if (!(picture instanceof Blob) || typeof size !== "object" || size === null) return null
  const { w, h } = size as Partial<Record<keyof BattleMatSize, unknown>>
  if (!positive(w) || !positive(h) || !isBattleMatFraming(framing)) return null
  return { picture, size: { w, h }, framing }
}

// What the table shows, or null when there is no picture.
let shown: BattleMatPicture | null = null
let loaded = false
const listeners = new Set<() => void>()

function show(kept: Kept | null): void {
  // The same picture framed anew keeps its address: nothing reloads.
  const url =
    kept !== null && shown?.picture === kept.picture
      ? shown.url
      : kept === null
        ? null
        : URL.createObjectURL(kept.picture)
  if (shown !== null && shown.url !== url) URL.revokeObjectURL(shown.url)
  shown = kept === null || url === null ? null : { ...kept, url }
  for (const listener of listeners) listener()
}

async function load(): Promise<void> {
  try {
    show(parse(await transact("readonly", (store) => store.get(KEY) as IDBRequest<unknown>)))
  } catch {
    show(null)
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  if (!loaded) {
    loaded = true
    void load()
  }
  return () => {
    listeners.delete(listener)
  }
}

/** The player's kept picture; null while it loads and when there is none. */
export function useBattleMatPicture(): BattleMatPicture | null {
  return useSyncExternalStore(subscribe, () => shown)
}

/**
 * Keeps the picture; false when this browser would not store it. A picture kept before stays as it
 * was then; with none before, the table goes on without one.
 */
export async function saveBattleMatPicture(
  picture: Blob,
  size: BattleMatSize,
  framing: BattleMatFraming,
): Promise<boolean> {
  const kept: Kept = { picture, size, framing }
  try {
    await transact("readwrite", (store) => store.put(kept, KEY))
  } catch {
    return false
  }
  loaded = true
  show(kept)
  return true
}

export async function clearBattleMatPicture(): Promise<void> {
  try {
    await transact("readwrite", (store) => store.delete(KEY))
  } catch {
    // Nothing stored that could be shown again.
  }
  loaded = true
  show(null)
}
