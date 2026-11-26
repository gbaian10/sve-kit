import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, it, vi } from "vitest"

import type { BattleBuild, InitMessage, SeatRequest } from "../../battle/engine/protocol"
import { SCENES } from "../../battle/fixtures/scenes"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { PracticeMatch } from "./PracticeMatch"

afterEach(() => {
  vi.unstubAllGlobals()
  prefsStore.set(DEFAULT_PREFS)
})

it("waits for a player's opening question, submits it through that seat and closes the Worker", async () => {
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
  const user = userEvent.setup()
  const view = await renderRoutes([{ path: "/", element: <PracticeMatch build={build} /> }])
  const start = screen.getByRole("button", { name: "開始練習對局" })
  expect(start).toBeDisabled()
  expect(Object.keys(init?.ports ?? {})).toEqual(["p1"])
  const port = init?.ports.p1
  if (!port) throw new Error("missing player port")
  try {
    const received = new Promise<SeatRequest>((resolve) => {
      port.onmessage = (event: MessageEvent<SeatRequest>) => {
        resolve(event.data)
      }
    })
    for (const message of scene.script.p1) port.postMessage(message)
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
