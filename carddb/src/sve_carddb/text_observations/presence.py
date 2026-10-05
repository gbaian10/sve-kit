"""Reproducible effect presence evidence for the official JP/EN detail template."""

import re
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, model_validator

from sve_carddb.build_inputs import Version
from sve_carddb.html import attribute, parse, select_all, select_one
from sve_carddb.registry.records import Hash, RecordData, Region, Text, UInt
from sve_carddb.snapshot.values import canonical, digest

if TYPE_CHECKING:
    from selectolax.lexbor import LexborNode

    from sve_carddb.build_inputs import Source

PARSER: Literal["effect-presence-v1/detail-v1"] = "effect-presence-v1/detail-v1"
_TEMPLATE = {"jp": "jp-card-detail-v1", "en": "en-card-detail-v1"}
_REASONS = {
    "present": {"nonempty_container"},
    "absent": {"empty_container", "template_omits_empty_effect"},
    "unknown": {"unrecognized_template", "incomplete_source", "ambiguous_container"},
}
_REQUIRED = (".ttl", ".img img", ".info", ".status")
_JP_NOTICE_HASHES = frozenset(
    {
        "sha256:9619d488281208af272012fb0899e220856405a469def67d3b0f36b9870f66dd",
        "sha256:faa303d69c167609c9ff300a627821e1ac855e574a41ea42e77f64635b300cc0",
        "sha256:79e68e92c200c27fed172ca31566348ede585a57241250204df83881a5eceaf1",
        "sha256:18f1e396ccc9f8f6512257c5d7b4a4a430e9a5a10b80cc2de99a32a277d0caa5",
        "sha256:575100a300e1b580ac2735ab4ce357ab3acf9267fa2025f573a563c5ebbb494a",
    }
)
_LAYOUT = {"div", "p", "span", "br", "-text", "-comment"}
PresenceState = Literal["present", "absent", "unknown"]


class PresenceResult(RecordData):
    recipe: Literal["effect-presence-v1"] = "effect-presence-v1"
    source_version_id: Version
    source_index: UInt
    parser_version: Literal["effect-presence-v1/detail-v1"] = PARSER
    template_id: Text | None
    container_locator: Text
    state: PresenceState
    reason_code: Text

    @model_validator(mode="after")
    def check_state(self) -> PresenceResult:
        """Keep state/reason pairs closed and absence tied to a known template."""
        if self.reason_code not in _REASONS[self.state]:
            raise ValueError("Invalid effect presence state/reason pair")
        if self.template_id is None and self.state != "unknown":
            raise ValueError("Effect presence requires a recognized template")
        return self


class EffectPresence(RecordData):
    result: PresenceResult
    result_hash: Hash

    @model_validator(mode="after")
    def check_hash(self) -> EffectPresence:
        """Evidence hashes cover every field, including template and source locator."""
        if self.result_hash != digest(canonical(self.result.model_dump(mode="json"))):
            raise ValueError("Effect presence result hash mismatch")
        return self

    def value(self) -> dict[str, JsonValue]:
        """Expose the complete hashable evidence without card text."""
        return self.model_dump(mode="json")


def _complete(face: LexborNode, region: Region, number: str) -> bool:
    if any(len(select_all(face, selector)) != 1 for selector in _REQUIRED):
        return False
    title = select_one(face, ".ttl")
    image = select_one(face, ".img img")
    if (
        title is None
        or not title.text(strip=True)
        or image is None
        or not attribute(image, "src")
    ):
        return False
    credit_nodes = select_all(face, ".illustrator")
    if len(credit_nodes) != 1 and not (
        region == "jp"
        and len(credit_nodes) == 2  # ruff: ignore[magic-value-comparison] -- one physical credit followed by one pinned notice
        and digest((credit_nodes[-1].html or "").encode()) in _JP_NOTICE_HASHES
    ):
        return False
    credit = credit_nodes[0]
    identity = select_one(credit, ".name") or select_one(credit, ".heading")
    if identity is None or identity.text(strip=True) != number:
        return False
    info = select_one(face, ".info")
    assert info is not None
    labels = [row.text(strip=True) for row in select_all(info, "dt")]
    expected = (
        {"クラス", "カード種類", "タイプ", "レアリティ"}
        if region == "jp"
        else {"Class", "Card Type", "Trait", "Rarity"}
    )
    return (
        len(labels) == len(set(labels))
        and expected <= set(labels)
        and len(select_all(info, "dt")) == len(select_all(info, "dd"))
        and all(
            len(select_all(face, ".status-Item-" + kind)) == 1
            for kind in ("Cost", "Power", "Hp")
        )
    )


