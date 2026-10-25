import { act, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { useImageIndex } from "../../app/snapshot"
import type { CardSummary, ImageIndex } from "../../data"
import { renderInRouter } from "../../test-utils"
import { imageSnapshot } from "../../test-utils/image-snapshot"
import { CardImage } from "./CardImage"

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
          small: "/cdn/a-small.webp",
          width: 320,
          height: 447,
        }
      : undefined,
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe("CardImage", () => {
  it("shows the card_s file in a small slot and the card_l file in a large one", async () => {
    const index = images("available")
    const { rerender } = await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={index}
        alt="redundant"
        size="small"
        className="w-8"
      />,
    )
    expect(document.querySelector("img")).toHaveAttribute("src", "/cdn/a-small.webp")
    rerender(
      <CardImage
        summary={summary}
        name={name}
        images={index}
        alt="redundant"
        size="large"
        className="w-8"
      />,
    )
    expect(document.querySelector("img")).toHaveAttribute("src", "/cdn/a.webp")
  })

  it("returns to a decoded face with its URL and no loading placeholder in the same render", async () => {
    vi.stubGlobal("IntersectionObserver", undefined)
    const { client } = imageSnapshot(2)
    const read = vi.spyOn(client, "fragments")
    function Viewer({ id }: { readonly id: number }) {
      const index = useImageIndex(client, true)
      return (
        <CardImage
          summary={{ ...summary, printingId: `p:${String(id)}`, faceId: `f:${String(id)}` }}
          name={name}
          images={index}
          alt="redundant"
          size="large"
          className="w-full"
        />
      )
    }
    const { rerender } = await renderInRouter(<Viewer id={0} />)
    await waitFor(() => {
      expect(document.querySelector("img")).not.toBeNull()
    })
    rerender(<Viewer id={1} />)
    await waitFor(() =>
      expect(document.querySelector("img")).toHaveAttribute("src", "/cdn/images/card_l/2.webp?v=1"),
    )
    const reads = read.mock.calls.length
    rerender(<Viewer id={0} />)
    expect(document.querySelector("img")).toHaveAttribute("src", "/cdn/images/card_l/1.webp?v=1")
    expect(screen.queryByText("卡圖資訊載入中")).not.toBeInTheDocument()
    expect(read).toHaveBeenCalledTimes(reads)
  })

  it("shows an error only on the failed face while another face remains pending", async () => {
    const failedIndex: ImageIndex = {
      asset: () => undefined,
      cardImage: () => undefined,
      known: (printingId) => printingId === "failed",
      failed: true,
      failedFace: (printingId) => printingId === "failed",
    }
    const props = { summary, name, size: "large" as const, className: "w-full" }
    const { rerender } = await renderInRouter(
      <CardImage {...props} alt="redundant" images={failedIndex} />,
    )
    // The placeholder tag and the screen-reader label both carry the state text.
    expect(screen.getAllByText("卡圖資訊載入中").length).toBeGreaterThan(0)
    expect(screen.queryByText("卡圖資訊載入失敗")).not.toBeInTheDocument()
    rerender(
      <CardImage
        {...props}
        summary={{ ...summary, printingId: "failed" }}
        alt="redundant"
        images={failedIndex}
      />,
    )
    expect(screen.getAllByText("卡圖資訊載入失敗").length).toBeGreaterThan(0)
    expect(screen.queryByText("卡圖資訊載入中")).not.toBeInTheDocument()
  })
  it("removes a loaded old-v image once unpublished and shows failure instead of falling back to it", async () => {
    const old = images("available")
    const props = { summary, name, size: "large" as const, className: "w-full" }
    const { rerender } = await renderInRouter(<CardImage {...props} alt="redundant" images={old} />)
    const oldImage = document.querySelector("img")
    if (!oldImage) throw new Error("missing image")
    act(() => {
      oldImage.dispatchEvent(new Event("load"))
    })
    rerender(<CardImage {...props} alt="redundant" images={images("available", "pending")} />)
    expect(document.querySelector("img")).toBeNull()
    const changed: ImageIndex = {
      asset: old.asset,
      eager: () => true,
      cardImage: () => ({
        src: "/cdn/images/card_l/1.webp?v=2",
        width: 459,
        height: 641,
      }),
    }
    rerender(<CardImage {...props} alt="redundant" images={changed} />)
    const replacement = document.querySelector("img")
    expect(replacement).toHaveAttribute("loading", "eager")
    act(() => {
      oldImage.dispatchEvent(new Event("load"))
    })
    expect(replacement).toHaveClass("opacity-0")
    act(() => {
      replacement?.dispatchEvent(new Event("error"))
    })
    expect(document.querySelector("img")).toBeNull()
    expect(screen.getByText("卡圖載入失敗", { selector: ".sr-only" })).toBeInTheDocument()
  })
  it("names the card in identify mode and stays silent in redundant mode", async () => {
    await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("available")}
        alt="identify"
        size="large"
        className="w-full"
      />,
    )
    expect(screen.getByRole("img", { name: "試作見習兵（BP01-001）" })).toBeInTheDocument()
    const img = document.querySelector("img")
    if (!img) throw new Error("no image")
    expect(img).toHaveAttribute("alt", "")
    expect(img).toHaveAttribute("src", "/cdn/a.webp")
    expect(img).not.toHaveAttribute("srcset")
    expect(img).toHaveAttribute("loading", "lazy")
  })

  it("keeps the placeholder until the image has loaded, then hides it", async () => {
    // A source never shown in this session: shown ones skip the placeholder on remount.
    const fresh: ImageIndex = {
      asset: () => ({ id: "img", availability: "available", publication_state: "approved" }),
      cardImage: () => ({
        src: "/cdn/fresh.webp",
        width: 320,
        height: 447,
      }),
    }
    await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={fresh}
        alt="redundant"
        size="large"
        className="w-full"
      />,
    )
    const img = document.querySelector("img")
    if (!img) throw new Error("no image")
    expect(img).toHaveAttribute("alt", "")
    expect(screen.getByText("卡圖載入中", { selector: ".sr-only" })).toBeInTheDocument()
    expect(document.querySelector("[aria-hidden] svg")).not.toBeNull()
    act(() => {
      img.dispatchEvent(new Event("load"))
    })
    expect(document.querySelector("[aria-hidden] svg")).toBeNull()
    expect(screen.queryByText("卡圖載入中")).not.toBeInTheDocument()
  })

  it("keeps a replacement image behind the placeholder until the new source loads", async () => {
    const { rerender } = await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("available")}
        alt="redundant"
        size="large"
        className="w-full"
      />,
    )
    const original = document.querySelector("img")
    if (!original) throw new Error("no image")
    act(() => {
      original.dispatchEvent(new Event("load"))
    })
    expect(document.querySelector("[aria-hidden] svg")).toBeNull()
    const changed: ImageIndex = {
      asset: images("available").asset,
      cardImage: () => ({
        src: "/cdn/new.webp",
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
        size="large"
        className="w-full"
      />,
    )
    expect(document.querySelector("[aria-hidden] svg")).not.toBeNull()
    const replacement = document.querySelector("img")
    if (!replacement) throw new Error("no replacement")
    act(() => {
      replacement.dispatchEvent(new Event("load"))
    })
    expect(document.querySelector("[aria-hidden] svg")).toBeNull()
  })

  it("shows the placeholder with a reason for missing and pending images", async () => {
    const { unmount } = await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("missing")}
        alt="redundant"
        size="large"
        className="w-full"
      />,
    )
    expect(screen.getByText("尚無卡圖", { selector: ".sr-only" })).toBeInTheDocument()
    unmount()
    const pending = await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("unfetched", "pending")}
        alt="redundant"
        size="large"
        className="w-full"
      />,
    )
    expect(screen.getByText("卡圖待確認", { selector: ".sr-only" })).toBeInTheDocument()
    expect(document.querySelector("img")).toBeNull()
    pending.unmount()
  })

  it("keeps a landscape image whole instead of cropping it to the portrait slot", async () => {
    const landscape: ImageIndex = {
      asset: images("available").asset,
      cardImage: () => ({
        src: "/cdn/wide.webp",
        width: 641,
        height: 459,
      }),
    }
    await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={landscape}
        alt="redundant"
        size="large"
        className="w-full"
      />,
    )
    expect(document.querySelector("img")).toHaveClass("object-contain")
  })

  it("shows the shared placeholder, never the text card, in every non-image state", async () => {
    const states: [ImageIndex | undefined, string][] = [
      [undefined, "卡圖資訊載入中"],
      [images("missing"), "尚無卡圖"],
      [images("unfetched", "pending"), "卡圖待確認"],
      [images("unfetched"), "卡圖未抓取"],
      [
        {
          ...images("available"),
          asset: () => undefined,
          cardImage: () => undefined,
          failed: true,
        },
        "卡圖資訊載入失敗",
      ],
    ]
    for (const [index, tag] of states) {
      const { unmount } = await renderInRouter(
        <CardImage
          summary={summary}
          name={name}
          images={index}
          alt="redundant"
          size="large"
          className="w-full"
        />,
      )
      expect(screen.getAllByText(tag)).toHaveLength(2)
      expect(screen.getByText(tag, { selector: ".sr-only" })).not.toHaveAttribute("aria-hidden")
      expect(document.querySelector("[aria-hidden] svg")).not.toBeNull()
      expect(screen.queryByText("BP01-001")).not.toBeInTheDocument()
      unmount()
    }
  })

  it("names the card in identify mode even without a visible image", async () => {
    await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={images("missing")}
        alt="identify"
        size="large"
        className="w-full"
      />,
    )
    expect(screen.getByRole("img", { name: "試作見習兵（BP01-001）" })).toBeInTheDocument()
  })

  it("shows the placeholder while the image index is still loading", async () => {
    await renderInRouter(
      <CardImage
        summary={summary}
        name={name}
        images={undefined}
        alt="redundant"
        size="large"
        className="w-full"
      />,
    )
    expect(screen.getByText("卡圖資訊載入中", { selector: ".sr-only" })).toBeInTheDocument()
    expect(document.querySelector("img")).toBeNull()
  })
})

