# Deployment design

This directory records confirmed deployment boundaries and storage constraints.
Design requirements do not imply a deployed service: `sim/server/` does not exist
yet, and the engine and web client are prototypes. See the
[project architecture](../../AGENTS.md) and [engine status](../sim/engine-status.md).

| Document | Scope |
| --- | --- |
| [Architecture](architecture.md) | One frontend, CDN and game-service boundaries; admission limits |
| [Matches](matches.md) | Confirmed Durable Object storage, recovery and information constraints |
| [Replays](replays.md) | Replay visibility, version provenance and public asset retention |
| [Cloudflare development](cloudflare-development.md) | Existing development configuration and maintainer-run verification procedure |

The architecture and replay contracts cite the adopted project rules and ADRs.
The explicitly confirmed storage-cleanup and admission requirements are attributed
in the relevant documents; their source was reviewed for
[#473](https://github.com/gbaian10/sve-kit/issues/473).
The remaining proposal is outside these documents' normative scope. The complete
online match lifecycle, archival workflow and backend platform choices require
maintainer confirmation through that issue before becoming specifications.

The development procedure distinguishes offline verification from platform
behavior that still needs a real deployment check. This directory does not
authorize deployment, resource creation, credential setup or R2 uploads.
