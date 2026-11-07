import { vi } from "vitest"

/**
 * jsdom has no 2D canvas: a stand-in context that draws nothing and reads back blank pixels, plus
 * the image decoding the scanner uses. Restore with `vi.restoreAllMocks()` and `vi.unstubAllGlobals()`.
 */
export function stubCanvas2d(): void {
  const methods: Record<string, unknown> = {
    getImageData: (_x: number, _y: number, width: number, height: number) => ({
      width,
      height,
      data: new Uint8ClampedArray(Math.max(0, width * height * 4) || 0),
    }),
    measureText: () => ({ width: 10 }),
  }
  const context = new Proxy(methods, {
    get: (target, key: string): unknown => target[key] ?? (() => undefined),
    set: () => true,
  })
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
    context as unknown as CanvasRenderingContext2D,
  )
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/png;base64,")
  vi.stubGlobal("createImageBitmap", () => Promise.resolve({ width: 800, height: 600 }))
  vi.stubGlobal(
    "ImageData",
    class {
      readonly data: Uint8ClampedArray
      readonly width: number
      readonly height: number
      constructor(data: Uint8ClampedArray, width: number, height: number) {
        this.data = data
        this.width = width
        this.height = height
      }
    },
  )
}
