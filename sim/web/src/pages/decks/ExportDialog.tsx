import { Check, Copy, Share2 } from "lucide-react"
import { useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"

import { useDialogHistory } from "../../app/dialog-history"
import { Button } from "../../components/ui/Button"
import { Dialog } from "../../components/ui/Dialog"
import { Notice } from "../../components/ui/Notice"
import { Segmented } from "../../components/ui/Segmented"
import { type DeckCodeCard, encodeDeckCode } from "../../domain/deck/code"
import { deckToText } from "../../domain/deck/text"
import type { DeckEntry } from "../../domain/deck/types"
import { useDeckEditor } from "./deck-editor"

type Tab = "text" | "share"
type Copied = "text" | "link" | "code" | null

/** The share link a deck code travels in; opening it shows the import preview. */
function shareLink(code: string): string {
  return `${window.location.origin}/decks/import?d=${code}`
}

/** The deck as text and as a share code or link, both made in the browser. */
export function ExportDialog({
  open,
  onClose,
}: {
  readonly open: boolean
  readonly onClose: () => void
}) {
  const { t } = useTranslation()
  const { deck, facts: loadedFacts, printingsReady, dataFailed, retryData } = useDeckEditor()
  // Card numbers and printing integers of reprints are known only once every printing is loaded.
  const facts = printingsReady ? loadedFacts : null
  const close = useDialogHistory("deck-export", open, onClose)
  const [tab, setTab] = useState<Tab>("text")
  const [copied, setCopied] = useState<Copied>(null)
  useEffect(() => {
    if (copied === null) return
    const timer = setTimeout(() => {
      setCopied(null)
    }, 1500)
    return () => {
      clearTimeout(timer)
    }
  }, [copied])
  const copy = (what: Exclude<Copied, null>, value: string) => {
    navigator.clipboard.writeText(value).then(
      () => {
        setCopied(what)
      },
      () => {
        setCopied(null)
      },
    )
  }
  const text = useMemo(
    () =>
      facts
        ? deckToText(
            deck,
            {
              cardNoOf: (entry) => facts.cardNoOf(entry),
              nameOf: (cardId) => facts.factsOf(cardId)?.name,
            },
            {
              leader: t("decks.leader"),
              main: t("decks.section.main"),
              evolve: t("decks.section.evolve"),
              side: t("decks.section.side"),
            },
          )
        : "",
    [deck, facts, t],
  )
  // Cards without a permanent integer cannot travel in a code; they are named, not dropped silently.
  const share = useMemo(() => {
    if (!facts) return { code: "", missing: [] }
    const missing: string[] = []
    const cards = (entries: readonly DeckEntry[]): DeckCodeCard[] =>
      entries.flatMap((entry) => {
        const intId = facts.intIdOf(entry)
        if (intId !== undefined) return [{ intId, count: entry.count }]
        missing.push(facts.factsOf(entry.cardId)?.name ?? entry.cardId)
        return []
      })
    const leader = deck.leader ? (facts.intIdOf(deck.leader) ?? null) : null
    if (deck.leader && leader === null)
      missing.push(facts.factsOf(deck.leader.cardId)?.name ?? deck.leader.cardId)
    const code = encodeDeckCode({
      edition: deck.edition,
      basis: deck.basis,
      leader,
      main: cards(deck.main),
      evolve: cards(deck.evolve),
      side: cards(deck.side),
      name: deck.name,
    })
    return { code, missing }
  }, [deck, facts])
  const link = share.code === "" ? "" : shareLink(share.code)
  const copyIcon = (what: Exclude<Copied, null>) =>
    copied === what ? (
      <Check className="size-4" aria-hidden="true" />
    ) : (
      <Copy className="size-4" aria-hidden="true" />
    )
  return (
    <Dialog open={open} onClose={close} title={t("decks.exportTitle")} variant="sheet">
      <div className="flex flex-col gap-3 pb-2">
        <Segmented
          className="self-start"
          label={t("decks.exportTitle")}
          value={tab}
          options={[
            { value: "text", label: t("decks.exportText") },
            { value: "share", label: t("decks.exportShare") },
          ]}
          onChange={setTab}
        />
        {facts === null && dataFailed && (
          <Notice
            tone="danger"
            role="alert"
            actions={<Button onClick={retryData}>{t("results.retry")}</Button>}
          >
            {t("results.loadFailed")}
          </Notice>
        )}
        {facts === null && !dataFailed && (
          <p role="status" className="text-13 text-text-2">
            {t("results.dependenciesPending")}
          </p>
        )}
        {facts !== null && tab === "text" && (
          <>
            <p className="text-13 text-text-2">{t("decks.exportTextHint")}</p>
            <textarea
              readOnly
              value={text}
              rows={14}
              aria-label={t("decks.exportText")}
              className="w-full rounded-button border border-border bg-well p-3 font-mono text-12 text-text-1"
            />
            <div className="flex gap-2">
              <Button
                variant="primary"
                onClick={() => {
                  copy("text", text)
                }}
              >
                {copyIcon("text")}
                {copied === "text" ? t("decks.copied") : t("decks.copyText")}
              </Button>
            </div>
          </>
        )}
        {facts !== null && tab === "share" && (
          <>
            <p className="text-13 text-text-2">{t("decks.exportShareHint")}</p>
            <label className="flex flex-col gap-1 text-12 text-text-3">
              {t("decks.shareLink")}
              <textarea
                readOnly
                value={link}
                rows={3}
                className="w-full rounded-button border border-border bg-well p-3 font-mono text-12 break-all text-text-1"
              />
            </label>
            <label className="flex flex-col gap-1 text-12 text-text-3">
              {t("decks.shareCode")}
              <textarea
                readOnly
                value={share.code}
                rows={2}
                className="w-full rounded-button border border-border bg-well p-3 font-mono text-12 break-all text-text-1"
              />
            </label>
            {share.missing.length > 0 && (
              <Notice tone="warning">
                {t("decks.shareMissing", {
                  count: share.missing.length,
                  cards: share.missing.join(t("decks.listSeparator")),
                })}
              </Notice>
            )}
            <div className="flex flex-wrap gap-2">
              <Button
                variant="primary"
                onClick={() => {
                  copy("link", link)
                }}
              >
                {copyIcon("link")}
                {copied === "link" ? t("decks.copied") : t("decks.copyLink")}
              </Button>
              <Button
                onClick={() => {
                  copy("code", share.code)
                }}
              >
                {copyIcon("code")}
                {copied === "code" ? t("decks.copied") : t("decks.copyCode")}
              </Button>
              {typeof navigator.share === "function" && (
                <Button
                  onClick={() => {
                    navigator.share({ title: deck.name, url: link }).catch(() => undefined)
                  }}
                >
                  <Share2 className="size-4" aria-hidden="true" />
                  {t("decks.share")}
                </Button>
              )}
            </div>
            <p className="text-12 text-text-3">{t("decks.shareNote")}</p>
          </>
        )}
      </div>
    </Dialog>
  )
}
