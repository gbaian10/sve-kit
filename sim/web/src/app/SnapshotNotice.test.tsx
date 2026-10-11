import { act, fireEvent, screen } from "@testing-library/react"
import { expect, it, vi } from "vitest"

import type { SnapshotClient, SnapshotStatus } from "../data"
import { UI_LANGUAGES } from "../i18n/languages"
import { renderInRouter, renderRoutes } from "../test-utils"
import { SnapshotNotice } from "./SnapshotNotice"

it.each(UI_LANGUAGES)(
  "marks a compatible previous/local active as older in %s and offers a retry",
  async (language) => {
    const reload = vi.fn(() => Promise.resolve())
    const client = { reload } as unknown as SnapshotClient
    const status: SnapshotStatus = {
      state: "ready",
      dataVersion: "synthetic",
      outdated: true,
      updateError: { kind: "incompatible", detail: "test" },
    }
    const { i18n } = await renderRoutes(
      [{ path: "*", element: <SnapshotNotice client={client} status={status} /> }],
      { language },
    )
    expect(screen.getByRole("status")).toHaveTextContent(i18n.t("snapshot.stale"))
    expect(screen.getByRole("status")).toHaveTextContent(i18n.t("snapshot.updateApp"))
    fireEvent.click(screen.getByRole("button", { name: i18n.t("snapshot.retry") }))
    expect(reload).toHaveBeenCalledTimes(1)
  },
)
it("does not label a verified current snapshot as older", async () => {
  await renderInRouter(
    <SnapshotNotice
      client={{} as SnapshotClient}
      status={{ state: "ready", dataVersion: "synthetic" }}
    />,
  )
  expect(screen.queryByRole("status")).not.toBeInTheDocument()
})

it("marks offline active data without offering a network retry, and reacts to reconnect", async () => {
  const online = vi.spyOn(navigator, "onLine", "get").mockReturnValue(false)
  try {
    const { i18n } = await renderRoutes([
      {
        path: "*",
        element: (
          <SnapshotNotice
            client={{} as SnapshotClient}
            status={{ state: "ready", dataVersion: "synthetic" }}
          />
        ),
      },
    ])
    expect(screen.getByRole("status")).toHaveTextContent(i18n.t("snapshot.offline"))
    expect(screen.queryByRole("button")).not.toBeInTheDocument()
    online.mockReturnValue(true)
    act(() => {
      window.dispatchEvent(new Event("online"))
    })
    expect(screen.queryByRole("status")).not.toBeInTheDocument()
  } finally {
    online.mockRestore()
  }
})

it.each(UI_LANGUAGES)(
  "shows background confirmation without claiming local data is older in %s",
  async (language) => {
    const { i18n } = await renderRoutes(
      [
        {
          path: "*",
          element: (
            <SnapshotNotice
              client={{} as SnapshotClient}
              status={{
                state: "ready",
                dataVersion: "synthetic",
                unconfirmed: true,
                updating: "index",
              }}
            />
          ),
        },
      ],
      { language },
    )
    expect(screen.getByRole("status")).toHaveTextContent(i18n.t("snapshot.checking"))
    expect(screen.getByRole("status")).not.toHaveTextContent(i18n.t("snapshot.stale"))
    expect(screen.queryByRole("button")).not.toBeInTheDocument()
  },
)

it.each(
  UI_LANGUAGES.flatMap((language) => [
    { language, online: true },
    { language, online: false },
  ]),
)(
  "distinguishes an unconfirmed network failure in $language (online: $online)",
  async ({ language, online }) => {
    const connection = vi.spyOn(navigator, "onLine", "get").mockReturnValue(online)
    try {
      const reload = vi.fn(() => Promise.resolve())
      const client = { reload } as unknown as SnapshotClient
      const status: SnapshotStatus = {
        state: "ready",
        dataVersion: "synthetic",
        unconfirmed: true,
        outdated: true,
        updateError: { kind: "network", detail: "offline request" },
      }
      const { i18n } = await renderRoutes(
        [{ path: "*", element: <SnapshotNotice client={client} status={status} /> }],
        { language },
      )
      expect(screen.getByRole("status")).toHaveTextContent(i18n.t("snapshot.unavailable"))
      expect(screen.getByRole("status")).not.toHaveTextContent(i18n.t("snapshot.stale"))
      if (online) {
        fireEvent.click(screen.getByRole("button", { name: i18n.t("snapshot.retry") }))
        expect(reload).toHaveBeenCalledTimes(1)
      } else expect(screen.queryByRole("button")).not.toBeInTheDocument()
    } finally {
      connection.mockRestore()
    }
  },
)

it.each(UI_LANGUAGES)(
  "keeps existing notices for confirmed network failures and other error kinds in %s",
  async (language) => {
    for (const failure of [
      { kind: "network", unconfirmed: false },
      { kind: "corrupt", unconfirmed: true },
      { kind: "incompatible", unconfirmed: true },
    ] as const) {
      const status: SnapshotStatus = {
        state: "ready",
        dataVersion: "synthetic",
        outdated: true,
        unconfirmed: failure.unconfirmed,
        updateError: { kind: failure.kind, detail: "test" },
      }
      const { i18n, unmount } = await renderRoutes(
        [
          {
            path: "*",
            element: (
              <SnapshotNotice
                client={{ reload: () => Promise.resolve() } as unknown as SnapshotClient}
                status={status}
              />
            ),
          },
        ],
        { language },
      )
      expect(screen.getByRole("status")).toHaveTextContent(i18n.t("snapshot.stale"))
      expect(screen.getByRole("status")).not.toHaveTextContent(i18n.t("snapshot.unavailable"))
      if (failure.kind === "incompatible")
        expect(screen.getByRole("status")).toHaveTextContent(i18n.t("snapshot.updateApp"))
      unmount()
    }
  },
)
