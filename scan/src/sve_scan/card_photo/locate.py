"""Find a card lying on a table in a photo and straighten it. Works for every card layout.

A card with a pale outer border is found among sharp edges. A full-art card (no border) whose edges
melt into busy art falls back to removing the table colour, so the photo needs a plain wooden or
similarly coloured table around the card.
"""

import cv2
import numpy as np

CARD_RATIO = 88 / 63  # height / width of a standard card


def _order(q: np.ndarray) -> np.ndarray:
    s, d = q.sum(1), np.diff(q, axis=1).ravel()
    return np.array([q[np.argmin(s)], q[np.argmin(d)], q[np.argmax(s)], q[np.argmax(d)]], np.float32)


def _card_shaped(q: np.ndarray, area: float, frame_area: float) -> bool:
    short, long = sorted(cv2.minAreaRect(q)[1])
    return area > 0.05 * frame_area and 1.25 < long / max(short, 1) < 1.55


def _squareness(q: np.ndarray) -> float:
    """Largest |cos| of the four corner angles; 0 is a perfect rectangle."""
    return max(
        abs(np.dot(q[i - 1] - q[i], q[(i + 1) % 4] - q[i]))
        / (np.linalg.norm(q[i - 1] - q[i]) * np.linalg.norm(q[(i + 1) % 4] - q[i]))
        for i in range(4)
    )


def _guess_from_edges(img: np.ndarray) -> np.ndarray | None:
    # colour alone mistakes table glare for a white border; edges do not
    h, w = img.shape[:2]
    gray = cv2.GaussianBlur(cv2.cvtColor(img, cv2.COLOR_BGR2GRAY), (5, 5), 0)
    edges = cv2.dilate(cv2.Canny(gray, 30, 90), np.ones((5, 5), np.uint8))
    contours, _ = cv2.findContours(edges, cv2.RETR_LIST, cv2.CHAIN_APPROX_SIMPLE)
    candidates = []
    for c in contours:
        approx = cv2.approxPolyDP(c, 0.02 * cv2.arcLength(c, True), True)
        if len(approx) != 4 or not cv2.isContourConvex(approx):
            continue
        q = approx.reshape(4, 2).astype(np.float32)
        area = cv2.contourArea(approx)
        if _card_shaped(q, area, h * w):
            candidates.append((area, _squareness(q), q))
    if not candidates:
        return None
    # the card and "card plus its shadow or glare" are both candidates; the card has the squarest corners
    largest = max(c[0] for c in candidates)
    return _order(min((c for c in candidates if c[0] >= 0.85 * largest), key=lambda c: c[1])[2])


