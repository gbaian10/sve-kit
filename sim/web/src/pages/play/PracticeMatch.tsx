import { ArrowLeft } from "lucide-react"
import { useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { Link } from "react-router"

import type { HostMessage } from "../../battle/contract"
import { BATTLE_CDN } from "../../battle/engine/assets"
import { startLocalBattle } from "../../battle/engine/client"
import type { BattleBuild } from "../../battle/engine/protocol"
import type { BattleSource } from "../../battle/source"
import { createSnapshotClient } from "../../data"
import { BattleScreen } from "./BattleScreen"

export function PracticeMatch({ build }: { readonly build: BattleBuild }) {
  const { t } = useTranslation()
  const [source, setSource] = useState<BattleSource>()
  const [opened, setOpened] = useState(false)
  const [failed, setFailed] = useState(false)
  const client = useMemo(
    () => createSnapshotClient(BATTLE_CDN, { region: "jp", pinnedManifest: build.manifest }),
    [build.manifest],
  )

  useEffect(() => {
    // The first question is prepared while the match is in the lobby. Nothing answers it
    // until the player enters. The policy seat never has a main-thread port.
    const battle = startLocalBattle(build.options)
    const channel = battle.channels.p1
    if (!channel) {
      battle.terminate()
      throw new Error("practice match needs a player seat")
    }
    const listeners = new Set<(message: HostMessage) => void>()
    const history: HostMessage[] = []
    const source: BattleSource = {
      seat: channel.seat,
      submit: (command) => channel.submit(command),
      listen(listener) {
        history.forEach(listener)
        listeners.add(listener)
        return () => listeners.delete(listener)
      },
      close() {
        battle.terminate()
      },
    }
    const unsubscribe = channel.listen((incoming) => {
      if (incoming.kind === "baseline_start") return
      const message: HostMessage =
        incoming.kind === "load_error"
          ? { kind: "host_error", format: 1, reason: "projection" }
          : incoming
      history.push(message)
      listeners.forEach((listener) => {
        listener(message)
      })
      if (
        message.kind === "host_error" ||
        (message.kind === "update" && message.input.kind === "engine_fault")
      ) {
        setFailed(true)
      }
      if (message.kind === "update" && message.input.kind === "question") setSource(source)
    })
    void client.load()
    return () => {
      unsubscribe()
      source.close()
    }
  }, [build, client])

  const leave = (
    <Link to="/" className="bp-chip-link">
      <ArrowLeft aria-hidden="true" className="bp-chip-icon" />
      {t("play.leave")}
    </Link>
  )
  return (
    <main>
      <h1 className="sr-only">{t("play.title")}</h1>
      {opened && source !== undefined ? (
        <BattleScreen
          source={source}
          leaders={build.leaders}
          client={client}
          corner={leave}
          practice
        />
      ) : (
        <div className="bp-root">
          <section className="bp-notice content-center justify-items-center gap-3">
            <button
              type="button"
              className="bp-chip-link"
              disabled={source === undefined || failed}
              onClick={() => {
                setOpened(true)
              }}
            >
              {t("play.practice.start")}
            </button>
            {failed ? (
              <span role="alert">{t("play.stopped")}</span>
            ) : source === undefined ? (
              <span role="status">{t("play.loading")}</span>
            ) : null}
            <div className="flex gap-3">
              {leave}
              <Link to="/play?scene=preset" className="bp-chip-link">
                {t("play.practice.fixtures")}
              </Link>
            </div>
          </section>
        </div>
      )}
    </main>
  )
}
