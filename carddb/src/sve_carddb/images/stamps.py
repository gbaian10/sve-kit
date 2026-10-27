"""Optional image comparison for human review of printed marks and serial plaques."""

# ruff: file-ignore[magic-value-comparison] -- pixel and residual thresholds belong to this fixed normalized-image recipe

from dataclasses import dataclass
from typing import TYPE_CHECKING, cast

import cv2
import numpy as np

if TYPE_CHECKING:
    from numpy.typing import NDArray

WIDTH = 459
HEIGHT = 641
MIN_MATCHES = 12
MIN_INLIERS = 10
MAX_BACKGROUND = 24
NEIGHBOURS = 2


@dataclass(frozen=True)
class MarkBox:
    x: int
    y: int
    width: int
    height: int
    low_contrast: bool = False


@dataclass(frozen=True)
class Detection:
    boxes: tuple[MarkBox, ...]
    method: str
    reference_index: int | None
    inliers: int = 0


@dataclass(frozen=True)
class MarkPattern:
    mask: NDArray[np.uint8]
    label: str


@dataclass(frozen=True)
class PatternMatch:
    label: str
    score: float
    box: MarkBox


def mark_pattern(raw: bytes, box: MarkBox, *, label: str) -> MarkPattern:
    """Extract a reviewer-selected pattern at runtime; no official template is bundled."""
    image = bitmap(raw)
    if min(box.x, box.y) < 0 or min(box.width, box.height) <= 0:
        raise ValueError("Mark example box is outside the image")
    crop = image[box.y : box.y + box.height, box.x : box.x + box.width]
    if crop.size == 0 or crop.shape[:2] != (box.height, box.width):
        raise ValueError("Mark example box is outside the image")
    hsv = cv2.cvtColor(crop, cv2.COLOR_BGR2HSV)
    gold = cv2.inRange(hsv, (12, 25, 70), (45, 255, 255))
    if int(np.count_nonzero(gold)) < 12 or not np.any(gold != gold.flat[0]):
        raise ValueError("Example needs a visible warm-coloured ornament")
    return MarkPattern(cast("NDArray[np.uint8]", gold), label)


def match_patterns(
    raw: bytes, patterns: tuple[MarkPattern, ...]
) -> tuple[PatternMatch, ...]:
    """Suggest matching gold or silver marks anywhere in the illustration, including faint plaques."""
    image = bitmap(raw)
    hsv = cv2.cvtColor(image, cv2.COLOR_BGR2HSV)
    masks = (
        cv2.inRange(hsv, (12, 25, 70), (45, 255, 255)),
        cv2.inRange(hsv, (0, 0, 80), (180, 60, 235)),
    )
    matches = []
    for pattern in patterns:
        best: PatternMatch | None = None
        for scale in (0.65, 0.8, 1.0, 1.15, 1.3):
            template = cv2.resize(
                pattern.mask, None, fx=scale, fy=scale, interpolation=cv2.INTER_NEAREST
            )
            height, width = template.shape[:2]
            if height >= 415 or width >= 405:
                continue
            for mask in masks:
                scores = cv2.matchTemplate(
                    mask[60:475, 30:435], template, cv2.TM_CCOEFF_NORMED
                )
                _, score, _, (x, y) = cv2.minMaxLoc(scores)
                if score >= 0.55 and (best is None or score > best.score):
                    best = PatternMatch(
                        pattern.label,
                        float(score),
                        MarkBox(x + 30, y + 60, width, height, True),
                    )
        if best is not None:
            matches.append(best)
    return tuple(matches)


class MarkGroups:
    """Small visual groups help a reviewer inspect similar local differences together."""

    def __init__(self) -> None:
        self.centres: list[NDArray[np.float32]] = []

    def add(self, image: NDArray[np.uint8], box: MarkBox) -> int:
        """Return a provisional shape group; matching never establishes a mark's meaning."""
        crop = image[box.y : box.y + box.height, box.x : box.x + box.width]
        edges = (
            cv2.Canny(cv2.resize(crop, (48, 32)), 45, 110).astype(np.float32).ravel()
        )
        norm = float(np.linalg.norm(edges))
        if norm:
            edges /= norm
        for index, centre in enumerate(self.centres):
            if float(np.dot(edges, centre)) >= 0.86:
                return index + 1
        self.centres.append(edges)
        return len(self.centres)


