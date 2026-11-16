import { describe, expect, it } from "vitest"

import { BATTLE_MAT_LOOKS, type BattleMatLook } from "../../../settings"
import { renderRoutes } from "../../../test-utils"
import { matGeometry } from "./geometry"
import { MatArt, type MatPhoto, ZoneFrames } from "./mats"

const WIDE = matGeometry(1200, false)
const NARROW = matGeometry(640, true)
const BOX = { left: 320, top: 0, width: 1000, height: 800 }
// A card's art (4:3) with its whole width framed for a box the size of the one drawn here.
const CARD: MatPhoto = {
  kind: "card",
  card: { printing_id: "p:test", face: 0 },
  framing: { cx: 0.5, cy: 0.5, span: 1, aspect: 1.25, reach: "table" },
  anchor: BOX,
}
// A 2000 × 1000 picture whose middle half was framed for a box the size of the one drawn here.
const UPLOAD: MatPhoto = {
  kind: "upload",
  url: "blob:mat",
  size: { w: 2000, h: 1000 },
  framing: { cx: 0.5, cy: 0.5, span: 0.5, aspect: 1.25, reach: "table" },
  anchor: BOX,
}

const art = (
  look: BattleMatLook,
  options: { photo?: MatPhoto | null; blur?: number; dim?: number; framed?: boolean } = {},
) =>
  renderRoutes([
    {
      path: "*",
      element: (
        <MatArt
          look={look}
          photo={options.photo ?? null}
          blur={options.blur ?? 0}
          dim={options.dim ?? 0}
          box={BOX}
          u={0.8}
          centre={0.45}
          framed={options.framed ?? false}
        />
      ),
    },
  ])

/** What tells each drawn picture from the others; a photo lies on the plain one. */
const MARK: Record<BattleMatLook, string> = {
  night: ".bp-night-rune",
  blue: ".bp-blue-stop-1",
  forest: ".bp-forest-vine",
  parchment: ".bp-parchment-ring",
  plain: ".bp-mat-plain",
  card: ".bp-mat-plain",
  upload: ".bp-mat-plain",
}

describe("the mat's picture", () => {
  it("draws each look, and no other", async () => {
    for (const look of BATTLE_MAT_LOOKS) {
      const { container, unmount } = await art(look)
      for (const [other, mark] of Object.entries(MARK))
        expect(container.querySelector(mark) !== null, `${look} draws ${other}`).toBe(
          MARK[look] === mark,
        )
      unmount()
    }
  })

  it("fills the box it is given, drawn in board units", async () => {
    const { container } = await art("night")
    const box = container.querySelector(".bp-mat-art")
    expect(box).toHaveStyle({ left: "320px", top: "0px", width: "1000px", height: "800px" })
    // 1000 × 800 px at 0.8 px a unit: details keep the size they have on a mat.
    expect(container.querySelector("svg")).toHaveAttribute("viewBox", "0 0 1250 1000")
    expect(box).not.toHaveClass("is-framed")
  })

  it("is framed like a mat when it lies on the board only", async () => {
    const { container } = await art("blue", { framed: true })
    const box = container.querySelector<HTMLElement>(".bp-mat-art")
    expect(box).toHaveClass("is-framed")
    // The corner of a mat: 18 board units.
    expect(box?.style.borderRadius).toBe("14.4px")
  })

  it("lays a card's art over the mat, as it is when nothing is blurred", async () => {
    const { container } = await art("card", { photo: CARD })
    const photo = container.querySelector<HTMLElement>(".bp-mat-photo")
    expect([photo?.style.inset, photo?.style.filter]).toEqual(["0px", ""])
    // The art keeps its 4:3 shape: as tall as the box, wider than it, centred.
    const picture = photo?.querySelector<HTMLElement>(".bp-mat-img")
    expect(picture?.querySelector(".bp-art")).not.toBeNull()
    expect([picture?.style.height, picture?.style.top]).toEqual(["800px", "0px"])
    expect(Number.parseFloat(picture?.style.width ?? "")).toBeCloseTo((800 * 4) / 3, 1)
    expect(Number.parseFloat(picture?.style.left ?? "")).toBeCloseTo((1000 - (800 * 4) / 3) / 2, 1)
    expect(container.querySelector<HTMLElement>(".bp-mat-veil")?.style.opacity).toBe("0")
    // The plain mat stays under it, for as long as the image is not in.
    expect(container.querySelector(".bp-mat-plain")).not.toBeNull()
  })

  it("blurs and darkens a photo as asked, letting the blur overhang the box", async () => {
    const { container } = await art("upload", { photo: UPLOAD, blur: 4, dim: 45 })
    const photo = container.querySelector<HTMLElement>(".bp-mat-photo")
    const picture = photo?.querySelector<HTMLElement>("img.bp-mat-img")
    expect(picture).toHaveAttribute("src", "blob:mat")
    expect([photo?.style.inset, photo?.style.filter]).toEqual(["-8px", "blur(4px)"])
    // The whole picture, placed so that its framed middle half covers the box: 1000 picture
    // pixels across 1000 screen pixels, measured from the photo's box, which overhangs by 8.
    expect([picture?.style.width, picture?.style.height]).toEqual(["2000px", "1000px"])
    expect([picture?.style.left, picture?.style.top]).toEqual(["-492px", "-92px"])
    // Overhanging on purpose: the overflow audit leaves it alone.
    expect(photo).toHaveAttribute("data-hit-area")
    expect(container.querySelector<HTMLElement>(".bp-mat-veil")?.style.opacity).toBe("0.45")
  })

  it("shows the plain mat where a look's photo is not there", async () => {
    const { container } = await art("upload")
    expect(container.querySelector(".bp-mat-photo")).toBeNull()
    expect(container.querySelector(".bp-mat-veil")).toBeNull()
    expect(container.querySelector(".bp-mat-plain")).not.toBeNull()
  })
})

