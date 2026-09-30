// @vitest-environment node
import { describe, expect, it } from "vitest"

import { imageSource } from "./images"

const variant = (size_key: string, width: number, height: number) => ({
  image_id: "img:a",
  size_key,
  format: "webp",
  path: `images/sha256/aa/${size_key}.webp`,
  width,
  height,
  bytes: 1,
})
const rows = [
  variant("card_l", 459, 641),
  variant("art_s", 160, 120),
  variant("card_s", 128, 179),
  variant("art_m", 384, 288),
]

describe("imageSource", () => {
  it("builds the card srcset by default and the art srcset on request", () => {
    expect(imageSource("/cdn", rows)).toEqual({
      src: "/cdn/images/sha256/aa/card_l.webp",
      srcSet: "/cdn/images/sha256/aa/card_s.webp 128w, /cdn/images/sha256/aa/card_l.webp 459w",
      width: 459,
      height: 641,
    })
    expect(imageSource("/cdn", rows, ["art_s", "art_m"])).toEqual({
      src: "/cdn/images/sha256/aa/art_m.webp",
      srcSet: "/cdn/images/sha256/aa/art_s.webp 160w, /cdn/images/sha256/aa/art_m.webp 384w",
      width: 384,
      height: 288,
    })
    expect(imageSource("/cdn", rows.slice(0, 1), ["art_s", "art_m"])).toBeUndefined()
  })
})
