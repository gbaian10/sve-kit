import { Settings as SettingsIcon } from "lucide-react"
import { useEffect, useRef, useState } from "react"
import { useTranslation } from "react-i18next"

import type { CardRef } from "../../../battle/contract"
import { Button } from "../../../components/ui/Button"
import { Dialog } from "../../../components/ui/Dialog"
import { Segmented } from "../../../components/ui/Segmented"
import { SettingRow } from "../../../components/ui/SettingRow"
import {
  BATTLE_MAT_BLUR_MAX,
  BATTLE_MAT_DIM_MAX,
  BATTLE_MAT_LOOKS,
  BATTLE_MAT_REACHES,
  prefsStore,
  usePrefs,
} from "../../../settings"
import { type KnownCard, useCards } from "../battle-cards"
import { clearMatUpload, saveMatUpload, useMatUpload } from "../mat-upload"
import { CardFace } from "../table/Cards"
import type { Framing } from "./mat-crop"
import { MatCropper } from "./MatCropper"

const RESULTS = 12

/** One card as a choice: its picture, name and number. */
function CardChoice({
  known,
  chosen,
  onPick,
}: {
  readonly known: KnownCard
  readonly chosen: boolean
  readonly onPick: () => void
}) {
  return (
    <button
      type="button"
      className={chosen ? "bp-set-card is-chosen" : "bp-set-card"}
      aria-pressed={chosen}
      onClick={onPick}
    >
      <span className="bp-set-thumb">
        <CardFace card={{ printing_id: known.summary.printingId, face: 0 }} />
      </span>
      <span className="bp-set-card-name" lang={known.name.lang}>
        {known.name.text}
      </span>
      <span className="bp-set-card-no">{known.summary.cardNo}</span>
    </button>
  )
}

/** Which card's art lies on the mat: the player's leader until they search for another. */
function MatCardPicker({
  leader,
  onFrame,
}: {
  readonly leader: CardRef
  /** Starts framing a card's art (null = the leader's), from the frame it had before if any. */
  readonly onFrame: (printing: string | null, initial?: Framing) => void
}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const cards = useCards()
  const [text, setText] = useState("")
  const current =
    prefs.battleMatCard === null
      ? cards.find(leader)
      : cards.find({ printing_id: prefs.battleMatCard, face: 0 })
  const results = cards.search(text, RESULTS)
  return (
    <div className="bp-set-block">
      <p className="bp-set-current">
        {current === undefined
          ? t("play.settings.card.unknown")
          : t("play.settings.card.current", {
              name: current.name.text,
              no: current.summary.cardNo,
            })}
        {prefs.battleMatCard === null ? ` ${t("play.settings.card.leader")}` : null}
      </p>
      <input
        type="search"
        className="bp-set-search"
        value={text}
        placeholder={t("play.settings.card.search")}
        aria-label={t("play.settings.card.search")}
        onChange={(event) => {
          setText(event.target.value)
        }}
      />
      {text.trim() !== "" && results.length === 0 ? (
        <p className="bp-set-note">{t("play.settings.card.none")}</p>
      ) : null}
      {results.length > 0 ? (
        <div className="bp-set-results">
          {results.map((known) => (
            <CardChoice
              key={known.summary.printingId}
              known={known}
              chosen={prefs.battleMatCard === known.summary.printingId}
              onPick={() => {
                // Nothing changes until the player has framed the art and said so.
                onFrame(known.summary.printingId)
              }}
            />
          ))}
        </div>
      ) : null}
      <div className="bp-set-upload">
        <Button
          size="sm"
          onClick={() => {
            onFrame(prefs.battleMatCard, prefs.battleMatCardFraming ?? undefined)
          }}
        >
          {t("play.settings.card.reframe")}
        </Button>
        {prefs.battleMatCard !== null ? (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              prefsStore.set({ battleMatCard: null, battleMatCardFraming: null })
            }}
          >
            {t("play.settings.card.reset")}
          </Button>
        ) : null}
      </div>
    </div>
  )
}

/**
 * The player's own picture: chosen from the device or pasted, framed for the mat, kept whole on the
 * device. `onFrame` starts framing a picture, with the frame it had when it was framed before.
 */
