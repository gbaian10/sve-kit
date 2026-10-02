"""Versioned JP credit variants; v1 evidence remains reproducible and unchanged."""

# ruff: file-ignore[private-member-access] -- reuse frozen v1 guards without changing their pinned implementation

import re
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, model_validator

from sve_carddb.build_inputs import Version  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves source identifiers
from sve_carddb.html import attribute, parse, select_all, select_one
from sve_carddb.registry.records import Hash, RecordData, Region, Text, UInt
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.text_observations import presence as v1

if TYPE_CHECKING:
    from selectolax.lexbor import LexborNode

    from sve_carddb.build_inputs import Source

PARSER: Literal["effect-presence-v2/detail-v2"] = "effect-presence-v2/detail-v2"
CODE_PATH = "carddb/src/sve_carddb/text_observations/presence_v2.py"
NOTICE_HASHES = frozenset(
    {
        "sha256:0792f0d623bc47ea05b4274913bd6428a2a4b1e0ac4c90482a254f81cf25d1fc",
        "sha256:0f336c8d27b0391c2661bdba8f1d33cae3959b4a5478fe6c214fdc15959dbd25",
        "sha256:129dcef9d9bf131c2e07391edbc8977e0f691f1c31640839989a7887bc791b45",
        "sha256:22cdd5d2675012e94df44abfc500a45b6e223b36da9226e6d261fb89f157b898",
        "sha256:35c9ea4278c4d1023980ae3cb419ae924db504d186dd7f8aec4f8078ad681d27",
        "sha256:43b3ec026a55dbbb70bf5cfeacba8535dff6eaa823a07e3877ce90d7f191b1de",
        "sha256:47f0e44349c31229448848cf5e6ce63c0b1c15f79f192adb052ee9075dbb1b9c",
        "sha256:49897f25896896097cbf6ab473670a3000f2c91acb357c357c979ebec1c9a568",
        "sha256:4d7d539caecc6fb5b7221e905158fece7d72d9ea9dba0792f5c578e7c83b2e98",
        "sha256:588391d6462845b3f3184234f358a7ab946e391d763fafb4a89872151ab514b8",
        "sha256:717f2d2bef81c593ea45475741c1ace73a3e9ef51d879289e46ede3d2da1e36b",
        "sha256:74a2c349c3b1713014036155a25a116d51ca01c89b50e6d9dfc02fa2ce2a64b1",
        "sha256:a4fa300fd2dbe66659eb54d81ecab59286ebc5fa6e8c8318835f8e4487074f16",
        "sha256:adc81475c7c4f664b8612b379002fb92be0ab54d14922e9da1b571557bbdd2ac",
        "sha256:c41575ddfa471eaf4924e91e592a5b7f2d4e004418ebeece14cf404ebeccc752",
        "sha256:d852db215c5703829073e3ac09839dfe309061a770117394cb8e35967d66a526",
        "sha256:d8f632ad80ac757554ec85c463386ac698e5179346d368d171fe107e4aea9712",
        "sha256:e6d38db37ad7816872a3196e05359b801492a08584c8e5bd4f0edd1828fd1167",
        "sha256:f6cc52ba9ecbf85edb43d0012ab41b3705f9ed116354407228408e3bec341285",
        "sha256:f7d438fc61b795922aab4d2538eba73b3c5fe325080ddd74f2682bf666d2b083",
    }
)


class PresenceResult(RecordData):
    recipe: Literal["effect-presence-v2"] = "effect-presence-v2"
    source_version_id: Version
    source_index: UInt
    parser_version: Literal["effect-presence-v2/detail-v2"] = PARSER
    template_id: Text | None
    container_locator: Text
    state: v1.PresenceState
    reason_code: Text

    @model_validator(mode="after")
    def check_result(self) -> PresenceResult:
        """Keep the v1 state/reason contract while separating the recipe identity."""
        if self.reason_code not in v1._REASONS[self.state]:
            raise ValueError("Invalid v2 effect presence state/reason pair")
        if self.template_id is None and self.state != "unknown":
            raise ValueError("V2 effect presence requires a recognized template")
        return self


class EffectPresence(RecordData):
    result: PresenceResult
    result_hash: Hash

    @model_validator(mode="after")
    def check_hash(self) -> EffectPresence:
        """Pin every result field, including the new recipe and parser versions."""
        if self.result_hash != digest(canonical(self.result.model_dump(mode="json"))):
            raise ValueError("V2 effect presence result hash mismatch")
        return self

    def value(self) -> dict[str, JsonValue]:
        """Expose evidence without copying any official source text."""
        return self.model_dump(mode="json")


