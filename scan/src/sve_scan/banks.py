"""Build the effect banks used by both trainers.
Every effect is a per-pixel affine map x' = a*x + b (+ R for real layers).
real bank : foil texture bank (66) + deck-photo sleeve layers (583) -> ab [n,6,168,120] fp16, res [n,3,336,240] fp16
proc banks: wrinkles / ceiling lights / misc (sleeve rim, sticker, matte veil) -> ab [n,6,168,120] fp16
Inputs: the foil texture bank (texbank) and the deck-photo sleeve layers (sleeve-bank), both private-derived; outputs
stay private too. Only this code is published.
usage: python -m sve_scan banks <texbank.npz> <sleeve_material.npz> <real_out.npz> <proc_out.npz>"""

import math
import sys

import cv2
import numpy as np

cv2.setNumThreads(3)
TEXBANK, XBANK, REAL_OUT, PROC_OUT = sys.argv[1:5]
H, W = 336, 240
HH, HW = H // 2, W // 2
N_WR, N_HS, N_MISC = 1500, 1500, 500


def half(ab):  # [H?,W?,6] -> [6,HH,HW]
    return cv2.resize(ab.astype(np.float32), (HW, HH), interpolation=cv2.INTER_AREA).transpose(2, 0, 1)


# ---------------- real ----------------
tex = np.load(TEXBANK)["layers"].astype(np.float32)  # [n,H,W,9] RGB a,b,R
xz = np.load(XBANK)
rab = [half(L[..., :6]) for L in tex] + [half(cv2.resize(a.astype(np.float32), (W, H))) for a in xz["ab"]]
rres = [L[..., 6:9].transpose(2, 0, 1) for L in tex] + [r.astype(np.float32).transpose(2, 0, 1) for r in xz["res"]]
src = ["tex"] * len(tex) + ["x"] * len(xz["ab"])
np.savez(REAL_OUT, ab=np.stack(rab).astype(np.float16), res=np.stack(rres).astype(np.float16), src=np.array(src))
print("real", len(rab), flush=True)
del tex, xz, rab, rres

# ---------------- procedural ----------------
rng = np.random.default_rng(77)
U = lambda a, b: float(rng.uniform(a, b))
yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)


def noise(sigma):
    n = cv2.GaussianBlur(rng.standard_normal((H, W)).astype(np.float32), (0, 0), sigma)
    return n / (n.std() + 1e-6)


def line_dist(ang, x0, y0, wander):
    c, s = math.cos(ang), math.sin(ang)
    return (xx - x0) * -s + (yy - y0) * c + noise(U(20, 45)) * wander


def ridge(d, w, sharp):
    return np.maximum(0, 1 - np.abs(d) / w) ** 1.3 if sharp else np.exp(-((d / w) ** 2))


def h_sag():
    return noise(U(25, 50)) * U(5, 12) + noise(U(10, 16)) * U(1, 3)


def h_fold():
    h = noise(U(30, 50)) * U(1, 3)
    for _ in range(rng.integers(1, 4)):
        h += (
            ridge(line_dist(U(0, math.pi), U(0.1, 0.9) * W, U(0.1, 0.9) * H, U(0, 5)), U(1.5, 4), True)
            * U(4, 10)
            * rng.choice([-1, 1])
        )
    return h


def h_crumple():
    h = noise(U(4, 7)) * U(0.5, 1.5)
    for _ in range(rng.integers(15, 45)):
        ang = U(0, math.pi)
        x0, y0 = U(0, W), U(0, H)
        along = (xx - x0) * math.cos(ang) + (yy - y0) * math.sin(ang)
        h += (
            ridge(line_dist(ang, x0, y0, U(0, 3)), U(1.5, 5), True)
            * np.exp(-((along / U(20, 90)) ** 2))
            * U(1.5, 5)
            * rng.choice([-1, 1])
        )
    return h


