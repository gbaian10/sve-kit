import { useEffect, useEffectEvent, useRef } from "react"

import { REPORT_TURNSTILE_ACTION } from "../../domain/card-report"

interface TurnstileApi {
  render(
    container: HTMLElement,
    options: {
      sitekey: string
      action: string
      size: "compact" | "flexible"
      language: string
      "response-field": false
      callback(token: string): void
      "expired-callback"(): void
      "error-callback"(): void
    },
  ): string
  remove(id: string): void
}

let scriptRequest: Promise<TurnstileApi> | undefined

function loadTurnstile(): Promise<TurnstileApi> {
  const browser: Window & { turnstile?: TurnstileApi } = window
  if (browser.turnstile) return Promise.resolve(browser.turnstile)
  scriptRequest ??= new Promise<TurnstileApi>((resolve, reject) => {
    const script = document.createElement("script")
    script.src = "https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit"
    script.async = true
    const fail = () => {
      script.remove()
      reject(new Error("Turnstile API unavailable"))
    }
    script.onload = () => {
      if (browser.turnstile) resolve(browser.turnstile)
      else fail()
    }
    script.onerror = fail
    document.head.append(script)
  }).catch((error: unknown) => {
    scriptRequest = undefined
    throw error
  })
  return scriptRequest
}

export function ReportTurnstile({
  siteKey,
  language,
  onToken,
  onError,
}: {
  readonly siteKey: string
  readonly language: string
  readonly onToken: (token: string | null) => void
  readonly onError: () => void
}) {
  const element = useRef<HTMLDivElement>(null)
  const changeToken = useEffectEvent(onToken)
  const showError = useEffectEvent(onError)

  useEffect(() => {
    const container = element.current
    if (!container) return
    let active = true
    let remove: (() => void) | undefined
    void loadTurnstile()
      .then((api) => {
        if (!active) return
        const widget = api.render(container, {
          sitekey: siteKey,
          action: REPORT_TURNSTILE_ACTION,
          size: container.clientWidth < 300 ? "compact" : "flexible",
          language: language.toLowerCase(),
          "response-field": false,
          callback: (token) => {
            if (active) changeToken(token)
          },
          "expired-callback": () => {
            if (active) changeToken(null)
          },
          "error-callback": () => {
            if (active) showError()
          },
        })
        remove = () => {
          api.remove(widget)
        }
      })
      .catch(() => {
        if (active) showError()
      })
    return () => {
      active = false
      remove?.()
    }
  }, [siteKey, language])

  return <div ref={element} />
}
