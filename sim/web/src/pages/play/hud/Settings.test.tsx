import "fake-indexeddb/auto"

import { act, fireEvent, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { CardSummary } from "../../../data"
import { DEFAULT_PREFS, prefsStore } from "../../../settings"
import { renderRoutes } from "../../../test-utils"
import { type BattleCards, BattleCardsContext, type KnownCard } from "../battle-cards"
import { SettingsButton } from "./Settings"

beforeEach(() => {
  prefsStore.set({ uiLanguage: "zh-TW" })
})

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
  document.documentElement.removeAttribute("lang")
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

const known = (printingId: string, cardNo: string, name: string): KnownCard => ({
  summary: { printingId, faceId: "f:test", cardNo, classCode: null } as CardSummary,
  name: { text: name, lang: "zh-Hant" },
  original: undefined,
  untranslated: false,
  classLabel: null,
  wording: undefined,
})

const LEADER = known("p:leader", "T-LD01", "測試主戰者")
const DRAGON = known("p:dragon", "T-001", "測試之龍")
const KNIGHT = known("p:knight", "T-002", "測試騎士")
const ALL = [LEADER, DRAGON, KNIGHT]

/** A snapshot of three cards; a search finds the ones whose name or number holds the text. */
const CARDS: BattleCards = {
  images: undefined,
  text: undefined,
  find: (card) => ALL.find((entry) => entry.summary.printingId === card.printing_id),
  search: (text, limit) =>
    ALL.filter(
      (entry) => entry.name.text.includes(text) || entry.summary.cardNo.includes(text),
    ).slice(0, limit),
}

/** Closes the settings the way Escape does: jsdom does not turn the key into the dialog's cancel. */
const dismiss = () => {
  fireEvent(screen.getByRole("dialog"), new Event("cancel", { cancelable: true }))
}

const open = async (aspect = 2) => {
  const user = userEvent.setup()
  await renderRoutes([
    {
      path: "*",
      element: (
        <BattleCardsContext value={CARDS}>
          <SettingsButton leader={{ printing_id: "p:leader", face: 0 }} matAspect={aspect} />
        </BattleCardsContext>
      ),
    },
  ])
  await user.click(screen.getByRole("button", { name: "對戰設定" }))
  return user
}

describe("the battle settings", () => {
  it("offers only what the chosen picture needs", async () => {
    const user = await open()
    const look = screen.getByRole("combobox", { name: "遊戲墊圖案" })
    // A drawn mat: no card to pick, nothing to upload, nothing to blur.
    expect(screen.queryByRole("searchbox")).toBeNull()
    expect(screen.queryByRole("slider")).toBeNull()
    await user.selectOptions(look, "卡面")
    expect(screen.getByRole("searchbox", { name: "搜尋卡名或卡號" })).toBeInTheDocument()
    expect(screen.getAllByRole("slider")).toHaveLength(2)
    await user.selectOptions(look, "自己的圖片")
    expect(screen.queryByRole("searchbox")).toBeNull()
    expect(screen.getByRole("button", { name: "選擇圖片" })).toBeInTheDocument()
    expect(screen.getAllByRole("slider")).toHaveLength(2)
  })

  it("starts from the player's leader and takes the card they search for", async () => {
    prefsStore.set({ battleMatLook: "card" })
    const user = await open()
    expect(screen.getByText("目前：測試主戰者（T-LD01） 你的主戰者")).toBeInTheDocument()
    const search = screen.getByRole("searchbox", { name: "搜尋卡名或卡號" })
    await user.type(search, "沒有這張")
    expect(screen.getByText("找不到卡片")).toBeInTheDocument()
    await user.clear(search)
    // By name, then by card number.
    await user.type(search, "測試之")
    expect(screen.getAllByRole("button", { pressed: false })).toHaveLength(1)
    await user.clear(search)
    await user.type(search, "T-00")
    await user.click(screen.getByRole("button", { name: /測試騎士/ }))
    // Picking a card goes to framing its art; nothing changes before the player says so.
    expect(screen.getByRole("dialog", { name: "選擇取景" })).toBeInTheDocument()
    expect(prefsStore.get().battleMatCard).toBeNull()
    // The art is 4:3 and the mat 2:1: the frame takes the whole width and the middle of the height.
    const frame = document.querySelector<HTMLElement>(".bp-crop-frame")
    expect([frame?.style.left, frame?.style.right]).toEqual(["0%", "0%"])
    expect(Number.parseFloat(frame?.style.top ?? "")).toBeCloseTo(16.667, 2)
    await user.click(screen.getByRole("button", { name: "使用" }))
    expect(prefsStore.get().battleMatCard).toBe("p:knight")
    expect(prefsStore.get().battleMatCardFraming).toEqual({
      cx: 0.5,
      cy: 0.5,
      span: 1,
      aspect: 2,
      reach: "table",
    })
    expect(screen.getByText("目前：測試騎士（T-002）")).toBeInTheDocument()
    // The frame can be changed later, starting from where it was.
    await user.click(screen.getByRole("button", { name: "調整取景" }))
    fireEvent.change(screen.getByRole("slider", { name: "放大" }), { target: { value: "2" } })
    await user.click(screen.getByRole("button", { name: "使用" }))
    expect(prefsStore.get().battleMatCardFraming).toMatchObject({ span: 0.5, cx: 0.5, cy: 0.5 })
    await user.click(screen.getByRole("button", { name: "改回主戰者" }))
    expect([prefsStore.get().battleMatCard, prefsStore.get().battleMatCardFraming]).toEqual([
      null,
      null,
    ])
  })

  it("frames a chosen picture before keeping it, and lets the player back out", async () => {
    prefsStore.set({ battleMatLook: "upload" })
    const user = await open(3)
    expect(screen.getByText("這個設定視窗開著時，按 Ctrl+V 也可以貼上。")).toBeInTheDocument()
    await user.upload(
      screen.getByLabelText("選擇圖片", { selector: "input" }),
      new File(["picture"], "mat.png", { type: "image/png" }),
    )
    // Nothing is kept until the player says so.
    expect(screen.getByRole("dialog", { name: "選擇取景" })).toBeInTheDocument()
    expect(document.querySelector(".bp-crop-stage img.bp-crop-whole")).not.toBeNull()
    expect(screen.getByRole("slider", { name: "放大" })).toBeInTheDocument()
    // Until the picture has loaded there is nothing to frame.
    expect(screen.getByRole("button", { name: "使用" })).toBeDisabled()
    expect(document.querySelector(".bp-crop-frame")).toBeNull()
    await user.click(screen.getByRole("button", { name: "取消" }))
    expect(screen.getByRole("dialog", { name: "對戰設定" })).toBeInTheDocument()
    expect(screen.getByText("還沒有圖片")).toBeInTheDocument()
  })

  it("draws the frame on the whole picture in the mat's shape, and greys what it leaves out", async () => {
    prefsStore.set({ battleMatLook: "upload" })
    const user = await open(2)
    await user.upload(
      screen.getByLabelText("選擇圖片", { selector: "input" }),
      new File(["picture"], "mat.png", { type: "image/png" }),
    )
    const whole = document.querySelector<HTMLImageElement>("img.bp-crop-whole")
    if (whole === null) throw new Error("no picture to frame")
    // A square picture, 1000 × 1000: a 2:1 frame takes its whole width and the middle half.
    Object.defineProperties(whole, {
      naturalWidth: { value: 1000 },
      naturalHeight: { value: 1000 },
    })
    fireEvent.load(whole)
    const frame = document.querySelector<HTMLElement>(".bp-crop-frame")
    expect([frame?.style.left, frame?.style.top, frame?.style.right, frame?.style.bottom]).toEqual([
      "0%",
      "25%",
      "0%",
      "25%",
    ])
    // The full-colour copy shows through the frame only; the picture under it is the grey one.
    expect(document.querySelector<HTMLElement>("img.bp-crop-kept")?.style.clipPath).toBe(
      // jsdom writes the four sides in their shortest form.
      "inset(25% 0%)",
    )
    // A smaller frame, still around the middle.
    fireEvent.change(screen.getByRole("slider", { name: "放大" }), { target: { value: "2" } })
    expect([frame?.style.left, frame?.style.top]).toEqual(["25%", "37.5%"])
    expect(screen.getByRole("button", { name: "使用" })).toBeEnabled()
  })

  it("takes a picture pasted while the settings are open, and nothing else that is pasted", async () => {
    await open(2)
    const paste = (items: readonly { type: string; getAsFile: () => File | null }[]) => {
      const event = new Event("paste", { bubbles: true, cancelable: true })
      Object.defineProperty(event, "clipboardData", { value: { items } })
      fireEvent(document, event)
    }
    // Text is not a picture: the settings stay as they are.
    paste([{ type: "text/plain", getAsFile: () => null }])
    expect(screen.getByRole("dialog", { name: "對戰設定" })).toBeInTheDocument()
    expect(prefsStore.get().battleMatLook).toBe("night")
    paste([
      { type: "text/html", getAsFile: () => null },
      {
        type: "image/png",
        getAsFile: () => new File(["pasted"], "image.png", { type: "image/png" }),
      },
    ])
    // A picture is taken as the player's own and goes straight to framing.
    expect(await screen.findByRole("dialog", { name: "選擇取景" })).toBeInTheDocument()
    expect(prefsStore.get().battleMatLook).toBe("upload")
  })

  it("reads a picture from the clipboard at the press of a button, and says when it cannot", async () => {
    prefsStore.set({ battleMatLook: "upload" })
    const user = await open(2)
    const clipboard = (read: () => Promise<ClipboardItems>) => {
      Object.defineProperty(navigator, "clipboard", { value: { read }, configurable: true })
    }
    const button = screen.getByRole("button", { name: "從剪貼簿貼上" })
    // Nothing but text on the clipboard.
    clipboard(() =>
      Promise.resolve([
        { types: ["text/plain"], getType: () => Promise.resolve(new Blob(["x"])) },
      ] as unknown as ClipboardItems),
    )
    await user.click(button)
    expect(await screen.findByText("剪貼簿裡沒有圖片。")).toBeInTheDocument()
    // No permission, or a browser that cannot read pictures: the other ways are named.
    clipboard(() => Promise.reject(new Error("denied")))
    await user.click(button)
    expect(
      await screen.findByText("讀不到剪貼簿。請在這個視窗按 Ctrl+V，或用「選擇圖片」。"),
    ).toBeInTheDocument()
    clipboard(() =>
      Promise.resolve([
        {
          types: ["text/html", "image/png"],
          getType: () => Promise.resolve(new Blob(["picture"], { type: "image/png" })),
        },
      ] as unknown as ClipboardItems),
    )
    await user.click(button)
    expect(await screen.findByRole("dialog", { name: "選擇取景" })).toBeInTheDocument()
  })

  it("drops what the clipboard brings back after the settings were closed", async () => {
    prefsStore.set({ battleMatLook: "upload" })
    const made = vi.spyOn(URL, "createObjectURL")
    const user = await open(2)
    // The clipboard answers late: only after the settings are closed.
    let answer: (items: ClipboardItems) => void = () => undefined
    Object.defineProperty(navigator, "clipboard", {
      value: { read: () => new Promise<ClipboardItems>((resolve) => (answer = resolve)) },
      configurable: true,
    })
    await user.click(screen.getByRole("button", { name: "從剪貼簿貼上" }))
    dismiss()
    expect(screen.queryByRole("dialog")).toBeNull()
    await act(async () => {
      answer([
        {
          types: ["image/png"],
          getType: () => Promise.resolve(new Blob(["late"], { type: "image/png" })),
        },
      ] as unknown as ClipboardItems)
      await Promise.resolve()
    })
    // Nothing was made of the late picture, and opening the settings again shows the settings.
    expect(made).not.toHaveBeenCalled()
    await user.click(screen.getByRole("button", { name: "對戰設定" }))
    expect(screen.getByRole("dialog", { name: "對戰設定" })).toBeInTheDocument()
    expect(document.querySelector(".bp-crop")).toBeNull()
  })

  it("drops a late picture when the settings closed between reading the clipboard and its picture", async () => {
    prefsStore.set({ battleMatLook: "upload" })
    const made = vi.spyOn(URL, "createObjectURL")
    const user = await open(2)
    let deliver: (picture: Blob) => void = () => undefined
    Object.defineProperty(navigator, "clipboard", {
      value: {
        read: () =>
          Promise.resolve([
            {
              types: ["image/png"],
              getType: () => new Promise<Blob>((resolve) => (deliver = resolve)),
            },
          ] as unknown as ClipboardItems),
      },
      configurable: true,
    })
    await user.click(screen.getByRole("button", { name: "從剪貼簿貼上" }))
    dismiss()
    await act(async () => {
      deliver(new Blob(["late"], { type: "image/png" }))
      await Promise.resolve()
    })
    expect(made).not.toHaveBeenCalled()
    // A new opening is a new cycle: the same button works again.
    await user.click(screen.getByRole("button", { name: "對戰設定" }))
    expect(screen.getByRole("dialog", { name: "對戰設定" })).toBeInTheDocument()
  })

  it("gives the framed picture's address back when the settings close or go away", async () => {
    prefsStore.set({ battleMatLook: "upload" })
    let made = 0
    vi.spyOn(URL, "createObjectURL").mockImplementation(() => `blob:framing-${String(++made)}`)
    const freed = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined)
    const user = userEvent.setup()
    const { unmount } = await renderRoutes([
      {
        path: "*",
        element: (
          <BattleCardsContext value={CARDS}>
            <SettingsButton leader={{ printing_id: "p:leader", face: 0 }} matAspect={2} />
          </BattleCardsContext>
        ),
      },
    ])
    const choose = async () => {
      await user.click(screen.getByRole("button", { name: "對戰設定" }))
      await user.upload(
        screen.getByLabelText("選擇圖片", { selector: "input" }),
        new File(["picture"], "mat.png", { type: "image/png" }),
      )
    }
    await choose()
    dismiss()
    expect(freed).toHaveBeenCalledWith("blob:framing-1")
    // Still framing when the screen goes away.
    await choose()
    expect(freed).not.toHaveBeenCalledWith("blob:framing-2")
    unmount()
    expect(freed).toHaveBeenCalledWith("blob:framing-2")
  })
})