def _omission_state(face: LexborNode) -> tuple[PresenceState, str]:
    # The omission contract requires the terminal credit after intact info/stats.
    parent = select_one(face, ".txt-Inner")
    if parent is None:
        return "unknown", "unrecognized_template"
    text = select_one(face, ".txt")
    if text is None or len(select_all(face, ".txt-Inner")) != 1:
        return "unknown", "unrecognized_template"
    if [
        set((attribute(child, "class") or "").split()) & {"img", "txt"}
        for child in face.iter()
    ] != [{"img"}, {"txt"}] or [
        set((attribute(child, "class") or "").split()) & {"ttl", "txt-Inner"}
        for child in text.iter()
    ] != [{"ttl"}, {"txt-Inner"}]:
        return "unknown", "ambiguous_container"
    children = list(parent.iter())
    allowed = {"info", "status", "speech", "illustrator"}
    if (
        any(
            not set((attribute(child, "class") or "").split()) & allowed
            for child in children
        )
        or any(
            child.tag == "-text" and child.text().strip()
            for child in parent.iter(include_text=True)
        )
        or (
            not children
            or "illustrator" not in (attribute(children[-1], "class") or "").split()
        )
    ):
        return "unknown", "ambiguous_container"
    return "absent", "template_omits_empty_effect"


def _container_state(face: LexborNode) -> tuple[PresenceState, str]:
    containers = select_all(face, ".detail")
    if len(containers) > 1:
        return "unknown", "ambiguous_container"
    if not containers:
        return _omission_state(face)
    container = containers[0]
    descendants = list(container.traverse(include_text=True))
    if any(
        node.tag in {"span", "p", "div", "br"}
        and (
            attribute(node, "style") is not None
            or attribute(node, "class") not in {None, "detail"}
        )
        for node in descendants
    ):
        return "unknown", "ambiguous_container"
    if any(node.tag not in _LAYOUT for node in descendants):
        if any(node.tag not in _LAYOUT | {"img"} for node in descendants):
            return "unknown", "ambiguous_container"
        if any(
            node.tag == "img"
            and (not attribute(node, "src") or not attribute(node, "alt"))
            for node in descendants
        ):
            return "unknown", "ambiguous_container"
    return (
        ("present", "nonempty_container")
        if container.text() or any(node.tag == "img" for node in descendants)
        else ("absent", "empty_container")
    )


def detect_presence(
    raw: bytes, source: Source, *, region: Region, number: str, source_index: int
) -> EffectPresence:
    """Require a whole page and every complete face before proving absence."""
    tree = parse(raw.decode("utf-8", errors="strict"))
    details = select_all(tree, ".cardlist-Detail")
    faces = select_all(tree, ".cardlist-Detail_Box_Inner")
    template: str | None = _TEMPLATE[region] if len(details) == 1 else None
    locator = canonical(
        {
            "faces": ".cardlist-Detail .cardlist-Detail_Box_Inner",
            "source_index": source_index,
            "container": ".detail",
            "omission_parent": ".txt-Inner",
        }
    ).decode()
    state: PresenceState = "unknown"
    reason = "unrecognized_template"
    if template is not None:
        if (
            not re.search(rb"</body>\s*</html>\s*$", raw, re.IGNORECASE)
            or not 1 <= len(faces) <= 2  # ruff: ignore[magic-value-comparison] -- official pages have one or two physical faces
            or not 0 <= source_index < len(faces)
            or any(not _complete(face, region, number) for face in faces)
        ):
            state, reason = "unknown", "incomplete_source"
        else:
            if region == "jp" and any(
                len(select_all(face, ".illustrator")) > 1 for face in faces
            ):
                template = "jp-card-detail-notice-v1"
            state, reason = _container_state(faces[source_index])
    if reason == "unrecognized_template":
        template = None
    result = PresenceResult(
        source_version_id=source.id,
        source_index=source_index,
        template_id=template,
        container_locator=locator,
        state=state,
        reason_code=reason,
    )
    return EffectPresence(
        result=result, result_hash=digest(canonical(result.model_dump(mode="json")))
    )
