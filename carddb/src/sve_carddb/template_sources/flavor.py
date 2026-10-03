"""Independent flavor-exact-v1 recipe; narrative punctuation never becomes rules."""

from dataclasses import dataclass
from typing import Literal

from sve_carddb.snapshot.values import digest

VERSION = "flavor-exact-v1"
CODE_PATH = "carddb/src/sve_carddb/template_sources/flavor.py"
# The contract fixes this set rather than relying on the installed Unicode database.
WHITE_SPACE: frozenset[int] = frozenset(
    (
        *range(0x0009, 0x000E),
        0x0020,
        0x0085,
        0x00A0,
        0x1680,
        *range(0x2000, 0x200B),
        0x2028,
        0x2029,
        0x202F,
        0x205F,
        0x3000,
    )
)
type FlavorState = Literal["unknown", "empty", "whitespace_only", "present"]


@dataclass(frozen=True)
class Segment:
    start: int
    end: int


@dataclass(frozen=True)
class FlavorPart:
    normalized: str
    member_source: str
    segments: tuple[Segment, ...]
    role: Literal["flavor"] = "flavor"
    line_ordinal: Literal[0] = 0
    anchor: None = None
    legacy_id: None = None
    slots: tuple[()] = ()

    @property
    def normalized_hash(self) -> str:
        """Hash exact UTF-8 rather than a display or classification representation."""
        return digest(self.normalized.encode("utf-8"))

    @property
    def template(self) -> None:
        """A flavor payload receives its new ID only after complete payload validation."""
        return None


@dataclass(frozen=True)
class FlavorResult:
    state: FlavorState
    parts: tuple[FlavorPart, ...]

    @property
    def part(self) -> FlavorPart | None:
        """Only one full-field narrative part exists, never a first-line shortcut."""
        return self.parts[0] if self.parts else None


def normalize(text: str) -> str:
    """Preserve every source code point, while rejecting invalid boundary values."""
    if not isinstance(text, str):
        raise ValueError("Flavor text must be a string")  # ruff: ignore[type-check-without-type-error] -- agreed flavor boundary API uses ValueError
    try:
        text.encode("utf-8")
    except UnicodeEncodeError:
        raise ValueError("Flavor text must be valid UTF-8") from None
    return text


def partition(text: str | None) -> FlavorResult:
    """Keep unknown, exact empty and known whitespace separate from narrative text."""
    if text is None:
        return FlavorResult("unknown", ())
    raw = normalize(text)
    if not raw:
        return FlavorResult("empty", ())
    if all(ord(char) in WHITE_SPACE for char in raw):
        return FlavorResult("whitespace_only", ())
    return FlavorResult("present", (FlavorPart(raw, raw, (Segment(0, len(raw)),)),))
