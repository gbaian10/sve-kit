import { type SyntheticEvent, useState } from "react"
import { useTranslation } from "react-i18next"
import { useNavigate } from "react-router"

import { Button } from "../ui/Button"

/** A one-line search that lands on /cards?q=…; for pages that are not the search page. */
export function QuickSearchForm({ className }: { readonly className?: string }) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [text, setText] = useState("")
  const submit = (event: SyntheticEvent<HTMLFormElement>) => {
    event.preventDefault()
    const query = text.trim()
    void navigate(query === "" ? "/cards" : `/cards?q=${encodeURIComponent(query)}`)
  }
  return (
    <form onSubmit={submit} className={className} role="search">
      <div className="flex items-center gap-2">
        <input
          type="search"
          aria-label={t("search.label")}
          placeholder={t("search.placeholder")}
          value={text}
          onChange={(event) => {
            setText(event.target.value)
          }}
          className="h-11 min-w-0 flex-1 rounded-button border border-border bg-surface-1 px-4 text-16 text-text-1 placeholder:text-text-3 focus:border-border-strong focus:outline-none focus-visible:outline-2"
        />
        <Button type="submit">{t("search.submit")}</Button>
      </div>
    </form>
  )
}
