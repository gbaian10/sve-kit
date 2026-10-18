"""Exact source-position candidates without granting semantic or runtime approval."""

from collections import defaultdict
from dataclasses import InitVar, dataclass, field
from typing import TYPE_CHECKING, Literal

from sve_carddb.core.json import digest

if TYPE_CHECKING:
    from sve_carddb.contracts.source_binding import SourceDescriptor, SourceSpan
    from sve_carddb.domains.effects.classifications import Member, Position


@dataclass(frozen=True)
class FrameOccurrence:
    position: Position
    card_id: str
    face_id: str
    frame_id: str
    binding_id: str
    normalizer: str
    source: SourceDescriptor
    span: SourceSpan
    current: bool
    raw: InitVar[str]
    body_hash: str = field(init=False)

    def __post_init__(self, raw: str) -> None:
        """Adapters resolve owner identity; the matcher independently checks exact bytes."""
        if (
            self.source.owner.kind != "face_revision"
            or self.source.field != self.position.field
            or self.source.ordinal != self.position.ordinal
            or self.source.source_hash != digest(raw.encode())[7:]
            or self.span.role != "body"
        ):
            raise ValueError("Frame occurrence differs from exact source field")
        if any(
            s.end > len(raw)
            or raw[: s.start].count("\n") != self.position.line
            or "\n" in raw[s.start : s.end]
            for s in self.span.segments
        ):
            raise ValueError("Frame occurrence differs from exact source line")
        object.__setattr__(
            self,
            "body_hash",
            digest("".join(raw[s.start : s.end] for s in self.span.segments).encode()),
        )


Status = Literal["current", "historical", "source_changed", "no_candidate", "ambiguous"]


@dataclass(frozen=True)
class Match:
    member_id: str
    status: Status
    candidates: tuple[FrameOccurrence, ...]
    exact: tuple[FrameOccurrence, ...]
    selected: tuple[FrameOccurrence, ...]


class Matcher:
    """Index by owner coordinates before considering source bytes or version preference."""

    def __init__(self, frames: tuple[FrameOccurrence, ...]) -> None:
        self._positions: dict[Position, list[FrameOccurrence]] = defaultdict(list)
        seen: dict[tuple[Position, str], FrameOccurrence] = {}
        for frame in frames:
            key = (frame.position, frame.binding_id)
            if key in seen:
                if seen[key] != frame:
                    raise ValueError("Conflicting frame binding evidence")
                continue
            seen[key] = frame
            self._positions[frame.position].append(frame)

    def match(self, member: Member) -> Match:
        """All exact history survives even when a unique current candidate is preferred."""
        candidates = tuple(self._positions.get(member.position, ()))
        exact = tuple(c for c in candidates if c.body_hash == member.body_hash)
        selected = tuple(c for c in exact if c.current) or exact
        status: Status
        if len(selected) > 1 or len({(c.card_id, c.face_id) for c in candidates}) > 1:
            status = "ambiguous"
        elif selected:
            status = "current" if selected[0].current else "historical"
        else:
            status = "source_changed" if candidates else "no_candidate"
        return Match(member.id, status, candidates, exact, selected)
