import { snapshotBase, type SnapshotRoot } from "./roots"

// Official artwork the interface shows that is not a card image. Like the card images it is
// published beside a snapshot root and never kept in the repository; this file is the one place
// that names it. A root without a file answers 404 and the caller draws something of its own.
//
// It lives under `ui/`, not `images/`: publishing treats any file under `images/` that the card
// image index does not list as removable.

/** The official card back. */
export function cardBackUrl(root?: SnapshotRoot): string {
  return `${snapshotBase(root)}/ui/card_back.webp`
}

/** A counter chip: `b` is the face a card's chip shows, `a` the one a second kind would show. */
export function counterChipUrl(side: "a" | "b", root?: SnapshotRoot): string {
  return `${snapshotBase(root)}/ui/counter_chip_${side}.webp`
}