def bitmap(raw: bytes) -> NDArray[np.uint8]:
    """Normalize source resolution without relying on a card-number filename."""
    image = cv2.imdecode(np.frombuffer(raw, dtype=np.uint8), cv2.IMREAD_COLOR)
    if image is None:
        raise ValueError("Cannot decode card image")
    return cast(
        "NDArray[np.uint8]",
        cv2.resize(image, (WIDTH, HEIGHT), interpolation=cv2.INTER_AREA),
    )


def _features(
    image: NDArray[np.uint8],
) -> tuple[NDArray[np.float32], NDArray[np.float32] | None]:
    mask = np.zeros((HEIGHT, WIDTH), np.uint8)
    mask[55:390, 45:420] = 255
    detector = cv2.SIFT.create(nfeatures=850, contrastThreshold=0.025)
    points, descriptors = detector.detectAndCompute(
        cv2.cvtColor(image, cv2.COLOR_BGR2GRAY), mask
    )
    return np.asarray([point.pt for point in points], dtype=np.float32), cast(
        "NDArray[np.float32] | None", descriptors
    )


def _warp(
    target: NDArray[np.uint8], reference: NDArray[np.uint8]
) -> tuple[NDArray[np.uint8], NDArray[np.uint8], int] | None:
    points, descriptors = _features(target)
    other_points, other_descriptors = _features(reference)
    if descriptors is None or other_descriptors is None:
        return None
    matches = cv2.BFMatcher().knnMatch(other_descriptors, descriptors, k=NEIGHBOURS)
    good = [
        pair[0]
        for pair in matches
        if len(pair) == NEIGHBOURS and pair[0].distance < 0.72 * pair[1].distance
    ]
    if len(good) < MIN_MATCHES:
        return None
    source = np.asarray([other_points[m.queryIdx] for m in good], np.float32)
    destination = np.asarray([points[m.trainIdx] for m in good], np.float32)
    raw_transform, raw_inliers = cv2.findHomography(source, destination, cv2.RANSAC, 3)
    transform = cast("NDArray[np.float64] | None", raw_transform)
    inliers = cast("NDArray[np.uint8] | None", raw_inliers)
    if (
        transform is None
        or inliers is None
        or int(inliers.sum()) < MIN_INLIERS
        or float(inliers.mean()) < 0.35
    ):
        return None
    area = abs(
        cv2.contourArea(
            cv2.perspectiveTransform(
                np.asarray([[[45, 55], [420, 55], [420, 390], [45, 390]]], np.float32),
                transform,
            )[0]
        )
    )
    if not 45000 < area < 240000:
        return None
    valid = np.zeros((HEIGHT, WIDTH), np.uint8)
    valid[60:475, 40:425] = 255
    overlap = cv2.bitwise_and(
        valid, cv2.warpPerspective(valid, transform, (WIDTH, HEIGHT))
    )
    return (
        cast(
            "NDArray[np.uint8]",
            cv2.warpPerspective(reference, transform, (WIDTH, HEIGHT)),
        ),
        cast("NDArray[np.uint8]", cv2.erode(overlap, np.ones((7, 7), np.uint8))),
        int(inliers.sum()),
    )