it("labels only the failed face while untouched faces remain loading", async () => {
  const scoped = {
    ...images("available"),
    asset: () => undefined,
    cardImage: () => undefined,
    failed: true,
    known: (printing: string) => printing === summary.printingId,
    failedFace: (printing: string) => printing === summary.printingId,
  }
  const { rerender } = await renderInRouter(
    <CardImage
      summary={summary}
      name={name}
      images={scoped}
      alt="redundant"
      size="large"
      className="w-full"
    />,
  )
  expect(screen.getByText("卡圖資訊載入失敗", { selector: ".sr-only" })).toBeInTheDocument()
  rerender(
    <CardImage
      summary={{ ...summary, printingId: "p:unrequested" }}
      name={name}
      images={scoped}
      alt="redundant"
      size="large"
      className="w-full"
    />,
  )
  expect(screen.queryByText("卡圖資訊載入失敗")).not.toBeInTheDocument()
  expect(screen.getByText("卡圖資訊載入中", { selector: ".sr-only" })).toBeInTheDocument()
  rerender(
    <CardImage
      summary={{ ...summary, printingId: "p:known-empty" }}
      name={name}
      images={{ ...scoped, known: () => true }}
      alt="redundant"
      size="large"
      className="w-full"
    />,
  )
  expect(screen.queryByText("卡圖資訊載入失敗")).not.toBeInTheDocument()
})
