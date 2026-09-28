import { cn } from "./cn"

// Design §8: skeletons pulse slowly (1.6s); reduced motion turns it off.
export function Skeleton({ className }: { readonly className?: string }) {
  return (
    <div
      aria-hidden="true"
      className={cn(
        "rounded-card bg-surface-2 motion-safe:animate-pulse motion-safe:[animation-duration:1.6s]",
        className,
      )}
    />
  )
}
