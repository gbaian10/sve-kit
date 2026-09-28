import { type ClassValue, clsx } from "clsx"

/** Class joiner; ESLint and Prettier are configured to scan and sort classes passed to `cn`. */
export function cn(...classes: ClassValue[]): string {
  return clsx(classes)
}
