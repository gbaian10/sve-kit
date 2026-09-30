import { act, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
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
    withdrawal_reason: "rights holder request",
    source_url: "https://example.invalid/x.png",
  }),
  artImage: () => undefined,
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

// Two printings through one slot, as a results cell keyed by card id does after a filter change.
function TwoPrintings() {
  const [second, setSecond] = useState(false)
  const index: ImageIndex = {
    asset: () => ({ id: "img", availability: "available", publication_state: "approved" }),
    artImage: () => undefined,
    cardImage: (printingId) => ({
      src: `/cdn/${printingId}.webp`,
      srcSet: `/cdn/${printingId}.webp 320w`,
      width: 320,
      height: 447,
    }),
  }
  return (
    <>
      <button
        type="button"
        onClick={() => {
          setSecond(true)
        }}
      >
        next
      </button>
      <CardImage
        summary={second ? { ...summary, printingId: "p:bp01-002", cardNo: "BP01-002" } : summary}
        name={name}
        images={index}
        alt="redundant"
        sizes="50vw"
        className="w-full"
      />
    </>
  )
}

const withArt: ImageIndex = {
  asset: () => ({ id: "img", availability: "available", publication_state: "approved" }),
  cardImage: () => ({
    src: "/cdn/card.webp",
    srcSet: "/cdn/card.webp 320w",
    width: 320,
    height: 447,
  }),
  artImage: () => ({ src: "/cdn/art.webp", srcSet: "/cdn/art.webp 160w", width: 160, height: 120 }),
}

describe("CardImage", () => {
  it("shows the art crop in a 4:3 slot and falls back to the whole card when it fails", async () => {
    await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={withArt}
        alt="redundant"
        sizes="64px"
        variant="art"
        className="w-16"
      />,
    )
    const art = document.querySelector("img[srcset]")
    if (!art) throw new Error("no image")
    expect(art).toHaveAttribute("src", "/cdn/art.webp")
    expect(art.parentElement).toHaveClass("aspect-[4/3]")
    // No text card behind an art crop: it is drawn for the portrait card.
    expect(screen.queryByText("BP01-001")).not.toBeInTheDocument()
    act(() => {
      art.dispatchEvent(new Event("error"))
    })
    const card = document.querySelector("img[srcset]")
    expect(card).toHaveAttribute("src", "/cdn/card.webp")
    expect(card?.parentElement).toHaveClass("aspect-[63/88]")
    expect(screen.getByText("BP01-001", { selector: "[aria-hidden] *" })).toBeInTheDocument()
  })

  it("tries the image of another printing after one failed in the same slot", async () => {
    await renderInRouter(<TwoPrintings />)
    const user = userEvent.setup()
    const img = document.querySelector("img[srcset]")
    if (!img) throw new Error("no image")
    act(() => {
      img.dispatchEvent(new Event("error"))
    })
    expect(screen.getByText("尚無卡圖")).toBeInTheDocument()
    expect(document.querySelector("img[srcset]")).toBeNull()
    await user.click(screen.getByRole("button", { name: "next" }))
    expect(document.querySelector("img[srcset]")).toHaveAttribute("src", "/cdn/p:bp01-002.webp")
    expect(screen.queryByText("尚無卡圖")).not.toBeInTheDocument()
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

  it("shows the text card with a reason for missing, pending and withdrawn images", async () => {
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
    pending.unmount()
    await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("available", "withdrawn")}
        alt="redundant"
        sizes="50vw"
        className="w-full"
      />,
    )
    expect(
      screen.getByText("卡圖已撤下：rights holder request（來源 example.invalid）"),
    ).toBeInTheDocument()
    expect(document.querySelector("img[srcset]")).toBeNull()
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
