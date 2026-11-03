import { useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { useNavigate, useSearchParams } from "react-router"

import { deckPath } from "../../app/deck-routes"
import { useRouteEdition } from "../../app/snapshot"
import { Button } from "../../components/ui/Button"
import { Link } from "../../components/ui/Link"
import { Notice } from "../../components/ui/Notice"
import { Segmented } from "../../components/ui/Segmented"
import { useToast } from "../../components/ui/toast-context"
import { DeckCodeError } from "../../domain/deck/code"
import { type ImportedDeck, importEdition, readImport } from "../../domain/deck/import"
import { createDeck, totalCount } from "../../domain/deck/model"
import type { Region } from "../../domain/search"
import { decksStore, useDecks } from "../../settings"
import { PageTitle } from "../PageTitle"
import { useDeckData } from "./useDeckData"

const CODE_PARAM = "d"

function readSafely(text: string, read: () => ImportedDeck): ImportedDeck | "bad-code" | null {
  if (text.trim() === "") return null
  try {
    return read()
  } catch (error) {
    if (error instanceof DeckCodeError) return "bad-code"
    throw error
  }
}

/**
 * Paste a deck list or share code, or arrive from a share link (`?d=`): the preview says what was
 * understood and what was not before a new deck is made.
 */
export function DeckImportPage() {
  const { t } = useTranslation()
  const toast = useToast()
  const navigate = useNavigate()
  const [params, setParams] = useSearchParams()
  const [text, setText] = useState(() => params.get(CODE_PARAM) ?? "")
  // A code names its edition; a list is read in the page's edition (`ed`), which the user picks.
  const codeEdition = useMemo(() => {
    try {
      return importEdition(text)
    } catch {
      return undefined
    }
  }, [text])
  const routeEdition = useRouteEdition()
  const edition: Region = codeEdition ?? routeEdition
  useEffect(() => {
    if (edition === routeEdition) return
    const next = new URLSearchParams(params)
    next.set("ed", edition)
    setParams(next, { replace: true })
  }, [edition, routeEdition, params, setParams])
  const data = useDeckData(edition)
  // Reprints resolve by number or printing integer only once every printing is loaded.
  const facts = data.printingsReady ? data.facts : null
  const [saving, setSaving] = useState(false)
  // Creating waits for the saved decks to load, so the new deck is in the list the builder opens on.
  const decks = useDecks()
  const preview = useMemo(
    () =>
      facts === null
        ? null
        : readSafely(text, () =>
            readImport(text, {
              printingByCardNo: (cardNo) => facts.printingByCardNo(cardNo),
              printingByIntId: (intId) => facts.printingByIntId(intId),
              infoOf: (cardId) => facts.factsOf(cardId),
            }),
          ),
    [text, facts],
  )
  const create = () => {
    if (preview === null || preview === "bad-code") return
    const deck = {
      ...createDeck(
        {
          name: preview.name ?? t("decks.importedName"),
          edition,
          format: preview.format ?? "standard",
          basis: preview.basis,
        },
        new Date(),
      ),
      leader: preview.leader,
      main: preview.main,
      evolve: preview.evolve,
      side: preview.side,
    }
    // The page stays until the deck is stored, so a failed save can be tried again from here.
    setSaving(true)
    decksStore.save(deck).then(
      () => {
        void navigate(deckPath(deck), { replace: true })
      },
      () => {
        setSaving(false)
        toast.show({ message: t("decks.saveFailed") })
      },
    )
  }
  const nameOf = (cardId: string) => facts?.factsOf(cardId)?.name ?? cardId
  return (
    <div className="mx-auto flex w-full max-w-2xl flex-col gap-3 pb-6">
      <PageTitle>{t("decks.importTitle")}</PageTitle>
      <p className="text-14 text-text-2">
        {params.has(CODE_PARAM) ? t("decks.importLinkHint") : t("decks.importHint")}
      </p>
      <textarea
        value={text}
        onChange={(event) => {
          setText(event.target.value)
        }}
        rows={8}
        aria-label={t("decks.importPaste")}
        placeholder={t("decks.importPlaceholder")}
        className="w-full rounded-button border border-border bg-surface-1 p-3 font-mono text-13 text-text-1"
      />
      <div className="flex flex-wrap items-center gap-2 text-13 text-text-2">
        <span>{t("account.cardEdition")}</span>
        {codeEdition === undefined ? (
          <Segmented
            label={t("account.cardEdition")}
            value={edition}
            options={[
              { value: "jp", label: t("options.edition.jp") },
              { value: "en", label: t("options.edition.en") },
            ]}
            onChange={(next) => {
              const nextParams = new URLSearchParams(params)
              nextParams.set("ed", next)
              setParams(nextParams, { replace: true })
            }}
          />
        ) : (
          <span className="font-semibold text-text-1">{t(`options.edition.${edition}`)}</span>
        )}
      </div>
      {data.failed && (
        <Notice
          tone="danger"
          role="alert"
          actions={<Button onClick={data.retry}>{t("results.retry")}</Button>}
        >
          {t("results.loadFailed")}
        </Notice>
      )}
      {!data.failed && facts === null && text.trim() !== "" && (
        <p role="status" className="text-13 text-text-2">
          {t("results.dependenciesPending")}
        </p>
      )}
      {preview === "bad-code" && (
        <Notice tone="danger" role="alert">
          {t("decks.importBadCode")}
        </Notice>
      )}
      {preview !== null && preview !== "bad-code" && (
        <section
          aria-label={t("decks.importPreviewTitle")}
          className="flex flex-col gap-2 rounded-button border border-border bg-surface-1 p-3 text-13"
        >
          <p className="font-semibold">
            {t("decks.importCounts", {
              main: totalCount(preview.main),
              evolve: totalCount(preview.evolve),
              side: totalCount(preview.side),
            })}
          </p>
          <p className="text-text-2">
            {preview.leader
              ? t("decks.importLeader", { name: nameOf(preview.leader.cardId) })
              : t("decks.leaderMissingHint")}
          </p>
          {preview.name !== undefined && (
            <p className="text-text-2">{t("decks.importName", { name: preview.name })}</p>
          )}
          {preview.unknown.length > 0 && (
            <div className="text-danger">
              {t("decks.importUnknown", { count: preview.unknown.length })}
              <ul className="mt-1 flex flex-wrap gap-1 font-mono text-12">
                {preview.unknown.map((item) => (
                  <li key={item.label} className="rounded-badge bg-danger-soft px-1.5">
                    {t("decks.importCopies", { card: item.label, count: item.count })}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {preview.unrecognized.length > 0 && (
            <div className="text-text-2">
              {t("decks.importUnrecognized", { count: preview.unrecognized.length })}
              <ul className="mt-1 flex flex-col gap-0.5 font-mono text-12 text-text-3">
                {preview.unrecognized.map((item) => (
                  <li key={item.line} className="truncate">
                    {t("decks.importLine", { line: item.line, text: item.text })}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </section>
      )}
      <div className="flex flex-wrap items-center gap-3">
        <Button
          variant="primary"
          disabled={
            decks.status !== "ready" ||
            saving ||
            preview === null ||
            preview === "bad-code" ||
            (preview.leader === null &&
              preview.main.length + preview.evolve.length + preview.side.length === 0)
          }
          onClick={create}
        >
          {t("decks.importCreate")}
        </Button>
        <Link to="/decks">{t("decks.backToList")}</Link>
      </div>
    </div>
  )
}
