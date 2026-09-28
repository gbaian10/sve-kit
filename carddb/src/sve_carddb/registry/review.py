"""Explicit reviewed input receipts; never infer approval from confidence."""

import hashlib
from datetime import date
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from sve_carddb.registry.inputs import Card, Mapping, digest, read_cards, read_mapping


class Correction(BaseModel):
    model_config = ConfigDict(extra="forbid")
    region: Literal["jp", "en"]
    card_no: str
    face_index: int = Field(default=0, ge=0, le=1)
    field: Literal["effect", "card_type"]
    expected_raw_value: str
    corrected_value: str
    image_sha256: str
    locator: str
    state: Literal["active", "needs_review"]
    reason: str
    adoption_scope: Literal["identity_check_only"] | None = None
    source_correction_status: Literal["pending_user_confirmation"] | None = None


class Receipt(BaseModel):
    model_config = ConfigDict(extra="forbid")
    policy: Literal["identity-init-2026-09-28-v1"]
    reviewed_by: str
    reviewed_on: str
    input_hashes: dict[str, str]
    corrections: list[Correction] = Field(default_factory=list)
    reskins: dict[str, str] = Field(default_factory=dict)
    separate_groups: dict[str, str] = Field(default_factory=dict)
    art_groups: list[list[str]] = Field(default_factory=list)


class OriginalArt(BaseModel):
    en_no: str
    verdict: str


class Inputs(BaseModel):
    model_config = ConfigDict(arbitrary_types_allowed=True)
    jp: dict[str, Card]
    en: dict[str, Card]
    mapping: Mapping
    receipt: Receipt


def file_hash(path: Path) -> str:
    """Hash exact local bytes without opening a crawler or manifest database."""
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def read_inputs(paths: dict[str, Path], receipt_path: Path, images: Path) -> Inputs:
    """Require an explicit receipt matching every approved input snapshot."""
    receipt = Receipt.model_validate_json(receipt_path.read_bytes())
    if (
        not receipt.reviewed_by.strip()
        or date.fromisoformat(receipt.reviewed_on).isoformat() != receipt.reviewed_on
    ):
        raise ValueError("Receipt needs an explicit reviewer and complete ISO date")
    if set(receipt.input_hashes) != set(paths):
        raise ValueError("Receipt must cover exactly the supplied input roles")
    for role, path in paths.items():
        if file_hash(path) != receipt.input_hashes[role]:
            raise ValueError(f"Unreviewed input change: {role}")
    jp, en = read_cards(paths["jp"]), read_cards(paths["en"])
    mapping = read_mapping(paths["candidates"], paths["confirmations"])
    _check_art(paths["original_art"], mapping)
    for number, target in receipt.reskins.items():
        if mapping.targets[number] is not None:
            raise ValueError(f"Reskin must have its own identity: {number}")
        mapping.reskins[number] = target
    for correction in receipt.corrections:
        _check_correction(correction, jp if correction.region == "jp" else en, images)
    return Inputs(jp=jp, en=en, mapping=mapping, receipt=receipt)


def _check_correction(
    correction: Correction, cards: dict[str, Card], images: Path
) -> None:
    face = cards[correction.card_no].faces[correction.face_index]
    expected = (
        face.text
        if correction.field == "effect"
        else face.info.get("Card Type", face.card_type)
    )
    if expected != correction.expected_raw_value:
        raise ValueError(f"Stale correction: {correction.card_no}")
    image_parts = Path(face.image).parts[-2:]
    path = images / correction.region / Path(*image_parts)
    if not path.resolve().is_relative_to(images.resolve()):
        raise ValueError("Image path escapes the approved local root")
    if file_hash(path) != correction.image_sha256:
        raise ValueError(f"Changed correction evidence: {correction.card_no}")


def validation_cards(inputs: Inputs) -> tuple[dict[str, Card], dict[str, Card]]:
    """Use image-supported type exceptions only for identity checks.

    Raw observations remain untouched; candidate corrections are not applied to
    published rules or promoted to confirmed corrections.
    """
    jp = {number: card.model_copy(deep=True) for number, card in inputs.jp.items()}
    en = {number: card.model_copy(deep=True) for number, card in inputs.en.items()}
    for correction in inputs.receipt.corrections:
        if correction.field != "card_type":
            continue
        if correction.state == "needs_review" and (
            correction.adoption_scope != "identity_check_only"
            or correction.source_correction_status != "pending_user_confirmation"
        ):
            raise ValueError(
                "Pending type correction needs explicit identity-only scope"
            )
        card = (jp if correction.region == "jp" else en)[correction.card_no]
        face = card.faces[correction.face_index]
        if correction.region == "jp":
            face.card_type = correction.corrected_value
        else:
            face.info["Card Type"] = correction.corrected_value
    return jp, en


def observation(card: Card, region: str) -> dict[str, JsonValue]:
    """Pin the parsed, complete face observations without storing official text."""
    return {
        "region": region,
        "card_no": card.number,
        "observation_hash": digest(card.model_dump(mode="json")),
        "rules_hash": card.rules_hash(),
        "recipe": "registry-observation-v1",
    }


def _check_art(path: Path, mapping: Mapping) -> None:
    reviewed_art = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        row = OriginalArt.model_validate_json(line)
        if row.verdict == "en_original_art":
            reviewed_art.add(row.en_no)
    if not mapping.original_art <= reviewed_art:
        raise ValueError("Original-art confirmation lacks an image comparison record")
