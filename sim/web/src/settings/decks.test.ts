import "fake-indexeddb/auto"

import { afterEach, describe, expect, it } from "vitest"

import { createDeck, renameDeck } from "../domain/deck/model"
import { decksStore } from "./decks"
import { DECKS_DB_NAME } from "./decks-db"

const NOW = new Date("2026-11-03T01:00:00.000Z")

const deck = (name: string, now = NOW) =>
  createDeck({ name, edition: "jp", basis: { kind: "class", classCode: "forest" } }, now)

const ready = async () => {
  await decksStore.reload()
  const state = decksStore.get()
  if (state.status !== "ready") throw new Error(`decks not ready: ${state.status}`)
  return state.decks
}

/** Writes a record straight into the database, as another build or a damaged write would. */
function putRaw(record: unknown): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DECKS_DB_NAME, 1)
    request.onsuccess = () => {
      const transaction = request.result.transaction("decks", "readwrite")
      transaction.objectStore("decks").put(record)
      transaction.oncomplete = () => {
        request.result.close()
        resolve()
      }
      transaction.onerror = () => {
        reject(transaction.error ?? new Error("put failed"))
      }
    }
    request.onerror = () => {
      reject(request.error ?? new Error("open failed"))
    }
  })
}

afterEach(async () => {
  for (const item of await ready()) await decksStore.remove(item.id)
})

describe("decksStore", () => {
  it("keeps saved decks in IndexedDB across a fresh read, newest first", async () => {
    const older = deck("Older", new Date("2026-11-01T00:00:00.000Z"))
    const newer = deck("Newer", new Date("2026-11-02T00:00:00.000Z"))
    await decksStore.save(older)
    await decksStore.save(newer)
    expect((await ready()).map((item) => item.name)).toEqual(["Newer", "Older"])
  })

  it("replaces the stored deck with the same id instead of adding one", async () => {
    const original = deck("A")
    await decksStore.save(original)
    await decksStore.save(renameDeck(original, "B", new Date("2026-11-04T00:00:00.000Z")))
    const decks = await ready()
    expect(decks).toHaveLength(1)
    expect(decks[0]).toMatchObject({ id: original.id, name: "B", revision: 2 })
  })

  it("forgets a removed deck", async () => {
    const kept = deck("Kept")
    const removed = deck("Removed")
    await decksStore.save(kept)
    await decksStore.save(removed)
    await decksStore.remove(removed.id)
    expect((await ready()).map((item) => item.id)).toEqual([kept.id])
  })

  it("skips a damaged record without losing the others", async () => {
    const good = deck("Good")
    await decksStore.save(good)
    await putRaw({ id: "broken", name: 3 })
    await putRaw({ ...good, id: "bad-date", updatedAt: "not-a-date" })
    expect((await ready()).map((item) => item.id)).toEqual([good.id])
  })

  it("applies quick successive edits in order without losing any", async () => {
    const original = deck("A")
    await decksStore.save(original)
    const edits = ["B", "C", "D"].map((name, index) =>
      decksStore.save(renameDeck(original, name, new Date(Date.UTC(2026, 10, 4, 0, index)))),
    )
    const shown = decksStore.get()
    expect(shown.status === "ready" && shown.decks[0]?.name).toBe("D")
    await Promise.all(edits)
    expect((await ready()).map((item) => item.name)).toEqual(["D"])
  })

  it("keeps edits still on their way when storage is read in between", async () => {
    const original = deck("A")
    await decksStore.save(original)
    const later = (minute: number) => new Date(Date.UTC(2026, 10, 4, 0, minute))
    const b = decksStore.save(renameDeck(original, "B", later(1)))
    const c = decksStore.save(renameDeck(original, "C", later(2)))
    await decksStore.reload()
    const during = decksStore.get()
    expect(during.status === "ready" && during.decks.map((item) => item.name)).toEqual(["C"])
    await Promise.all([b, c])
    const after = decksStore.get()
    expect(after.status === "ready" && after.decks.map((item) => item.name)).toEqual(["C"])
    expect((await ready()).map((item) => item.name)).toEqual(["C"])
  })

  it("drops a failed edit but keeps the edits after it", async () => {
    const original = deck("A")
    await decksStore.save(original)
    // A function cannot be stored, so this write fails inside IndexedDB.
    const broken = { ...renameDeck(original, "B", NOW), extra: () => undefined }
    const failed = decksStore.save(broken)
    const c = decksStore.save(renameDeck(original, "C", new Date("2026-11-05T00:00:00.000Z")))
    await expect(failed).rejects.toThrow()
    await c
    const shown = decksStore.get()
    expect(shown.status === "ready" && shown.decks.map((item) => item.name)).toEqual(["C"])
    expect((await ready()).map((item) => item.name)).toEqual(["C"])
  })

  it("tells subscribers when the list changes", async () => {
    await ready()
    let calls = 0
    const unsubscribe = decksStore.subscribe(() => {
      calls += 1
    })
    await decksStore.save(deck("A"))
    unsubscribe()
    expect(calls).toBeGreaterThan(0)
    const state = decksStore.get()
    expect(state.status === "ready" && state.decks.map((item) => item.name)).toEqual(["A"])
  })
})