def h_bubble():
    h = noise(U(30, 50)) * U(0.5, 2)
    for _ in range(rng.integers(1, 4)):
        edge = rng.random() < 0.7
        cx = (rng.choice([U(0, 0.2), U(0.8, 1)]) if edge else U(0.2, 0.8)) * W
        cy = (rng.choice([U(0, 0.2), U(0.8, 1)]) if edge and rng.random() < 0.5 else U(0, 1)) * H
        r = U(15, 70)
        h += np.exp(-(((xx - cx) ** 2 + (yy - cy) ** 2) / r**2)) * U(5, 14)
    return h


def h_ripple():
    ang = U(0, math.pi)
    freq = U(4, 14) / max(W, H)
    t = (xx * math.cos(ang) + yy * math.sin(ang)) + noise(U(25, 45)) * U(2, 10)
    amp = 0.5 + 0.5 * np.clip(noise(U(35, 60)) * 0.5 + 0.5, 0, 1)
    return np.sin(t * freq * 2 * math.pi) * amp * U(2, 6) + noise(U(30, 50)) * U(1, 3)


def h_edge():
    d = [xx, W - 1 - xx, yy, H - 1 - yy][rng.integers(4)]
    reach = U(15, 60)
    return np.exp(-d / reach) * U(8, 18) + noise(U(15, 30)) * U(1, 3) * np.exp(-d / (2 * reach))


WR = [h_sag, h_fold, h_crumple, h_bubble, h_ripple, h_edge]
WR_W = np.array([1.2, 1, 1, 0.8, 0.4, 0.8])
WR_W /= WR_W.sum()  # ripple is rarer in real photos


