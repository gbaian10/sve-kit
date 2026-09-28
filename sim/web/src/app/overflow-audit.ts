type OverflowKind = "page" | "nowrap" | "clipped" | "visible"

export interface OverflowProblem {
  readonly kind: OverflowKind
  readonly element: Element
  readonly overflow: number
}

export interface OverflowReport {
  readonly kind: OverflowKind
  readonly tag: string
  readonly className: string
  readonly text: string
  readonly overflow: number
}

export type StyleReader = (element: Element) => CSSStyleDeclaration

// clientWidth and scrollWidth are rounded separately, so a 1px difference is not an overflow.
const SLACK = 1

function isVisible(element: Element): boolean {
  return (element as { checkVisibility?: () => boolean }).checkVisibility?.() ?? true
}

function classify(style: CSSStyleDeclaration): OverflowKind | null {
  if (style.overflowX === "auto" || style.overflowX === "scroll") return null
  if (style.textOverflow === "ellipsis") return null
  if (style.overflowX === "hidden" || style.overflowX === "clip") return "clipped"
  return style.whiteSpace === "nowrap" ? "nowrap" : "visible"
}

/**
 * Every visible box whose content is wider than the box, innermost boxes only. Scroll areas and
 * deliberate ellipsis truncation are not overflow; everything else is text that got cut or pushed
 * out, which is what a long translation or a narrow phone does first.
 */
export function findOverflow(root: Document, readStyle?: StyleReader): OverflowProblem[] {
  const view = root.defaultView
  if (!view) return []
  const style = readStyle ?? ((element) => view.getComputedStyle(element))
  const problems: OverflowProblem[] = []
  const html = root.documentElement
  const pageOverflow = html.scrollWidth - html.clientWidth
  if (pageOverflow > SLACK) problems.push({ kind: "page", element: html, overflow: pageOverflow })
  for (const element of root.body.querySelectorAll("*")) {
    const overflow = element.scrollWidth - element.clientWidth
    // Inline boxes report 0/0; visually hidden ones (sr-only) are a 1px clipped box on purpose.
    if (overflow <= SLACK || element.clientWidth <= 1 || !isVisible(element)) continue
    const kind = classify(style(element))
    if (kind) problems.push({ kind, element, overflow })
  }
  return problems.filter(
    (problem) =>
      problem.kind === "page" ||
      !problems.some(
        (other) =>
          other !== problem && other.kind !== "page" && problem.element.contains(other.element),
      ),
  )
}

export function describeOverflow(problem: OverflowProblem): OverflowReport {
  return {
    kind: problem.kind,
    tag: problem.element.tagName.toLowerCase(),
    className: problem.element.getAttribute("class") ?? "",
    text: problem.kind === "page" ? "" : problem.element.textContent.trim().slice(0, 60),
    overflow: problem.overflow,
  }
}

declare global {
  interface Window {
    /** Dev only: run the audit now and get a serialisable report (used by the headless self-check). */
    __sveOverflowAudit?: () => readonly OverflowReport[]
  }
}

/** Dev only: logs a console error whenever text overflows, after DOM changes, resizes and fonts. */
export function installOverflowAudit(root: Document = document): () => void {
  const view = root.defaultView
  if (!view) return () => undefined
  // Only what overflowed in the previous scan; an element that recovered and overflows again later
  // is news again, so the record is rebuilt on every scan.
  let reported = new Map<Element, number>()
  let timer: ReturnType<typeof setTimeout> | undefined
  const run = () => {
    timer = undefined
    const current = new Map<Element, number>()
    for (const problem of findOverflow(root)) {
      current.set(problem.element, problem.overflow)
      if (reported.get(problem.element) === problem.overflow) continue
      const report = describeOverflow(problem)
      console.error(
        `[overflow-audit] ${report.kind}: <${report.tag}> is ${String(report.overflow)}px too narrow for ${JSON.stringify(report.text)}`,
        problem.element,
      )
    }
    reported = current
  }
  const schedule = () => {
    if (timer !== undefined) clearTimeout(timer)
    timer = setTimeout(run, 300)
  }
  const observer = new view.MutationObserver(schedule)
  observer.observe(root.body, {
    subtree: true,
    childList: true,
    attributes: true,
    characterData: true,
  })
  view.addEventListener("resize", schedule)
  void (root as { fonts?: FontFaceSet }).fonts?.ready.then(schedule)
  view.__sveOverflowAudit = () => findOverflow(root).map(describeOverflow)
  schedule()
  return () => {
    observer.disconnect()
    view.removeEventListener("resize", schedule)
    if (timer !== undefined) clearTimeout(timer)
    delete view.__sveOverflowAudit
  }
}
