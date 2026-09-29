// First-load skeleton: text-card shaped boxes pulsing slowly (design: `pulse` 1.6s).
export function SkeletonGrid({ count = 12 }: { readonly count?: number }) {
  return (
    <ul
      aria-hidden="true"
      className="grid grid-cols-2 gap-x-3 gap-y-3.5 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6"
    >
      {Array.from({ length: count }, (_, index) => (
        <li key={index} className="flex flex-col gap-1.5">
          <span className="block aspect-[63/88] w-full rounded-card border border-border bg-surface-2 motion-safe:animate-pulse" />
          <span className="block h-3.5 w-3/4 rounded-sm bg-surface-2 motion-safe:animate-pulse" />
        </li>
      ))}
    </ul>
  )
}
