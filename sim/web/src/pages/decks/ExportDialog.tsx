import { Check, Copy, Download, Share2 } from "lucide-react"
import { useEffect, useMemo, useRef, useState } from "react"
import { useTranslation } from "react-i18next"

import { useDialogHistory } from "../../app/dialog-history"
import { Button } from "../../components/ui/Button"
import { Dialog } from "../../components/ui/Dialog"
import { Notice } from "../../components/ui/Notice"
import { Segmented } from "../../components/ui/Segmented"
import type { ImageIndex } from "../../data"
import { type DeckCodeCard, encodeDeckCode } from "../../domain/deck/code"
import { totalCount } from "../../domain/deck/model"
import { deckToText } from "../../domain/deck/text"
import type { DeckEntry } from "../../domain/deck/types"
import { useDeckEditor } from "./deck-editor"
import {
  canvasEncoder,
  type DeckImageCard,
  DeckImageMissing,
  type DeckImageSize,
  encodeDeckImage,
  renderDeckImage,
} from "./deck-image"
import { useDeckLabels } from "./deck-labels"

type Tab = "text" | "share" | "image"

interface Picture {
  readonly size: DeckImageSize
  readonly url: string
  readonly blob: Blob
  readonly width: number
  readonly height: number
}

const EXTENSIONS: Record<string, string> = {
  "image/png": "png",
  "image/webp": "webp",
  "image/jpeg": "jpg",
}

/** Theme colours for the canvas, read from the page's own tokens. */
function canvasColors() {
  const styles = getComputedStyle(document.documentElement)
  const token = (name: string) => styles.getPropertyValue(name).trim()
  return {
    page: token("--bg"),
    text: token("--text-1"),
    muted: token("--text-3"),
    badge: token("--accent"),
    badgeText: token("--accent-ink"),
  }
}

function sizeLabel(bytes: number): string {
  return bytes >= 1024 * 1024
    ? `${(bytes / 1024 / 1024).toFixed(1)} MB`
    : `${String(Math.round(bytes / 1024))} KB`
}
type Copied = "text" | "link" | "code" | null

/** The share link a deck code travels in; opening it shows the import preview. */
function shareLink(code: string): string {
  return `${window.location.origin}/decks/import?d=${code}`
}