def _guess_from_table(img: np.ndarray) -> np.ndarray | None:
    h, w = img.shape[:2]
    hsv = cv2.cvtColor(cv2.GaussianBlur(img, (9, 9), 0), cv2.COLOR_BGR2HSV)
    table = (hsv[..., 0] >= 4) & (hsv[..., 0] <= 24) & (hsv[..., 1] >= 70) & (hsv[..., 2] >= 70)
    card = np.where(table, 0, 255).astype(np.uint8)
    card = cv2.morphologyEx(card, cv2.MORPH_OPEN, np.ones((15, 15), np.uint8))
    card = cv2.morphologyEx(card, cv2.MORPH_CLOSE, np.ones((41, 41), np.uint8))
    contours, _ = cv2.findContours(card, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
    best = None
    for c in contours:
        x, y, cw, ch = cv2.boundingRect(c)
        if x <= 2 or y <= 2 or x + cw >= w - 2 or y + ch >= h - 2:
            continue  # touches the frame: books, paper or the edge of the table
        box = cv2.boxPoints(cv2.minAreaRect(c)).astype(np.float32)
        if _card_shaped(box, cv2.contourArea(c), h * w) and (
            best is None or cv2.contourArea(c) > cv2.contourArea(best)
        ):
            best = c
    if best is None:
        return None
    hull = cv2.convexHull(best)
    for eps in np.linspace(0.005, 0.08, 60):
        approx = cv2.approxPolyDP(hull, eps * cv2.arcLength(hull, True), True)
        if len(approx) == 4:
            return _order(approx.reshape(4, 2).astype(np.float32))
    return _order(cv2.boxPoints(cv2.minAreaRect(best)).astype(np.float32))


def _intersect(l1: np.ndarray, l2: np.ndarray) -> np.ndarray:
    (vx1, vy1, x1, y1), (vx2, vy2, x2, y2) = l1, l2
    t = np.linalg.solve(np.array([[vx1, -vx2], [vy1, -vy2]]), [x2 - x1, y2 - y1])
    return np.array([x1 + vx1 * t[0], y1 + vy1 * t[0]], np.float32)


def _refine(img: np.ndarray, guess: np.ndarray, full_art: bool) -> np.ndarray:
    """Fit each side as a straight line through the card-to-table transition.

    A bordered card is pale just inside its edge and the table is more saturated outside; only that
    direction counts, or the border's inner edge (pale to coloured art) would match as well.
    """
    h, w = img.shape[:2]
    gray = cv2.GaussianBlur(cv2.cvtColor(img, cv2.COLOR_BGR2GRAY), (5, 5), 0)
    ys, xs = np.nonzero(cv2.Canny(gray, 30, 90))
    pts = np.stack([xs, ys], 1).astype(np.float32)
    sat = cv2.cvtColor(img, cv2.COLOR_BGR2HSV)[..., 1].astype(np.float32)
    centre = guess.mean(0)

    def contrast(outside: float, inside: float) -> float:
        return abs(outside - inside) if full_art else outside - inside

    lines = []
    for i in range(4):
        a, b = guess[i], guess[(i + 1) % 4]
        length = float(np.linalg.norm(b - a))
        u = (b - a) / length
        n = np.array([-u[1], u[0]], np.float32)
        if (centre - a) @ n > 0:
            n = -n  # n points out of the card
        t, offset = (pts - a) @ u, (pts - a) @ n
        near = (t > 0.12 * length) & (t < 0.88 * length) & (np.abs(offset) < 0.05 * length)
        # the card edge is a long straight line: a tall peak among edge-pixel offsets from the guess
        bins = np.arange(-0.05 * length, 0.05 * length + 2, 2)
        hist, _ = np.histogram(offset[near], bins)
        strong = np.nonzero(hist >= 0.095 * length)[0]  # a quarter of the side, in 2px bins
        shift = 0.0
        if len(strong):
            samples = a + u * (np.linspace(0.15, 0.85, 60)[:, None] * length)

            def sat_at(o: float, samples: np.ndarray = samples, n: np.ndarray = n) -> float:
                p = np.round(samples + n * o).astype(int)
                return float(np.median(sat[p[:, 1].clip(0, h - 1), p[:, 0].clip(0, w - 1)]))

            shift = max(
                ((bins[k] + bins[k + 1]) / 2 for k in strong), key=lambda o: contrast(sat_at(o + 6), sat_at(o - 6))
            )
        # sub-pixel pass: walk outward and stop where saturation crosses halfway to the table's
        start, steps, found = a + n * shift, np.arange(-0.02 * length, 0.02 * length, 0.5), []
        for tt in np.linspace(0.1, 0.9, 120) * length:
            xy = (start + u * tt)[None, :] + n[None, :] * steps[:, None]
            prof = cv2.remap(sat, xy[:, :1].astype(np.float32), xy[:, 1:].astype(np.float32), cv2.INTER_LINEAR).ravel()
            prof = np.convolve(prof, np.ones(5) / 5, mode="same")
            inside, outside = float(np.median(prof[5:25])), float(np.median(prof[-25:-5]))
            if contrast(outside, inside) < 25:
                continue  # no clear card/table contrast here (glare, shadow)
            half = (inside + outside) / 2
            crossed = prof[5:] > half if outside > inside else prof[5:] < half
            k = int(np.argmax(crossed)) + 5
            if not crossed[k - 5]:
                continue
            f = (half - prof[k - 1]) / (prof[k] - prof[k - 1])
            found.append(start + u * tt + n * (steps[k - 1] + f * 0.5))
        if len(found) >= 20:
            on_edge = np.array(found, np.float32)
        else:
            on_edge = pts[near & (np.abs(offset - shift) < 3)]
            if len(on_edge) < 20:
                on_edge = np.array([a + u * length * s for s in np.linspace(0.1, 0.9, 20)], np.float32)
        lines.append(cv2.fitLine(on_edge, cv2.DIST_HUBER, 0, 0.01, 0.01).ravel())
    return np.array([_intersect(lines[(i - 1) % 4], lines[i]) for i in range(4)], np.float32)


def find_card(img: np.ndarray) -> np.ndarray:
    """Corners of the card, TL, TR, BR, BL in image space."""
    guess = _guess_from_edges(img)
    if guess is not None:
        return _refine(img, guess, full_art=False)
    guess = _guess_from_table(img)
    if guess is None:
        raise ValueError("card not found: the whole card must be in the photo with some table around it")
    return _refine(img, guess, full_art=True)


def warp(img: np.ndarray, corners: np.ndarray, width: int) -> np.ndarray:
    """The card straightened to `width` pixels wide at 63:88; corners run clockwise from the card's top left."""
    height = int(round(width * CARD_RATIO))
    target = np.array([[0, 0], [width - 1, 0], [width - 1, height - 1], [0, height - 1]], np.float32)
    return cv2.warpPerspective(
        img, cv2.getPerspectiveTransform(corners, target), (width, height), flags=cv2.INTER_CUBIC
    )


def stand_upright(img: np.ndarray, corners: np.ndarray, extra_turns: int = 0) -> np.ndarray:
    """Corners reordered to start at the card's top left. The darker end of the card (its name bar) is
    taken as the bottom; `extra_turns` turns the result further clockwise when that guess is wrong."""
    tl, tr, br, bl = corners
    if np.linalg.norm(tr - tl) > np.linalg.norm(bl - tl):  # card lies sideways
        corners = np.array([bl, tl, tr, br], np.float32)
    probe = cv2.cvtColor(warp(img, corners, 300), cv2.COLOR_BGR2GRAY).astype(float)
    h = probe.shape[0]
    if probe[int(h * 0.03) : int(h * 0.2)].mean() < probe[int(h * 0.8) : int(h * 0.97)].mean():
        corners = np.array([corners[2], corners[3], corners[0], corners[1]], np.float32)
    return np.roll(corners, extra_turns % 4, axis=0)


def photo_turns(corners: np.ndarray) -> int:
    """Quarter turns counter-clockwise that bring the whole photo upright with the card."""
    top = (corners[0] + corners[1]) / 2 - corners.mean(0)
    return int(round(np.degrees(np.arctan2(top[0], -top[1])) / 90)) % 4
