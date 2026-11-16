import { snapshotBase, type SnapshotRoot } from "./roots"

/**
 * The official card back. Like the card images it is published beside a snapshot root and never
 * kept in the repository; a root without it answers 404 and the caller draws a back of its own.
 *
 * It lives under `ui/`, not `images/`: publishing treats any file under `images/` that the card
 * image index does not list as removable.
 */
export function cardBackUrl(root?: SnapshotRoot): string {
  return `${snapshotBase(root)}/ui/card_back.webp`
}
