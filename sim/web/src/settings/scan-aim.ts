// How large the single-card aiming frame is on this device, as a share of the largest frame that
// fits. A smaller frame keeps neighbouring cards out of the finder's view.

const KEY = "sve-kit:scan-aim"
/** The smallest frame offered: still large enough for the finder to see a card's corners. */
export const MIN_AIM_SCALE = 0.4

/** Within the offered range, in whole percent so repeated key steps do not drift. */
export function clampAimScale(scale: number): number {
  return Math.min(1, Math.max(MIN_AIM_SCALE, Math.round(scale * 100) / 100))
}

/** The saved frame size, or the full frame. Storage can be blocked, so every access is guarded. */
export function savedAimScale(): number {
  try {
    const raw = Number(localStorage.getItem(KEY))
    return raw > 0 ? clampAimScale(raw) : 1
  } catch {
    return 1
  }
}

export function saveAimScale(scale: number): void {
  try {
    localStorage.setItem(KEY, String(clampAimScale(scale)))
  } catch {
    // Without storage the frame is full size again on the next visit.
  }
}
