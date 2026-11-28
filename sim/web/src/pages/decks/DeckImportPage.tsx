import { useEffect, useMemo, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { useNavigate, useSearchParams } from "react-router"

import { deckPath } from "../../app/deck-routes"
import { useRouteEdition } from "../../app/snapshot"
import { Button } from "../../components/ui/Button"
import { Link } from "../../components/ui/Link"
import { Notice } from "../../components/ui/Notice"
import { Segmented } from "../../components/ui/Segmented"
import { useToast } from "../../components/ui/toast-context"
import { type DeckLogResult, fetchDeckLogDeck } from "../../data"
import { DeckCodeError } from "../../domain/deck/code"
import {
  type ImportedDeck,
  importEdition,
  type ImportLookup,
  readDeckLogImport,
  readImport,
} from "../../domain/deck/import"
import { createDeck, totalCount } from "../../domain/deck/model"
import type { DeckEntry } from "../../domain/deck/types"
import { deckLogReferenceOf } from "../../domain/decklog"
import type { Region } from "../../domain/search"
import { decksStore, useDecks } from "../../settings"
import { PageTitle } from "../PageTitle"
import { deckImageInput, deckImagesPending } from "./deck-image-input"
import { useDeckLabels } from "./deck-labels"
import { DeckPicture } from "./DeckPicture"
import { useDeckData, useDeckImages } from "./useDeckData"

const CODE_PARAM = "d"
const NO_CARDS: readonly DeckEntry[] = []

function readSafely(text: string, read: () => ImportedDeck): ImportedDeck | "bad-code" | null {
  if (text.trim() === "") return null
  try {
    return read()
  } catch (error) {
    if (error instanceof DeckCodeError) return "bad-code"
    throw error
  }
}

interface DeckLogLookup {
  readonly key: string
  readonly result: DeckLogResult | "loading"
}

/**
 * Paste a deck list, share code or DeckLog code or link, or arrive from a share link (`?d=`): the preview says what was
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
  // DeckLog runs one system per edition: a link says which, a bare code is read in the page's edition.
  const deckLog = useMemo(
    () => (codeEdition === undefined ? deckLogReferenceOf(text) : undefined),
    [codeEdition, text],
  )
  const routeEdition = useRouteEdition()
  const edition: Region = codeEdition ?? deckLog?.region ?? routeEdition
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
  const deckLogKey = deckLog ? `${edition}/${deckLog.code}` : ""
  const [lookup, setLookup] = useState<DeckLogLookup | null>(null)
  const deckLogState = lookup !== null && lookup.key === deckLogKey ? lookup.result : null
  const pending = useRef<AbortController | null>(null)
  // A lookup for a code or edition the page has moved away from is dropped, unmounting included.
  useEffect(
    () => () => {
      pending.current?.abort()
      pending.current = null
      // The aborted lookup never settles, so coming back to its code must not find it still loading.
      setLookup((previous) => (previous?.result === "loading" ? null : previous))
    },
    [deckLogKey],
  )
  const readDeckLog = () => {
    if (!deckLog) return
    pending.current?.abort()
    const controller = new AbortController()
    pending.current = controller
    const key = deckLogKey
    setLookup({ key, result: "loading" })
    fetchDeckLogDeck(edition, deckLog.code, controller.signal).then(
      (result) => {
        // Only the latest lookup may settle the state; an older one finishing late is ignored.
        if (pending.current !== controller) return
        pending.current = null
        setLookup({ key, result })
      },
      () => undefined,
    )
  }
  const preview = useMemo(() => {
    if (facts === null) return null
    const importLookup: ImportLookup = {
      printingByCardNo: (cardNo) => facts.printingByCardNo(cardNo),
      printingByIntId: (intId) => facts.printingByIntId(intId),
      infoOf: (cardId) => facts.factsOf(cardId),
    }
    if (deckLog)
      return deckLogState !== null && deckLogState !== "loading" && "deck" in deckLogState
        ? readDeckLogImport(deckLogState.deck, importLookup)
        : null
    return readSafely(text, () => readImport(text, importLookup))
  }, [text, facts, deckLog, deckLogState])
  const deckLogDeck =
    deckLogState !== null && deckLogState !== "loading" && "deck" in deckLogState
      ? deckLogState.deck
      : undefined
  const deckLogError =
    deckLogState !== null && deckLogState !== "loading" && "error" in deckLogState
      ? deckLogState.error
      : undefined
  const understood = preview !== null && preview !== "bad-code" ? preview : null
  const deckName =
    understood?.name ??
    (deckLogDeck
      ? t("decks.importDeckLogName", { code: deckLogDeck.code })
      : t("decks.importedName"))
  // The picture's cards: reprints' image rows come from the search worker, as in the builder.
  const pictureCards = useMemo(
    () => ({
      edition,
      leader: understood?.leader ?? null,
      main: understood?.main ?? NO_CARDS,
      evolve: understood?.evolve ?? NO_CARDS,
      side: NO_CARDS,
    }),
    [edition, understood],
  )
  const { images, pending: imagePending } = useDeckImages(pictureCards, facts, data.images)
  const labels = useDeckLabels()
  // Class names come with the search worker's options; until then (unless that search failed) a
  // class would show as its code.
  const basisText =
    understood === null ||
    (understood.basis.kind === "class" &&
      labels.classOptions === undefined &&
      !labels.classOptionsFailed)
      ? null
      : labels.basisLabel(understood.basis)
  const hasPicture =
    understood !== null &&
    (understood.leader !== null || understood.main.length + understood.evolve.length > 0)
  const picture = useMemo(() => {
    if (facts === null || understood === null || !hasPicture || basisText === null) return null
    const deck = { ...pictureCards, name: deckName, basis: understood.basis }
    if (deckImagesPending(deck, facts, imagePending)) return null
    return deckImageInput({
      deck,
      facts,
      images,
      size: "small",
      file: "card_s",
      missing: "placeholder",
      t,
      basisLabel: () => basisText,
    })
  }, [facts, understood, hasPicture, pictureCards, deckName, images, imagePending, t, basisText])
  const create = () => {
    if (preview === null || preview === "bad-code") return
    const deck = {
      ...createDeck(
        {
          name: deckName,
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
      <p className="text-14 text-pretty text-text-2">
        {params.has(CODE_PARAM) ? t("decks.importLinkHint") : t("decks.importHint")}
      </p>
      <textarea
        value={text}
        onChange={(event) => {
          setText(event.target.value)
        }}
        rows={8}
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
        aria-label={t("decks.importPaste")}
        placeholder={t("decks.importPlaceholder")}
        className="w-full rounded-button border border-border bg-surface-1 p-3 font-mono text-13 text-text-1"
      />
      <div className="flex flex-wrap items-center gap-2 text-13 text-text-2">
        <span>{t("account.cardEdition")}</span>
        {codeEdition === undefined && deckLog?.region === undefined ? (
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
      {deckLog && (
        <div className="flex flex-col gap-2 rounded-button border border-border bg-surface-1 p-3 text-13">
          <p className="text-text-2">
            {deckLog.region === undefined
              ? t("decks.importDeckLogBareHint", {
                  code: deckLog.code,
                  edition: t(`options.edition.${edition}`),
                })
              : t("decks.importDeckLogLinkHint", {
                  code: deckLog.code,
                  edition: t(`options.edition.${edition}`),
                })}
          </p>
          <div className="flex flex-wrap items-center gap-3">
            <Button
              type="button"
              disabled={deckLogState === "loading" || deckLogDeck !== undefined}
              onClick={readDeckLog}
            >
              {t("decks.importDeckLogRead")}
            </Button>
            <p role="status" className="text-13 text-text-2">
              {deckLogState === "loading" ? t("decks.importDeckLogLoading") : ""}
            </p>
          </div>
          {deckLogError !== undefined && (
            <Notice tone="danger" role="alert">
              {t(`decks.importDeckLogErrors.${deckLogError}`, {
                code: deckLog.code,
                edition: t(`options.edition.${edition}`),
              })}
            </Notice>
          )}
        </div>
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
          {hasPicture && (
            <DeckPicture input={picture} label={t("decks.imageAlt", { name: deckName })} />
          )}
          {deckLogDeck && (
            <p className="text-text-2">
              {t("decks.importDeckLogSource", { code: deckLogDeck.code })}
            </p>
          )}
          {deckLogDeck && deckLogDeck.className !== null && (
            <p className="text-text-2">
              {t("decks.importDeckLogClass")}
              {/* DeckLog's own wording, in its edition's language. */}
              <span lang={deckLogDeck.region === "jp" ? "ja" : "en"}>{deckLogDeck.className}</span>
            </p>
          )}
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
          {preview.extraLeaders.length > 0 && (
            <div className="text-danger">
              {t("decks.importExtraLeaders", { count: preview.extraLeaders.length })}
              <ul className="mt-1 flex flex-wrap gap-1 font-mono text-12">
                {preview.extraLeaders.map((item) => (
                  <li key={item.label} className="rounded-badge bg-danger-soft px-1.5">
                    {t("decks.importCopies", { card: item.label, count: item.count })}
                  </li>
                ))}
              </ul>
            </div>
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
