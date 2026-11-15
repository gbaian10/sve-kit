import { snapshotBase, type SnapshotRoot } from "./roots"

/**
 * The official card back. Like the card images it is published beside a snapshot root and never
 * kept in the repository; a root without it answers 404 and the caller draws a back of its own.
 */
export function cardBackUrl(root?: SnapshotRoot): string {
  return `${snapshotBase(root)}/images/card_back.webp`
}
