import { act, screen } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"

import type { CardSummary, ImageIndex } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { CardImage } from "./CardImage"
import { imageHeldBack } from "./image-state"

const summary: CardSummary = {
  cardId: "c:bp01-001",
  printingId: "p:bp01-001",
  faceId: "f:bp01-001",
  cardNo: "BP01-001",
  region: "jp",
  classCode: "royal",
  name: {
    original: { lang: "ja", text: "試作の見習い兵" },
    translations: { "zh-Hant": "試作見習兵" },
  },
  cost: 1,
  attack: 1,
  defense: 1,
}
const name = { text: "試作見習兵", lang: "zh-Hant" as const }
const images = (availability: string, publication = "approved"): ImageIndex => ({
  asset: () => ({
    id: "img",
    availability,
    publication_state: publication,
  }),
  cardImage: () =>
    availability === "available"
      ? {
          src: "/cdn/a.webp",
          srcSet: "/cdn/a.webp 128w, /cdn/b.webp 320w",
          width: 320,
          height: 447,
        }
      : undefined,
})

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

describe("CardImage", () => {
  it("removes a loaded old-v image once unpublished and shows failure instead of falling back to it", async () => {
    const old = images("available")
    const props = { summary, name, sizes: "50vw", className: "w-full" }
    const { rerender } = await renderInRouter(<CardImage {...props} alt="redundant" images={old} />)
    const oldImage = document.querySelector("img[srcset]")
    if (!oldImage) throw new Error("missing image")
    act(() => {
      oldImage.dispatchEvent(new Event("load"))
    })
    rerender(<CardImage {...props} alt="redundant" images={images("available", "pending")} />)
    expect(document.querySelector("img[srcset]")).toBeNull()
    const changed: ImageIndex = {
      asset: old.asset,
      eager: () => true,
      cardImage: () => ({
        src: "/cdn/images/card_l/1.webp?v=2",
        srcSet: "/cdn/images/card_l/1.webp?v=2 459w",
        width: 459,
        height: 641,
      }),
    }
    rerender(<CardImage {...props} alt="redundant" images={changed} />)
    const replacement = document.querySelector("img[srcset]")
    expect(replacement).toHaveAttribute("loading", "eager")
    act(() => {
      oldImage.dispatchEvent(new Event("load"))
    })
    expect(replacement).toHaveClass("opacity-0")
    act(() => {
      replacement?.dispatchEvent(new Event("error"))
    })
    expect(document.querySelector("img[srcset]")).toBeNull()
    expect(screen.getByText("卡圖載入失敗")).toBeInTheDocument()
  })
  it("names the card in identify mode and stays silent in redundant mode", async () => {
    await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("available")}
        alt="identify"
        sizes="50vw"
        className="w-full"
      />,
    )
    expect(screen.getByRole("img", { name: "試作見習兵（BP01-001）" })).toBeInTheDocument()
    const img = document.querySelector("img[srcset]")
    if (!img) throw new Error("no image")
    expect(img).toHaveAttribute("alt", "")
    expect(img).toHaveAttribute("srcset", "/cdn/a.webp 128w, /cdn/b.webp 320w")
    expect(img).toHaveAttribute("sizes", "50vw")
    expect(img).toHaveAttribute("loading", "lazy")
  })

  it("keeps the text card until the image has loaded, then hides it", async () => {
    await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("available")}
        alt="redundant"
        sizes="50vw"
        className="w-full"
      />,
    )
    const img = document.querySelector("img[srcset]")
    if (!img) throw new Error("no image")
    expect(img).toHaveAttribute("alt", "")
    expect(screen.getByText("BP01-001", { selector: "[aria-hidden] *" })).toBeInTheDocument()
    act(() => {
      img.dispatchEvent(new Event("load"))
    })
    expect(screen.queryByText("BP01-001")).not.toBeInTheDocument()
  })

  it("keeps a replacement image behind the text card until the new source loads", async () => {
    const { rerender } = await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("available")}
        alt="redundant"
        sizes="50vw"
        className="w-full"
      />,
    )
    const original = document.querySelector("img[srcset]")
    if (!original) throw new Error("no image")
    act(() => {
      original.dispatchEvent(new Event("load"))
    })
    expect(screen.queryByText("BP01-001")).not.toBeInTheDocument()
    const changed: ImageIndex = {
      asset: images("available").asset,
      cardImage: () => ({
        src: "/cdn/new.webp",
        srcSet: "/cdn/new.webp 320w",
        width: 320,
        height: 447,
      }),
    }
    rerender(
      <CardImage
        summary={summary}
        name={name}
        images={changed}
        alt="redundant"
        sizes="50vw"
        className="w-full"
      />,
    )
    expect(screen.getByText("BP01-001", { selector: "[aria-hidden] *" })).toBeInTheDocument()
    const replacement = document.querySelector("img[srcset]")
    if (!replacement) throw new Error("no replacement")
    act(() => {
      replacement.dispatchEvent(new Event("load"))
    })
    expect(screen.queryByText("BP01-001")).not.toBeInTheDocument()
  })

  it("shows the text card with a reason for missing and pending images", async () => {
    const { unmount } = await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("missing")}
        alt="redundant"
        sizes="50vw"
        className="w-full"
      />,
    )
    expect(screen.getByText("尚無卡圖")).toBeInTheDocument()
    unmount()
    const pending = await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("unfetched", "pending")}
        alt="redundant"
        sizes="50vw"
        className="w-full"
      />,
    )
    expect(screen.getByText("卡圖待確認")).toBeInTheDocument()
    expect(document.querySelector("img[srcset]")).toBeNull()
    pending.unmount()
  })

  it("holds the image back in data-saver mode until the caller wants it", async () => {
    prefsStore.set({ dataSaver: true })
    const { rerender } = await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("available")}
        alt="redundant"
        sizes="50vw"
        className="w-full"
      />,
    )
    expect(document.querySelector("img[srcset]")).toBeNull()
    expect(imageHeldBack(images("available"), summary)).toBe(true)
    expect(imageHeldBack(images("missing"), summary)).toBe(false)
    rerender(
      <CardImage
        summary={summary}
        name={name}
        images={images("available")}
        alt="redundant"
        sizes="50vw"
        className="w-full"
        imageWanted
      />,
    )
    expect(document.querySelector("img[srcset]")).not.toBeNull()
  })

  it("names the card in identify mode even without a visible image", async () => {
    await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("missing")}
        alt="identify"
        sizes="50vw"
        className="w-full"
      />,
    )
    expect(screen.getByRole("img", { name: "試作見習兵（BP01-001）" })).toBeInTheDocument()
  })

  it("shows the text card while the image index is still loading", async () => {
    await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={undefined}
        alt="redundant"
        sizes="50vw"
        className="w-full"
      />,
    )
    expect(screen.getByText("試作見習兵")).toBeInTheDocument()
    expect(document.querySelector("img[srcset]")).toBeNull()
  })
})