/** The deck as text, as a share code or link, and as a picture; all made in the browser. */
export function ExportDialog({
  open,
  onClose,
  images,
}: {
  readonly open: boolean
  readonly onClose: () => void
  /** The deck's card images (its printings, from the search worker). */
  readonly images: ImageIndex | undefined
}) {
  const { t } = useTranslation()
  const { deck, facts: loadedFacts, printingsReady, dataFailed, retryData } = useDeckEditor()
  // Card numbers and printing integers of reprints are known only once every printing is loaded.
  const facts = printingsReady ? loadedFacts : null
  const labels = useDeckLabels()
  const [size, setSize] = useState<DeckImageSize>("small")
  const [picture, setPicture] = useState<Picture | null>(null)
  const [drawing, setDrawing] = useState<
    | { readonly state: "idle" | "drawing" | "failed" }
    | { readonly state: "missing"; readonly cards: readonly string[] }
  >({ state: "idle" })
  // Each picture request is a job; closing, another size or leaving drops the one in flight.
  const job = useRef(0)
  const dropJob = () => {
    job.current += 1
    setDrawing({ state: "idle" })
  }
  useEffect(
    () => () => {
      job.current += 1
    },
    [],
  )
  // An old picture's object URL is released when a new one replaces it or the dialog goes away.
  useEffect(
    () => () => {
      if (picture) URL.revokeObjectURL(picture.url)
    },
    [picture],
  )
  const close = useDialogHistory("deck-export", open, () => {
    dropJob()
    setPicture(null)
    onClose()
  })
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
  const draw = async (chosen: DeckImageSize) => {
    if (!facts) return
    const mine = ++job.current
    setDrawing({ state: "drawing" })
    const card = (entry: DeckEntry): DeckImageCard => {
      const summary = facts.summaryOf(entry.cardId, entry.printingId)
      return {
        src: summary ? images?.cardImage(summary.printingId, summary.faceId)?.src : undefined,
        label: facts.factsOf(entry.cardId)?.name ?? entry.cardId,
        count: entry.count,
      }
    }
    const leader = deck.leader
    const leaderSummary = leader ? facts.summaryOf(leader.cardId, leader.printingId) : undefined
    const leaderName = leader ? (facts.factsOf(leader.cardId)?.name ?? leader.cardId) : ""
    try {
      const canvas = await renderDeckImage({
        size: chosen,
        title: deck.name,
        subtitle: t("decks.imageSubtitle", {
          edition: t(`options.edition.${deck.edition}`),
          basis: labels.basisLabel(deck.basis),
          main: totalCount(deck.main),
          evolve: totalCount(deck.evolve),
        }),
        leader: leader
          ? {
              src: leaderSummary
                ? images?.cardImage(leaderSummary.printingId, leaderSummary.faceId)?.src
                : undefined,
              label: leaderName,
            }
          : null,
        leaderLine: t("decks.importLeader", { name: leaderName }),
        sections: [
          { label: t("decks.section.main"), cards: deck.main.map(card) },
          { label: t("decks.section.evolve"), cards: deck.evolve.map(card) },
        ],
        // The viewer's own calendar date, as YYYY-MM-DD.
        footer: t("decks.imageFooter", { date: new Date().toLocaleDateString("sv-SE") }),
        colors: canvasColors(),
      })
      const blob = await encodeDeckImage(chosen, canvasEncoder(canvas))
      if (job.current !== mine) return
      setPicture({
        size: chosen,
        url: URL.createObjectURL(blob),
        blob,
        width: canvas.width,
        height: canvas.height,
      })
      setDrawing({ state: "idle" })
    } catch (error) {
      if (job.current !== mine) return
      setDrawing(
        error instanceof DeckImageMissing
          ? { state: "missing", cards: error.cards }
          : { state: "failed" },
      )
    }
  }
  const fileName = picture
    ? `${deck.name.replace(/[^\p{L}\p{N} _-]+/gu, "").trim() || "deck"}${picture.size === "hd" ? "-hd" : ""}.${EXTENSIONS[picture.blob.type] ?? "png"}`
    : ""
  const pictureFile = picture
    ? new File([picture.blob], fileName, { type: picture.blob.type })
    : null
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
            { value: "image", label: t("decks.exportImage") },
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
        {facts !== null && tab === "image" && (
          <>
            <p className="text-13 text-text-2">{t("decks.exportImageHint")}</p>
            <Segmented
              className="self-start"
              label={t("decks.imageSize")}
              value={size}
              options={[
                { value: "small", label: t("decks.imageSmall") },
                { value: "hd", label: t("decks.imageHd") },
              ]}
              onChange={(next) => {
                dropJob()
                setPicture(null)
                setSize(next)
              }}
            />
            {picture === null && drawing.state !== "drawing" && (
              <Button
                variant="primary"
                className="self-start"
                onClick={() => {
                  void draw(size)
                }}
              >
                {t("decks.drawImage")}
              </Button>
            )}
            {drawing.state === "drawing" && (
              <p role="status" className="text-13 text-text-2">
                {t("decks.drawingImage")}
              </p>
            )}
            {drawing.state === "failed" && (
              <Notice tone="danger" role="alert">
                {t("decks.drawImageFailed")}
              </Notice>
            )}
            {drawing.state === "missing" && (
              <Notice tone="danger" role="alert">
                {t("decks.drawImageMissing", {
                  count: drawing.cards.length,
                  cards: drawing.cards.join(t("decks.listSeparator")),
                })}
              </Notice>
            )}
            {picture && (
              <>
                <img
                  src={picture.url}
                  alt={t("decks.imageAlt", { name: deck.name })}
                  className="w-full rounded-button border border-border"
                />
                <p className="text-12 text-text-3 tabular-nums">
                  {t("decks.imageInfo", {
                    width: picture.width,
                    height: picture.height,
                    size: sizeLabel(picture.blob.size),
                  })}
                </p>
                <div className="flex flex-wrap gap-2">
                  <a
                    href={picture.url}
                    download={fileName}
                    className="inline-flex h-11 items-center gap-2 rounded-button bg-accent px-4 text-15 font-semibold text-accent-ink no-underline hover:brightness-105"
                  >
                    <Download className="size-4" aria-hidden="true" />
                    {t("decks.downloadImage")}
                  </a>
                  {pictureFile &&
                    "canShare" in navigator &&
                    navigator.canShare({ files: [pictureFile] }) && (
                      <Button
                        onClick={() => {
                          navigator
                            .share({ files: [pictureFile], title: deck.name })
                            .catch(() => undefined)
                        }}
                      >
                        <Share2 className="size-4" aria-hidden="true" />
                        {t("decks.share")}
                      </Button>
                    )}
                </div>
              </>
            )}
          </>
        )}
      </div>
    </Dialog>
  )
}