describe("the mat's printing", () => {
  const frames = (mat = WIDE, opp = false) =>
    renderRoutes([{ path: "*", element: <ZoneFrames opp={opp} mat={mat} /> }])

  it("prints a base under every pile: corner ornaments and the emblem on three of them", async () => {
    const { container } = await frames()
    expect(container.querySelectorAll(".bp-zone-base")).toHaveLength(5)
    const marks = (zone: string) =>
      container.querySelectorAll(`.bp-zone-${zone} .bp-zone-art`).length
    // Four corners and the emblem; the cemetery has its skull and an inner line; banish is bare.
    expect([
      marks("leader"),
      marks("deck"),
      marks("evolve"),
      marks("cemetery"),
      marks("banish"),
    ]).toEqual([5, 5, 5, 1, 0])
    expect(container.querySelectorAll(".bp-zone-inner")).toHaveLength(1)
    // Every base carries its zone's name, as outlines: no text follows the interface language.
    expect(container.querySelectorAll(".bp-zone-ink")).toHaveLength(5)
    expect(container.querySelector("text")).toBeNull()
  })

  it("names the two rows between arrows and marks five slots in each", async () => {
    const { container } = await frames()
    expect(container.querySelectorAll(".bp-print-label")).toHaveLength(2)
    // Two arrows per row name, and the divider's ornament.
    expect(container.querySelectorAll(".bp-print-accent")).toHaveLength(5)
    expect(container.querySelectorAll(".bp-frame-slot")).toHaveLength(10)
  })

  it("scales a pile's base with a small pile, and mirrors the opponent's mat", async () => {
    const wide = await frames()
    const base = (container: HTMLElement) =>
      Number(container.querySelector(".bp-zone-leader .bp-zone-base")?.getAttribute("width"))
    const wideBase = base(wide.container)
    wide.unmount()
    const narrow = await frames(NARROW)
    expect(base(narrow.container)).toBeLessThan(wideBase)
    narrow.unmount()
    const own = await frames(WIDE)
    const x = (container: HTMLElement) =>
      Number(container.querySelector(".bp-zone-leader .bp-zone-base")?.getAttribute("x"))
    const ownX = x(own.container)
    own.unmount()
    const other = await frames(WIDE, true)
    // The leader's base lies at the right of its owner's mat, so at the left of the turned one.
    expect(x(other.container)).toBeLessThan(ownX)
  })
})