function MatUpload({ onFrame }: { readonly onFrame: (file: Blob, initial?: Framing) => void }) {
  const { t } = useTranslation()
  const kept = useMatUpload()
  const input = useRef<HTMLInputElement>(null)
  const [clipboard, setClipboard] = useState<"empty" | "denied" | null>(null)
  const paste = async () => {
    try {
      for (const item of await navigator.clipboard.read()) {
        const type = item.types.find((entry) => entry.startsWith("image/"))
        if (type === undefined) continue
        setClipboard(null)
        onFrame(await item.getType(type))
        return
      }
      setClipboard("empty")
    } catch {
      // No permission, or a browser that cannot read pictures from the clipboard.
      setClipboard("denied")
    }
  }
  return (
    <div className="bp-set-block">
      <p className="bp-set-note">{t("play.settings.upload.hint")}</p>
      <div className="bp-set-upload">
        {kept !== null ? <img className="bp-set-preview" src={kept.url} alt="" /> : null}
        <input
          ref={input}
          type="file"
          accept="image/*"
          className="sr-only"
          aria-label={t("play.settings.upload.choose")}
          onChange={(event) => {
            const file = event.target.files?.[0]
            // The same file can be chosen again after a cancelled framing.
            event.target.value = ""
            if (file !== undefined) onFrame(file)
          }}
        />
        <Button
          size="sm"
          onClick={() => {
            input.current?.click()
          }}
        >
          {kept === null ? t("play.settings.upload.choose") : t("play.settings.upload.replace")}
        </Button>
        <Button
          size="sm"
          onClick={() => {
            void paste()
          }}
        >
          {t("play.settings.upload.clipboard")}
        </Button>
        {kept !== null ? (
          <>
            <Button
              size="sm"
              onClick={() => {
                onFrame(kept.picture, kept.framing)
              }}
            >
              {t("play.settings.upload.reframe")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                void clearMatUpload()
              }}
            >
              {t("play.settings.upload.remove")}
            </Button>
          </>
        ) : (
          <span className="bp-set-note">{t("play.settings.upload.none")}</span>
        )}
      </div>
      {clipboard === "empty" ? (
        <p role="status" className="bp-set-note is-warning">
          {t("play.settings.upload.clipboardEmpty")}
        </p>
      ) : clipboard === "denied" ? (
        <p role="status" className="bp-set-note is-warning">
          {t("play.settings.upload.clipboardDenied")}
        </p>
      ) : (
        <p className="bp-set-note">{t("play.settings.upload.paste")}</p>
      )}
    </div>
  )
}

/** How much a card's art or the player's picture is blurred and darkened; 0 leaves it as it is. */
function PhotoSliders() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  return (
    <div className="bp-set-block">
      <label className="bp-set-slider">
        <span aria-hidden="true">{t("play.settings.blur")}</span>
        <input
          type="range"
          aria-label={t("play.settings.blur")}
          min={0}
          max={BATTLE_MAT_BLUR_MAX}
          step={1}
          value={prefs.battleMatBlur}
          onChange={(event) => {
            prefsStore.set({ battleMatBlur: Number(event.target.value) })
          }}
        />
        <output>
          {prefs.battleMatBlur === 0
            ? t("play.settings.blurNone")
            : t("play.settings.blurValue", { n: prefs.battleMatBlur })}
        </output>
      </label>
      <label className="bp-set-slider">
        <span aria-hidden="true">{t("play.settings.dim")}</span>
        <input
          type="range"
          aria-label={t("play.settings.dim")}
          min={0}
          max={BATTLE_MAT_DIM_MAX}
          step={5}
          value={prefs.battleMatDim}
          onChange={(event) => {
            prefsStore.set({ battleMatDim: Number(event.target.value) })
          }}
        />
        <output>{t("play.settings.dimValue", { n: prefs.battleMatDim })}</output>
      </label>
    </div>
  )
}

