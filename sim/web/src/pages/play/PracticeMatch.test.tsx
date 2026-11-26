import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, it, vi } from "vitest"

import type { BattleBuild, InitMessage, SeatRequest } from "../../battle/engine/protocol"
import { SCENES } from "../../battle/fixtures/scenes"
import * as data from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { PracticeMatch } from "./PracticeMatch"

vi.mock("./battle-cards", async (importOriginal) => {
  const original = await importOriginal<typeof import("./battle-cards")>()
  return { ...original, useBattleCards: original.useCards }
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  prefsStore.set(DEFAULT_PREFS)
})

async function practice(failure?: "manifest" | "printings") {
  const scene = SCENES.find(({ id }) => id === "firstPlayer")
  if (!scene) throw new Error("missing first-player scene")
  const build: BattleBuild = {
    manifest: `sha256:${"1".repeat(64)}`,
    programs: { key: "programs", bytes: 1, path: "program.json", sha256: "unused" },
    leaders: { p1: scene.leaders[0], p2: scene.leaders[1] },
    options: {
      decks: [
        { construction: { kind: "class", value: "synthetic" }, main: [], evolve: [] },
        { construction: { kind: "class", value: "synthetic" }, main: [], evolve: [] },
      ],
      control: ["player", "policy"],
      max_submissions: 1,
    },
  }
  const client: data.SnapshotClient = {
    base: "/cdn",
    status: () =>
      failure === "manifest"
        ? { state: "error", kind: "network", detail: "404" }
        : { state: "ready", dataVersion: "synthetic" },
    subscribe: () => () => undefined,
    load: () => Promise.resolve(),
    reload: () => Promise.resolve(),
    retry: () => Promise.resolve(),
    fragments: () => Promise.resolve([]),
    snapshot: () => ({
      dataVersion: "synthetic",
      manifestHash: build.manifest,
      printingReady: true,
      manifest: {},
      config: {},
      files: new Map(),
      bootstrap: [],
    }),
    prefetchPrintings: () =>
      failure === "printings" ? Promise.reject(new Error("missing printing")) : Promise.resolve(),
  }
  vi.spyOn(data, "createSnapshotClient").mockReturnValue(client)

  let init: InitMessage | undefined
  const terminate = vi.fn()
  vi.stubGlobal(
    "Worker",
    class {
      postMessage(message: InitMessage) {
        init = message
      }
      terminate = terminate
    },
  )
  prefsStore.set({ uiLanguage: "zh-TW" })
  const view = await renderRoutes([{ path: "/", element: <PracticeMatch build={build} /> }])
  const start = screen.getByRole("button", { name: "開始練習對局" })
  expect(start).toBeDisabled()
  expect(Object.keys(init?.ports ?? {})).toEqual(["p1"])
  const port = init?.ports.p1
  if (!port) throw new Error("missing player port")
  for (const message of scene.script.p1) port.postMessage(message)
  return { start, port, view, terminate }
}

it("waits for the pinned catalogue and a player's opening question, then submits through that seat", async () => {
  const user = userEvent.setup()
  const { start, port, view, terminate } = await practice()
  try {
    const received = new Promise<SeatRequest>((resolve) => {
      port.onmessage = (event: MessageEvent<SeatRequest>) => {
        resolve(event.data)
      }
    })
    await waitFor(() => expect(start).toBeEnabled())
    await user.click(start)
    const group = await screen.findByRole("group", { name: "對局操作" })
    await user.click(within(group).getByRole("button", { name: "選擇先攻" }))
    expect(await received).toMatchObject({
      kind: "submit",
      submit: {
        command: { kind: "answer", question_id: "first-player", answer: { chosen: ["seat:p1"] } },
      },
    })
    view.unmount()
    expect(terminate).toHaveBeenCalledOnce()
  } finally {
    port.close()
  }
})

it.each(["manifest", "printings"] as const)(
  "blocks entry when the pinned %s cannot load",
  async (failure) => {
    const { start, port } = await practice(failure)
    try {
      expect(await screen.findByRole("alert")).toHaveTextContent("無法繼續")
      expect(start).toBeDisabled()
    } finally {
      port.close()
    }
  },
)
