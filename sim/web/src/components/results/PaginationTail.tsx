import { useEffect, useRef } from "react"
import { useTranslation } from "react-i18next"

import { Button } from "../ui/Button"

interface PaginationTailProps {
  readonly automatic?: boolean
  readonly requestKey: string
  readonly total: number
  readonly hasMore: boolean
  readonly loading: boolean
  readonly failed: boolean
  readonly onMore: () => void
  readonly onRetry: () => void
}

/** One paging trigger for both the page viewport and the narrow list's scrollport. */
export function PaginationTail({
  requestKey,
  automatic = true,
  total,
  hasMore,
  loading,
  failed,
  onMore,
  onRetry,
}: PaginationTailProps) {
  const { t } = useTranslation()
  const sentinel = useRef<HTMLDivElement>(null)
  const requested = useRef<string | null>(null)
  useEffect(() => {
    const node = sentinel.current
    if (
      !node ||
      !automatic ||
      !hasMore ||
      loading ||
      failed ||
      typeof IntersectionObserver === "undefined"
    )
      return
    let observer: IntersectionObserver | undefined
    let active = true
    const observe = () => {
      observer?.disconnect()
      const container = node.closest<HTMLElement>("[data-list-scroll]")
      const root = container && getComputedStyle(container).overflowY === "auto" ? container : null
      observer = new IntersectionObserver(
        (entries) => {
          if (
            !active ||
            !entries.some((entry) => entry.isIntersecting) ||
            requested.current === requestKey
          )
            return
          requested.current = requestKey
          onMore()
        },
        { root, rootMargin: `0px 0px ${String(root?.clientHeight ?? window.innerHeight)}px 0px` },
      )
      observer.observe(node)
    }
    observe()
    window.addEventListener("resize", observe)
    return () => {
      active = false
      observer?.disconnect()
      window.removeEventListener("resize", observe)
    }
  }, [requestKey, automatic, hasMore, loading, failed, onMore])
  return (
    <div
      ref={sentinel}
      data-results-sentinel=""
      className="sticky left-0 flex h-14 shrink-0 items-center justify-center gap-2 text-13 text-text-2"
      aria-live="polite"
    >
      {failed ? (
        <>
          <span>{t("results.loadFailed")}</span>
          <Button className="h-8" onClick={onRetry}>
            {t("results.retry")}
          </Button>
        </>
      ) : loading ? (
        <span role="status">{t("results.loadingMore")}</span>
      ) : !hasMore ? (
        <span>{t("results.allLoaded", { count: total })}</span>
      ) : !automatic ? (
        <Button size="sm" onClick={onMore}>
          {t("results.loadMore")}
        </Button>
      ) : null}
    </div>
  )
}
