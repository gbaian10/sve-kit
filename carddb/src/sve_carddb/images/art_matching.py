"""Reproducible same-face illustration candidates, never automatic adoption."""

from collections import defaultdict
from dataclasses import dataclass
from itertools import combinations
from typing import TYPE_CHECKING, Literal

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import Hash, RecordData, Text, UInt
from sve_carddb.core.regions import Region

if TYPE_CHECKING:
    from pathlib import Path

RECIPE: Literal["sift800-ratio075-ransac5-v1"] = "sift800-ratio075-ransac5-v1"
SAME_MIN = 40
REVIEW_MIN = 10
REVIEW_MAX = 100


class ArtMember(RecordData):
    printing_id: Text
    face_id: Text
    card_id: Text
    region: Region
    card_no: Text
    int_id: UInt
    ordinal: UInt
    image_sha256: Hash

    @property
    def key(self) -> str:
        """Keep face identity explicit, including double-faced printings."""
        return self.printing_id + "/" + self.face_id


class ArtScore(RecordData):
    left: Text
    right: Text
    inliers: UInt


class ArtGroup(RecordData):
    face_id: Text
    members: tuple[Text, ...]


class ArtReview(ArtScore):
    scope: Literal["same_language", "cross_language"]


class ArtCandidates(RecordData):
    format: Literal["sve-art-candidates-v1"] = "sve-art-candidates-v1"
    recipe: Literal["sift800-ratio075-ransac5-v1"] = RECIPE
    opencv_version: Text
    image_index_sha256: Hash
    members: tuple[ArtMember, ...]
    unavailable: tuple[Text, ...]
    scores: tuple[ArtScore, ...]
    groups: tuple[ArtGroup, ...]
    review: tuple[ArtReview, ...]
    conflicts: tuple[ArtScore, ...]

    def content(self) -> bytes:
        """Write canonical JSON without local paths or source wording."""
        return canonical(self.model_dump(mode="json"))

    @classmethod
    def read(cls, path: Path) -> ArtCandidates:
        """Reject incomplete caches before reusing any previous scores."""
        result = cls.model_validate_json(path.read_bytes())
        result.validate_graph()
        return result

    def validate_graph(self) -> None:
        """Require complete same-face pair coverage and reproducible projections."""
        members = {member.key: member for member in self.members}
        if len(members) != len(self.members):
            raise ValueError("Duplicate art candidate member")
        faces: dict[str, list[str]] = defaultdict(list)
        for member in self.members:
            faces[member.face_id].append(member.key)
        expected = {
            tuple(sorted(pair))
            for keys in faces.values()
            for pair in combinations(keys, 2)
        }
        actual = {tuple(sorted((s.left, s.right))) for s in self.scores}
        if actual != expected or len(actual) != len(self.scores):
            raise ValueError("Art scores require every same-face pair exactly once")
        positions = {member.key: index for index, member in enumerate(self.members)}
        if any(
            positions[score.left] >= positions[score.right] for score in self.scores
        ):
            raise ValueError("Art scores must retain the image comparison direction")
        groups, review, conflicts = _group(self.members, self.scores)
        if (groups, review, conflicts) != (self.groups, self.review, self.conflicts):
            raise ValueError("Art candidate projections disagree with scores")


@dataclass(frozen=True)
class ArtInput:
    member: ArtMember
    path: Path


@dataclass(frozen=True)
class ArtComparison:
    candidates: ArtCandidates
    compared_pairs: int
    reused_pairs: int


def compare_art(
    inputs: tuple[ArtInput, ...],
    *,
    image_index_sha256: str,
    unavailable: tuple[str, ...] = (),
    previous: ArtCandidates | None = None,
) -> ArtComparison:
    """Compare only new pairs; changed/removed inputs require a fresh full run."""
    from sve_carddb.images.art_sift import opencv_version  # ruff: ignore[import-outside-top-level] -- keep optional OpenCV out of ordinary CLI imports

    ordered = sorted(inputs, key=lambda item: (item.member.int_id, item.member.ordinal))
    members = tuple(item.member for item in ordered)
    cache = _previous(previous, members, opencv_version())
    by_face: dict[str, list[ArtInput]] = defaultdict(list)
    for item in ordered:
        by_face[item.member.face_id].append(item)
    scores: list[ArtScore] = []
    reused = 0
    for face in by_face.values():
        compared, hits = _compare_face(face, cache)
        scores.extend(compared)
        reused += hits
    scored = tuple(scores)
    groups, review, conflicts = _group(members, scored)
    result = ArtCandidates(
        opencv_version=opencv_version(),
        image_index_sha256=image_index_sha256,
        members=members,
        unavailable=unavailable,
        scores=scored,
        groups=groups,
        review=review,
        conflicts=conflicts,
    )
    result.validate_graph()
    return ArtComparison(result, len(scores) - reused, reused)


def _compare_face(
    face: list[ArtInput], cache: dict[tuple[str, ...], int]
) -> tuple[list[ArtScore], int]:
    from sve_carddb.images.art_sift import features, inliers  # ruff: ignore[import-outside-top-level] -- optional comparison backend

    extracted = {}
    scores = []
    reused = 0
    for left, right in combinations(face, 2):
        key = tuple(sorted((left.member.key, right.member.key)))
        if key in cache:
            score = cache[key]
            reused += 1
        else:
            for item in (left, right):
                if item.member.key not in extracted:
                    raw = item.path.read_bytes()
                    if digest(raw) != item.member.image_sha256:
                        raise ValueError("Art bitmap changed during comparison")
                    extracted[item.member.key] = features(raw)
            score = inliers(extracted[left.member.key], extracted[right.member.key])
        scores.append(
            ArtScore(left=left.member.key, right=right.member.key, inliers=score)
        )
    return scores, reused


def _previous(
    previous: ArtCandidates | None, members: tuple[ArtMember, ...], version: str
) -> dict[tuple[str, ...], int]:
    current = {member.key: member for member in members}
    if len(current) != len(members):
        raise ValueError("Duplicate art comparison member")
    if previous is None:
        return {}
    previous.validate_graph()
    if previous.opencv_version != version or any(
        current.get(member.key) != member for member in previous.members
    ):
        raise ValueError(
            "Previous art inputs or OpenCV changed; run without --previous"
        )
    return {
        tuple(sorted((score.left, score.right))): score.inliers
        for score in previous.scores
    }


def _group(
    members: tuple[ArtMember, ...], scores: tuple[ArtScore, ...]
) -> tuple[tuple[ArtGroup, ...], tuple[ArtReview, ...], tuple[ArtScore, ...]]:
    by_key = {member.key: member for member in members}
    parent = {member.key: member.key for member in members}

    def find(key: str) -> str:
        while parent[key] != key:
            parent[key] = parent[parent[key]]
            key = parent[key]
        return key

    for score in scores:
        if score.inliers >= SAME_MIN:
            parent[find(score.left)] = find(score.right)
    groups: dict[str, list[str]] = defaultdict(list)
    for member in members:
        groups[find(member.key)].append(member.key)
    return (
        tuple(
            ArtGroup(face_id=by_key[keys[0]].face_id, members=tuple(keys))
            for keys in groups.values()
        ),
        tuple(
            ArtReview(
                **score.model_dump(),
                scope=(
                    "same_language"
                    if by_key[score.left].region == by_key[score.right].region
                    else "cross_language"
                ),
            )
            for score in scores
            if REVIEW_MIN <= score.inliers < REVIEW_MAX
        ),
        tuple(
            score
            for score in scores
            if score.inliers < REVIEW_MIN and find(score.left) == find(score.right)
        ),
    )