def wrinkle_ab():
    h = WR[rng.choice(len(WR), p=WR_W)]()
    if rng.random() < 0.4:
        h = h + WR[rng.choice(len(WR), p=WR_W)]() * U(0.3, 0.8)
    gx = cv2.Sobel(h, cv2.CV_32F, 1, 0, ksize=3) / 8
    gy = cv2.Sobel(h, cv2.CV_32F, 0, 1, ksize=3) / 8
    k = U(2, 6)
    n = np.stack([-gx * k, -gy * k, np.ones_like(h)], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    spec = np.zeros((H, W), np.float32)
    for _ in range(rng.integers(1, 4)):
        L = np.array([U(-0.6, 0.6), U(-0.7, 0.3), 1.0])
        L /= np.linalg.norm(L)
        hv = L + np.array([0, 0, 1.0])
        hv /= np.linalg.norm(hv)
        nh = np.clip(n @ hv, 0, 1)
        spec = np.maximum(spec, nh ** U(20, 220) * U(0.5, 1) + nh ** U(3, 18) * U(0.05, 0.55))
    ang = U(0, 2 * math.pi)
    fall = U(0.3, 0.7) + U(0.3, 0.7) * np.clip(
        ((xx / W - 0.5) * math.cos(ang) + (yy / H - 0.5) * math.sin(ang)) * 1.6 + 0.5, 0, 1
    )
    s = np.clip(spec * fall * U(0.5, 1.1), 0, 1)[..., None]
    wash, c, kc = U(0.03, 0.55), U(0.75, 0.95), U(0.7, 1)
    tint = (
        np.array([1, U(0.95, 1), U(0.9, 1)], np.float32)
        if rng.random() < 0.5
        else np.array([U(0.9, 1), U(0.96, 1), 1], np.float32)
    )
    # base = (x*(1-wash) + wash*c - .5)*kc + .5 ; out = base*(1-.85s) + s*tint
    a = np.broadcast_to((1 - wash) * kc * (1 - 0.85 * s), (H, W, 3))
    b = ((wash * c - 0.5) * kc + 0.5) * (1 - 0.85 * s) + s * tint
    return np.concatenate([a, b], 2)


def rrect(cx, cy, hw, hh, ang, r, soft):
    c, s = math.cos(ang), math.sin(ang)
    px = (xx - cx) * c + (yy - cy) * s
    py = -(xx - cx) * s + (yy - cy) * c
    dx = np.maximum(np.abs(px) - hw + r, 0)
    dy = np.maximum(np.abs(py) - hh + r, 0)
    return np.clip(0.5 - (np.hypot(dx, dy) - r) / max(soft, 0.5), 0, 1)


def m_dot():
    m = np.zeros((H, W), np.float32)
    for _ in range(rng.integers(1, 6)):
        r = U(2, 18)
        m = np.maximum(m, rrect(U(0, W), U(0, H), r, r, 0, r, U(0.5, 6)))
    return m


def m_glow():
    r = U(30, 160)
    return rrect(U(0, W), U(0, H), r * U(0.5, 1.5), r * U(0.5, 1.5), U(0, math.pi), r * 0.5, r * U(0.4, 1.2))


def m_tube():
    return rrect(U(0, W), U(0, H), U(0.3, 1.2) * H, U(2, 14), U(0, math.pi), U(1, 6), U(0.5, 10))


def m_tubes():
    ang = U(0, math.pi)
    gap = U(18, 70)
    n = rng.integers(2, 5)
    cx, cy = U(0.2, 0.8) * W, U(0.2, 0.8) * H
    hl, hw, soft = U(0.3, 1) * H, U(2, 10), U(0.5, 8)
    m = np.zeros((H, W), np.float32)
    for i in range(n):
        o = (i - (n - 1) / 2) * gap
        m = np.maximum(m, rrect(cx - o * math.sin(ang), cy + o * math.cos(ang), hl, hw, ang, 2, soft))
    return m


def m_panel():
    k = rng.integers(2, 4)
    cell = U(12, 45)
    gap = U(2, 8)
    cx, cy = U(0, W), U(0, H)
    ang = U(-0.4, 0.4)
    soft = U(0.5, 6)
    m = np.zeros((H, W), np.float32)
    c, s = math.cos(ang), math.sin(ang)
    for i in range(k):
        for j in range(k):
            ox, oy = (i - (k - 1) / 2) * (cell + gap), (j - (k - 1) / 2) * (cell + gap)
            m = np.maximum(m, rrect(cx + ox * c - oy * s, cy + ox * s + oy * c, cell / 2, cell / 2, ang, 2, soft))
    return m


def m_window():
    hw, hh = U(30, 120), U(30, 140)
    cx, cy = U(0, W), U(0, H)
    ang = U(-0.3, 0.3)
    m = rrect(cx, cy, hw, hh, ang, 3, U(1, 25))
    bar = U(2, 7)
    return m * (1 - rrect(cx, cy, bar, hh * 1.1, ang, 0, 1)) * (1 - rrect(cx, cy, hw * 1.1, bar, ang, 0, 1))


def m_ring():
    cx, cy, r = U(0, W), U(0, H), U(15, 90)
    w = U(2, 12)
    return np.clip(0.5 - (np.abs(np.hypot(xx - cx, yy - cy) - r) - w) / U(0.5, 6), 0, 1)


HS = [m_dot, m_glow, m_tube, m_tubes, m_panel, m_window, m_ring]
HS_KIND = ["dot", "glow", "tube", "tubes", "panel", "window", "ring"]


def light_ab():
    A = np.zeros((H, W, 3), np.float32)
    for _ in range(2 if rng.random() < 0.3 else 1):
        ki = rng.integers(len(HS))
        m = HS[ki]()
        if rng.random() < 0.7:
            amp = U(1, 6) if HS_KIND[ki] in ("tube", "tubes", "ring", "window") else U(1, 12)
            sg = U(25, 60)
            m = cv2.remap(
                m, xx + noise(sg) * amp, yy + noise(sg) * amp, cv2.INTER_LINEAR, borderMode=cv2.BORDER_CONSTANT
            )
        peak = float(np.clip(rng.beta(2.2, 1.6), 0.2, 1))
        m = cv2.GaussianBlur(m, (0, 0), U(0.6, 2.5))
        halo = cv2.GaussianBlur(m, (0, 0), U(6, 40))
        halo = halo / (halo.max() + 1e-6) * U(0, 0.5) * peak
        tint = [np.array([1, 0.97, 0.9]), np.array([0.92, 0.97, 1]), np.array([1, 1, 1])][rng.integers(3)].astype(
            np.float32
        )
        a1 = np.clip(m * peak + halo, 0, 1)[..., None] * tint
        A = 1 - (1 - A) * (1 - a1)  # screen-combine the lights
    return np.concatenate([1 - A, A], 2)  # screen blend: x' = x*(1-A) + A


def misc_ab():
    a = np.ones((H, W, 3), np.float32)
    b = np.zeros((H, W, 3), np.float32)

    def comp(a2, b2):
        nonlocal a, b
        a, b = a2 * a, a2 * b + b2

    r = rng.random()
    if r < 0.45 or rng.random() < 0.3:  # sleeve rim (single or double)
        for inset in [U(2, 6)] if rng.random() < 0.5 else [U(2, 5), U(7, 12)]:
            d = np.minimum.reduce([xx, W - 1 - xx, yy, H - 1 - yy])
            along = cv2.GaussianBlur(rng.random((H, W)).astype(np.float32), (0, 0), 25)
            along = (along - along.min()) / (np.ptp(along) + 1e-6)
            m = (np.exp(-(((d - inset) / U(0.6, 1.4)) ** 2)) * (0.4 + 0.6 * along) * U(0.3, 0.9))[..., None]
            comp(np.broadcast_to(1 - m, (H, W, 3)), np.broadcast_to(m, (H, W, 3)))
    if 0.45 <= r < 0.75 or rng.random() < 0.15:  # sticker / note
        sw, sh = int(U(0.2, 0.55) * W), int(U(0.07, 0.2) * H)
        col = [
            np.array(c, np.float32)
            for c in ([1, 0.9, 0.2], [1, 0.75, 0.8], [0.98, 0.98, 0.95], [0.6, 0.9, 1], [1, 0.6, 0.3])
        ][rng.integers(5)]
        patch = np.ones((sh, sw, 3), np.float32) * col
        for _ in range(rng.integers(2, 6)):
            y = int(U(0.2, 0.8) * sh)
            x0 = int(U(0.05, 0.3) * sw)
            x1 = int(U(0.5, 0.95) * sw)
            pts = np.int32([[xi, y + U(-3, 3)] for xi in range(x0, x1, 4)]).reshape(-1, 1, 2)
            cv2.polylines(patch, [pts], False, (0.1, 0.1, 0.15), int(U(1, 3)), cv2.LINE_AA)
        M = cv2.getRotationMatrix2D((sw / 2, sh / 2), U(-20, 20), 1)
        M[:, 2] += [U(-sw * 0.3, W - sw * 0.7), U(-sh * 0.3, H - sh * 0.7)]
        pw = cv2.warpAffine(patch, M, (W, H))
        mk = cv2.warpAffine(np.ones((sh, sw), np.float32), M, (W, H))[..., None]
        comp(np.broadcast_to(1 - mk, (H, W, 3)), pw * mk)
    if r >= 0.75:  # matte sleeve veil
        v = U(0.1, 0.3)
        c = U(0.75, 0.95)
        comp(np.full((H, W, 3), 1 - v, np.float32), np.full((H, W, 3), v * c, np.float32))
    return np.concatenate([a, b], 2)


out = {}
for name, fn, n in (("wr", wrinkle_ab, N_WR), ("hs", light_ab, N_HS), ("misc", misc_ab, N_MISC)):
    out[name] = np.stack([half(fn()) for _ in range(n)]).astype(np.float16)
    print(name, out[name].shape, flush=True)
np.savez(PROC_OUT, **out)
print("done")