def _basic(face: LexborNode) -> bool:
    if any(len(select_all(face, query)) != 1 for query in v1._REQUIRED):
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
    labels = [node.text(strip=True) for node in select_all(face, ".info dt")]
    return (
        len(labels) == len(set(labels))
        and {"クラス", "カード種類", "タイプ", "レアリティ"} <= set(labels)
        and len(labels) == len(select_all(face, ".info dd"))
        and all(
            len(select_all(face, ".status-Item-" + kind)) == 1
            for kind in ("Cost", "Power", "Hp")
        )
    )


def _numbered_credit(face: LexborNode, number: str) -> bool:
    credit_nodes = select_all(face, ".illustrator")
    if not credit_nodes or not _basic(face):
        return False
    identity = select_one(credit_nodes[0], ".name") or select_one(
        credit_nodes[0], ".heading"
    )
    if identity is None or identity.text(strip=True) != number:
        return False
    if len(credit_nodes) == 1:
        return True
    if len(credit_nodes) != 2:  # ruff: ignore[magic-value-comparison] -- one physical credit and one notice envelope
        return False
    parent = select_one(face, ".txt-Inner")
    return (
        parent is not None
        and list(parent.iter())[-2:] == credit_nodes
        and select_one(credit_nodes[-1], ".name") is None
        and select_one(credit_nodes[-1], ".heading") is None
        and digest((credit_nodes[-1].html or "").encode())
        in NOTICE_HASHES | v1._JP_NOTICE_HASHES
    )


def _back_credit(face: LexborNode) -> bool:
    # Newly recognized back variants prove presence only, never additional absence.
    parent = select_one(face, ".txt-Inner")
    if (
        not _basic(face)
        or v1._container_state(face)[0] != "present"
        or parent is None
        or len(select_all(face, ".txt-Inner")) != 1
    ):
        return False
    if any(
        not set((attribute(child, "class") or "").split())
        & {"info", "status", "detail", "speech", "illustrator"}
        for child in parent.iter()
    ) or any(
        node.tag == "-text" and node.text().strip()
        for node in parent.iter(include_text=True)
    ):
        return False
    credit_nodes = select_all(face, ".illustrator")
    if not credit_nodes:
        return True
    if (
        len(credit_nodes) != 1
        or select_one(credit_nodes[0], ".name") is not None
        or not list(parent.iter())
        or list(parent.iter())[-1] != credit_nodes[0]
    ):
        return False
    children = list(credit_nodes[0].iter())
    if (
        len(children) != 1
        or children[0].tag != "span"
        or attribute(children[0], "class") != "heading"
        or list(children[0].iter())
    ):
        return False
    artist = children[0].text(strip=True)
    return bool(artist) and all(char.isalpha() or char.isspace() for char in artist)


def detect_presence(
    raw: bytes, source: Source, *, region: Region, number: str, source_index: int
) -> EffectPresence:
    """Reuse v1 unchanged; only recognize explicitly bounded JP credit variants."""
    previous = v1.detect_presence(
        raw, source, region=region, number=number, source_index=source_index
    ).result
    state, reason, template = previous.state, previous.reason_code, previous.template_id
    if region == "jp" and state == "unknown" and reason == "incomplete_source":
        tree = parse(raw.decode("utf-8", errors="strict"))
        faces = select_all(tree, ".cardlist-Detail_Box_Inner")
        if (
            re.search(rb"</body>\s*</html>\s*$", raw, re.IGNORECASE)
            and len(select_all(tree, ".cardlist-Detail")) == 1
            and 1 <= len(faces) <= 2  # ruff: ignore[magic-value-comparison] -- official pages have one or two faces
            and 0 <= source_index < len(faces)
            and all(
                _numbered_credit(face, number) or (index == 1 and _back_credit(face))
                for index, face in enumerate(faces)
            )
        ):
            candidate_state, candidate_reason = v1._container_state(faces[source_index])
            # New page variants may prove presence, never additional absence.
            if candidate_state == "present":
                state, reason = candidate_state, candidate_reason
                template = "jp-card-detail-credit-v2"
    if template is not None and template.endswith("-v1"):
        template = template[:-3] + "-v2"
    if reason == "unrecognized_template":
        template = None
    result = PresenceResult(
        source_version_id=source.id,
        source_index=source_index,
        template_id=template,
        container_locator=previous.container_locator,
        state=state,
        reason_code=reason,
    )
    return EffectPresence(
        result=result, result_hash=digest(canonical(result.model_dump(mode="json")))
    )
