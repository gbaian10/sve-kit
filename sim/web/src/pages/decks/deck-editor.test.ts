import { describe, expect, it } from "vitest"

import { createDeck } from "../../domain/deck/model"
import type { Deck } from "../../domain/deck/types"
import { card, lookup } from "../../test-utils/deck-cards"
import { createDeckEditor } from "./deck-editor"
import type { DeckFacts } from "./deck-info"

const cards = lookup(
  card("c:follower"),
  card("c:evolved", { specialKinds: ["evolve"] }),
  card("c:leader", { typeCode: "leader" }),
  card("c:token", { specialKinds: ["token"] }),
)
// The editor only asks the facts for each card's rule facts.
const facts = { factsOf: cards } as unknown as DeckFacts

const base = createDeck(
  { name: "編輯", edition: "jp", basis: { kind: "class", classCode: "forest" } },
  new Date("2026-11-03T00:00:00.000Z"),
)

/** An editor over `deck`; returns the decks it saved. */
function editorOf(deck: Partial<Deck> = {}, withFacts: DeckFacts | null = facts) {
  const saved: Deck[] = []
  const editor = createDeckEditor(
    {
      deck: { ...base, ...deck },
      facts: withFacts,
      printingsReady: true,
      dataFailed: false,
      retryData: () => undefined,
      restrictions: new Map(),
      construction: { state: "loading" },
      problems: [],
      uiLanguage: "zh-TW",
      textLang: "ja",
      nameDisplay: "original",
      openCard: () => undefined,
      highlight: null,
      focusCard: () => undefined,
      pickLeader: () => undefined,
    },
    (next) => saved.push(next),
  )
  return { editor, saved }
}

describe("deck editor", () => {
  it("adds a card where it belongs, a leader as the leader, and nothing it cannot place", () => {
    const { editor, saved } = editorOf()
    editor.add("c:follower")
    editor.add("c:evolved", "p:evolved-alt")
    editor.add("c:leader", "p:leader-alt")
    editor.add("c:token")
    editor.add("c:unknown")
    expect(saved.map((deck) => [deck.main, deck.evolve, deck.leader])).toEqual([
      [[{ cardId: "c:follower", count: 1 }], [], null],
      [[], [{ cardId: "c:evolved", count: 1, printingId: "p:evolved-alt" }], null],
      [[], [], { cardId: "c:leader", printingId: "p:leader-alt" }],
    ])
  })

  it("keeps adding past the same-name limit and removes only copies that are in", () => {
    // Construction rules only warn in the builder (maintainer decision 2026-11-03).
    const { editor, saved } = editorOf({ main: [{ cardId: "c:follower", count: 3 }] })
    expect(editor.statusOf("c:follower")).toMatchObject({ placement: "main", count: 3, limit: 3 })
    editor.add("c:follower")
    expect(saved[0]?.main).toEqual([{ cardId: "c:follower", count: 4 }])
    editor.remove("c:evolved")
    editor.remove("c:leader")
    expect(saved).toHaveLength(1)
    editor.remove("c:follower")
    expect(saved[1]?.main).toEqual([{ cardId: "c:follower", count: 2 }])
  })

  it("knows nothing about cards before the facts load", () => {
    const { editor, saved } = editorOf({ leader: { cardId: "c:leader" } }, null)
    expect(editor.statusOf("c:leader")).toEqual({
      placement: "none",
      count: 0,
      sideCount: 0,
      limit: 0,
      isLeader: true,
    })
    editor.add("c:follower")
    expect(saved).toEqual([])
  })

  it("sets counts, printings and the name, and clears the leader", () => {
    const { editor, saved } = editorOf({
      leader: { cardId: "c:leader" },
      main: [{ cardId: "c:follower", count: 1 }],
    })
    editor.setCount("main", "c:follower", 2)
    editor.setPrinting("main", "c:follower", "p:follower-alt")
    editor.setPrinting("leader", "c:leader", "p:leader-alt")
    // Another card's printing cannot become the leader's.
    editor.setPrinting("leader", "c:follower", "p:follower-alt")
    editor.rename("新名")
    editor.setLeader(null)
    expect(saved.map((deck) => [deck.main, deck.leader, deck.name])).toEqual([
      [[{ cardId: "c:follower", count: 2 }], { cardId: "c:leader" }, "編輯"],
      [
        [{ cardId: "c:follower", count: 1, printingId: "p:follower-alt" }],
        { cardId: "c:leader" },
        "編輯",
      ],
      [
        [{ cardId: "c:follower", count: 1 }],
        { cardId: "c:leader", printingId: "p:leader-alt" },
        "編輯",
      ],
      [[{ cardId: "c:follower", count: 1 }], { cardId: "c:leader" }, "新名"],
      [[{ cardId: "c:follower", count: 1 }], null, "編輯"],
    ])
  })

  it("moves side cards into their deck, but not ones that are absent or belong nowhere", () => {
    const { editor, saved } = editorOf({
      main: [{ cardId: "c:follower", count: 1 }],
      side: [
        { cardId: "c:follower", count: 2 },
        { cardId: "c:token", count: 1 },
      ],
    })
    expect(editor.statusOf("c:follower").sideCount).toBe(2)
    editor.moveIn("c:evolved")
    editor.moveIn("c:token")
    editor.moveIn("c:follower")
    expect(saved).toHaveLength(1)
    expect(saved[0]?.main).toEqual([{ cardId: "c:follower", count: 3 }])
    expect(saved[0]?.side).toEqual([{ cardId: "c:token", count: 1 }])
  })

  it("moves nothing in before the facts load", () => {
    const { editor, saved } = editorOf({ side: [{ cardId: "c:follower", count: 1 }] }, null)
    editor.moveIn("c:follower")
    expect(saved).toEqual([])
  })
})
