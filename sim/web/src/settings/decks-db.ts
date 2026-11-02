import { parseDeck } from "../domain/deck/parse"
import type { Deck } from "../domain/deck/types"

/** Saved decks live in their own database so clearing caches never touches them. */
export const DECKS_DB_NAME = "sve-kit-decks"
const STORE = "decks"

let connection: Promise<IDBDatabase> | undefined

function open(): Promise<IDBDatabase> {
  connection ??= new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(DECKS_DB_NAME, 1)
    request.onupgradeneeded = () => {
      request.result.createObjectStore(STORE, { keyPath: "id" })
    }
    request.onsuccess = () => {
      const database = request.result
      // Another tab upgrading or deleting the database needs this connection gone.
      database.onversionchange = () => {
        database.close()
        connection = undefined
      }
      resolve(database)
    }
    request.onerror = () => {
      reject(request.error ?? new Error("Cannot open the deck database"))
    }
    request.onblocked = () => {
      reject(new Error("The deck database is blocked by another tab"))
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
    // A write counts only once the transaction commits, not when the request succeeds.
    transaction.oncomplete = () => {
      resolve(request.result)
    }
    transaction.onerror = transaction.onabort = () => {
      reject(transaction.error ?? new Error("The deck database transaction failed"))
    }
  })
}

/** Every stored deck that reads back whole; damaged records are left in place and skipped. */
export async function readDecks(): Promise<Deck[]> {
  const records = await transact("readonly", (store) => store.getAll() as IDBRequest<unknown[]>)
  return records.flatMap((record) => {
    const deck = parseDeck(record)
    return deck ? [deck] : []
  })
}

export async function writeDeck(deck: Deck): Promise<void> {
  await transact("readwrite", (store) => store.put(deck))
}

export async function deleteDeck(id: string): Promise<void> {
  await transact("readwrite", (store) => store.delete(id))
}