def _boxes(mask: NDArray[np.uint8], *, low_contrast: bool) -> tuple[MarkBox, ...]:
    closed = cv2.morphologyEx(mask, cv2.MORPH_CLOSE, np.ones((5, 7), np.uint8))
    _, _, stats, _ = cv2.connectedComponentsWithStats(closed)
    result = []
    for raw_x, raw_y, raw_w, raw_h, raw_area in stats[1:]:
        x, y, width, height, area = map(int, (raw_x, raw_y, raw_w, raw_h, raw_area))
        minimum_area = 12 if low_contrast else 40
        if (
            area < minimum_area
            or width < 8
            or height < 4
            or width > 250
            or height > 160
        ):
            continue
        # Anniversary logos can be tall and touch an edge; only narrow strips are frames.
        edge = x < 60 or x + width > 415
        if height > 3 * width or (edge and height > 60 and width < 35):
            continue
        result.append(MarkBox(x, y, width, height, low_contrast))
    return tuple(result)


def _correct(
    target: NDArray[np.uint8], warped: NDArray[np.uint8], valid: NDArray[np.bool_]
) -> NDArray[np.float32]:
    left = target.astype(np.float32)
    right = warped.astype(np.float32)
    for channel in range(3):
        x, y = right[:, :, channel][valid], left[:, :, channel][valid]
        keep = np.abs(x - y) < 45
        if int(keep.sum()) > 2000:
            gain, offset = np.linalg.lstsq(
                np.column_stack([x[keep], np.ones(int(keep.sum()))]),
                y[keep],
                rcond=None,
            )[0]
            right[:, :, channel] = np.clip(
                right[:, :, channel] * np.clip(gain, 0.75, 1.3)
                + np.clip(offset, -25, 25),
                0,
                255,
            )
    return right


def _difference(
    target: NDArray[np.uint8], reference: NDArray[np.uint8]
) -> tuple[tuple[MarkBox, ...], int] | None:
    aligned = _warp(target, reference)
    if aligned is None:
        return None
    warped, mask, inliers = aligned
    valid = mask > 0
    right = _correct(target, warped, valid)
    left = target.astype(np.float32)
    delta = np.max(
        np.abs(cv2.GaussianBlur(left, (3, 3), 0) - cv2.GaussianBlur(right, (3, 3), 0)),
        axis=2,
    )
    background_region = valid & (np.indices(valid.shape)[0] < 240)
    if not background_region.any():
        return None
    background = float(np.median(delta[background_region]))
    if background > MAX_BACKGROUND:
        return None
    # A plaque ornament must not become an inferred text-box boundary.
    strong = ((delta > max(34, background * 2.8 + 10)) & valid).astype(np.uint8) * 255
    fine_delta = np.max(np.abs(left - right), axis=2)
    faint = ((fine_delta > max(12, background * 2 + 6)) & valid).astype(np.uint8) * 255
    strong[:150] = 0
    faint[:150] = 0
    strong_boxes = _boxes(strong, low_contrast=False)
    faint_boxes = tuple(
        box
        for box in _boxes(faint, low_contrast=True)
        if not any(
            other.x <= box.x
            and other.y <= box.y
            and other.x + other.width >= box.x + box.width
            and other.y + other.height >= box.y + box.height
            for other in strong_boxes
        )
    )
    return (*strong_boxes, *faint_boxes), inliers


def detect_marks(target: bytes, references: tuple[bytes, ...] = ()) -> Detection:
    """Find candidate regions; neither a difference nor no difference proves a stamp.

    Callers order references with ordinary printings first. PR-to-PR comparisons
    are still candidates: both cards may share a plaque or event logo.
    """
    cv2.setNumThreads(1)
    image = bitmap(target)
    for index, raw in enumerate(references):
        result = _difference(image, bitmap(raw))
        if result is not None:
            boxes, inliers = result
            return Detection(boxes, "aligned-art-difference", index, inliers)
    hsv = cv2.cvtColor(image, cv2.COLOR_BGR2HSV)
    gold = cv2.inRange(hsv, (12, 25, 95), (42, 255, 255))
    gold[:150] = 0
    gold[475:] = 0
    gold[:, :40] = 0
    gold[:, 425:] = 0
    return Detection(
        _boxes(cast("NDArray[np.uint8]", gold), low_contrast=True),
        "gold-candidates-no-alignment",
        None,
    )
