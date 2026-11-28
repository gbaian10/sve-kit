import type { ReactNode } from "react"

import type { VIEW_MODES } from "../../domain/query/model"

// Solid blocks close to the official card list's switch: a 3×3 grid, a table with a header row,
// and three bars. They fill with currentColor so the segmented control's colours apply.
const PATHS: Record<(typeof VIEW_MODES)[number], ReactNode> = {
  grid: [1, 6.8, 12.6].flatMap((y) =>
    [1, 6.8, 12.6].map((x) => (
      <rect key={`${String(x)}-${String(y)}`} x={x} y={y} width="4.4" height="4.4" rx=".6" />
    )),
  ),
  table: (
    <>
      <rect x="1" y="1.5" width="16" height="3.4" rx=".6" />
      {[6.2, 9.9, 13.6].map((y) => (
        <g key={y}>
          <rect x="1" y={y} width="4.5" height="2.6" rx=".4" />
          <rect x="6.8" y={y} width="10.2" height="2.6" rx=".4" />
        </g>
      ))}
    </>
  ),
  list: [2, 7.4, 12.8].map((y) => <rect key={y} x="1" y={y} width="16" height="3.2" rx=".6" />),
}

/** The results view's icon; decorative, so the control carries the name. */
export function ViewIcon({ view }: { readonly view: (typeof VIEW_MODES)[number] }) {
  return (
    <svg aria-hidden="true" viewBox="0 0 18 18" fill="currentColor" className="size-4.5">
      {PATHS[view]}
    </svg>
  )
}