/** The gear and the battle settings it opens: the player's own mat, kept on this device. */
export function SettingsButton({
  leader,
  matAspect,
}: {
  /** The player's own leader: the card whose art the mat shows until another is picked. */
  readonly leader: CardRef
  /** Width over height of the mat's picture as it is laid out now: the shape an upload is cut to. */
  readonly matAspect: number
}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const [open, setOpen] = useState(false)
  // What is being framed: a picture of the player's own (with an address an image can show for as
  // long as that lasts), or a card's art.
  const [cropping, setCropping] = useState<
    | {
        readonly kind: "file"
        readonly file: Blob
        readonly url: string
        readonly initial?: Framing | undefined
      }
    | {
        readonly kind: "card"
        readonly printing: string | null
        readonly initial?: Framing | undefined
      }
    | null
  >(null)
  const [failed, setFailed] = useState(false)
  const look = prefs.battleMatLook
  const endCrop = () => {
    if (cropping?.kind === "file") URL.revokeObjectURL(cropping.url)
    setCropping(null)
  }
  const frame = (file: Blob, initial?: Framing) => {
    setFailed(false)
    setCropping({ kind: "file", file, url: URL.createObjectURL(file), initial })
  }
  // A picture pasted while the settings are open is taken as the player's own.
  useEffect(() => {
    if (!open) return
    const onPaste = (event: ClipboardEvent) => {
      // One picture at a time: a paste while one is being framed is left alone.
      if (document.querySelector(".bp-crop") !== null) return
      const file = Array.from(event.clipboardData?.items ?? [])
        .find((item) => item.type.startsWith("image/"))
        ?.getAsFile()
      if (file === null || file === undefined) return
      event.preventDefault()
      prefsStore.set({ battleMatLook: "upload" })
      setFailed(false)
      setCropping({ kind: "file", file, url: URL.createObjectURL(file) })
    }
    document.addEventListener("paste", onPaste)
    return () => {
      document.removeEventListener("paste", onPaste)
    }
  }, [open])
  return (
    <>
      <button
        type="button"
        className="bp-gear"
        onClick={() => {
          setOpen(true)
        }}
        aria-label={t("play.settings.title")}
        title={t("play.settings.title")}
      >
        <SettingsIcon aria-hidden="true" />
      </button>
      <Dialog
        open={open}
        onClose={() => {
          setOpen(false)
          endCrop()
        }}
        title={cropping === null ? t("play.settings.title") : t("play.settings.upload.cropTitle")}
        variant="center"
      >
        {cropping !== null ? (
          <div className="p-4">
            <MatCropper
              source={
                cropping.kind === "file"
                  ? cropping
                  : {
                      kind: "card",
                      card:
                        cropping.printing === null
                          ? leader
                          : { printing_id: cropping.printing, face: 0 },
                    }
              }
              aspect={matAspect}
              reach={prefs.battleMatReach}
              initial={cropping.initial}
              onCancel={endCrop}
              onDone={({ framing, kept }) => {
                endCrop()
                if (cropping.kind === "card") {
                  prefsStore.set({
                    battleMatCard: cropping.printing,
                    battleMatCardFraming: framing,
                  })
                  return
                }
                if (kept === null) return
                void saveMatUpload(kept.picture, kept.size, framing).then((stored) => {
                  setFailed(!stored)
                })
              }}
            />
          </div>
        ) : (
          <div className="p-4">
            <p className="bp-set-note">{t("play.settings.own")}</p>
            <SettingRow label={t("play.settings.reach")} hint={t("play.settings.reachHint")}>
              <Segmented
                label={t("play.settings.reach")}
                options={BATTLE_MAT_REACHES.map((value) => ({
                  value,
                  label: t(`options.battleMatReach.${value}`),
                }))}
                value={prefs.battleMatReach}
                onChange={(battleMatReach) => {
                  prefsStore.set({ battleMatReach })
                }}
              />
            </SettingRow>
            <SettingRow label={t("play.settings.look")} hint={t("play.settings.lookHint")}>
              {/* Seven choices are too many for a row of pills on a phone. */}
              <select
                aria-label={t("play.settings.look")}
                value={look}
                className="h-8 w-auto shrink-0 rounded-control border border-border bg-surface-1 px-2 text-text-1"
                onChange={(event) => {
                  const battleMatLook = BATTLE_MAT_LOOKS.find(
                    (entry) => entry === event.target.value,
                  )
                  if (battleMatLook !== undefined) prefsStore.set({ battleMatLook })
                }}
              >
                {BATTLE_MAT_LOOKS.map((entry) => (
                  <option key={entry} value={entry}>
                    {t(`play.settings.looks.${entry}`)}
                  </option>
                ))}
              </select>
            </SettingRow>
            {look === "card" ? (
              <MatCardPicker
                leader={leader}
                onFrame={(printing, initial) => {
                  setCropping({ kind: "card", printing, initial })
                }}
              />
            ) : null}
            {look === "upload" ? (
              <>
                <MatUpload onFrame={frame} />
                {failed ? (
                  <p role="alert" className="bp-set-note is-warning">
                    {t("play.settings.upload.failed")}
                  </p>
                ) : null}
              </>
            ) : null}
            {look === "card" || look === "upload" ? <PhotoSliders /> : null}
          </div>
        )}
      </Dialog>
    </>
  )
}
