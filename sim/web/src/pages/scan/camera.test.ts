import { afterEach, describe, expect, it, vi } from "vitest"

import { CameraScanner, CameraStopped } from "./camera"
import type { ScanEngine } from "./engine"

const engine = { index: { dim: 4 } } as unknown as ScanEngine

/** A camera whose permission prompt answers when the test says so. */
function fakeCamera() {
  const stops: number[] = []
  let grant: (() => void) | undefined
  const track = {
    stop: () => {
      stops.push(1)
    },
    applyConstraints: () => Promise.resolve(),
  }
  const stream = {
    getTracks: () => [track],
    getVideoTracks: () => [track],
  } as unknown as MediaStream
  // jsdom has no camera API; give this navigator one for the test.
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: {
      getUserMedia: () =>
        new Promise<MediaStream>((resolve) => {
          grant = () => {
            resolve(stream)
          }
        }),
    },
  })
  return {
    stops,
    stream,
    grant: () => {
      grant?.()
    },
  }
}

function attached(play: () => Promise<void> = () => Promise.resolve()) {
  const video = document.createElement("video")
  Object.defineProperty(video, "play", { value: play })
  const scanner = new CameraScanner(engine)
  scanner.attach(video, document.createElement("div"), document.createElement("canvas"))
  return { video, scanner }
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe("camera scanner", () => {
  it("closes a camera granted after the page already stopped it", async () => {
    const camera = fakeCamera()
    const { video, scanner } = attached()
    const starting = scanner.start()
    scanner.stop()
    camera.grant()
    await expect(starting).rejects.toBeInstanceOf(CameraStopped)
    expect(camera.stops).toHaveLength(1)
    // jsdom leaves srcObject undefined until something sets it; the late camera never shows.
    expect(video.srcObject ?? null).toBeNull()
  })

  it("closes the camera when the picture cannot play", async () => {
    const camera = fakeCamera()
    const { video, scanner } = attached(() => Promise.reject(new Error("NotAllowedError")))
    const starting = scanner.start()
    camera.grant()
    await expect(starting).rejects.toThrow("NotAllowedError")
    expect(camera.stops).toHaveLength(1)
    expect(video.srcObject).toBeNull()
  })

  it("shows the camera once started and closes it on stop", async () => {
    const camera = fakeCamera()
    const { video, scanner } = attached()
    const starting = scanner.start()
    camera.grant()
    expect(await starting).toBe(4)
    expect(video.srcObject).toBe(camera.stream)
    scanner.stop()
    expect(camera.stops).toHaveLength(1)
    expect(video.srcObject).toBeNull()
  })
})
