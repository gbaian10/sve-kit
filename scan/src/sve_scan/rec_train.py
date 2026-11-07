"""Recogniser training: a phone-sized embedder on official images with GPU augmentation.

One proxy per printing face (CosFace), soft target shared with other printings of the same art.
~8% of cards are held out of training (still in the gallery) to check that new packs work without retraining.
Recipe knobs are environment variables (the two stages of the current model are in scan/README.md).

usage: python -m sve_scan rec-train <arch> <epochs> [tag]
"""

import json
import math
import sys
import time

import cv2
import kornia
import numpy as np
import timm
import torch
import torch.nn as nn
import torch.nn.functional as F
from kornia.enhance import jpeg_codec_differentiable
from kornia.filters import gaussian_blur2d

from sve_scan import paths

OUT = paths.work()
DEV = "cuda"
H, W = 336, 240
IN_H, IN_W = 312, 224
MEAN = torch.tensor((0.485, 0.456, 0.406), device=DEV).view(1, 3, 1, 1)
STD = torch.tensor((0.229, 0.224, 0.225), device=DEV).view(1, 3, 1, 1)
COND = ("rect_mild", "rect_hard", "loose_crop")


def log(*a: object) -> None:
    print(time.strftime("%H:%M:%S"), *a, flush=True)


# ---------------- data ----------------
meta = json.loads((OUT / "gallery.json").read_text())
N = len(meta)
card_of = np.array([m["card_id"] for m in meta])
rng = np.random.default_rng(1)
cards = np.unique(card_of)
held_cards = set(rng.choice(cards, int(len(cards) * 0.08), replace=False))
held = np.array([c in held_cards for c in card_of])
train_idx = np.where(~held)[0]
qidx = np.array(json.loads((OUT / "queries.json").read_text()))

# soft targets: 0.8 on the printing, 0.2 spread over other training printings of the same card
cls_of = -np.ones(N, dtype=np.int64)
cls_of[train_idx] = np.arange(len(train_idx))
import os  # noqa: E402

# SIBLINGS=art → share the soft target only with printings of the same art (keeps alt arts apart)
sib_key = card_of
if os.environ.get("SIBLINGS") == "art":
    sib_key = np.array([str(m["art_id"] or m["card_id"]) for m in meta])
same_card: dict[str, list[int]] = {}
for gi in train_idx:
    same_card.setdefault(sib_key[gi], []).append(int(cls_of[gi]))


# ---------------- augmentation (GPU) ----------------
def glare(x: torch.Tensor, p: float, max_a: float) -> torch.Tensor:
    b = x.shape[0]
    yy, xx = torch.meshgrid(torch.linspace(0, 1, H, device=DEV), torch.linspace(0, 1, W, device=DEV), indexing="ij")
    cx, cy = torch.rand(b, 1, 1, device=DEV), torch.rand(b, 1, 1, device=DEV)
    ax, ay = 0.1 + 0.4 * torch.rand(b, 1, 1, device=DEV), 0.05 + 0.3 * torch.rand(b, 1, 1, device=DEV)
    ell = torch.exp(-2 * (((xx - cx) / ax) ** 2 + ((yy - cy) / ay) ** 2))
    ang = math.pi * torch.rand(b, 1, 1, device=DEV)
    off = torch.rand(b, 1, 1, device=DEV) - 0.5
    wid = 0.03 + 0.12 * torch.rand(b, 1, 1, device=DEV)
    streak = torch.exp(-((((xx - 0.5) * torch.cos(ang) + (yy - 0.5) * torch.sin(ang) - off) / wid) ** 2))
    use_ell = torch.rand(b, 1, 1, device=DEV) < 0.5
    m = torch.where(use_ell, ell, streak)
    a = (torch.rand(b, 1, 1, device=DEV) * max_a) * (torch.rand(b, 1, 1, device=DEV) < p)
    m = (m * a).unsqueeze(1)
    return x * (1 - m) + m


def frame_scene(x: torch.Tensor) -> torch.Tensor:
    """What a roughly aligned aim frame sees: card at 70-100% of the frame, tilted, on a mat, neighbours at the
    edges."""
    b = x.shape[0]
    wh = torch.tensor([W, H], device=DEV, dtype=torch.float32)
    src = torch.tensor([[0, 0], [W - 1, 0], [W - 1, H - 1], [0, H - 1]], device=DEV, dtype=torch.float32).expand(
        b, 4, 2
    )
    # background: blurred, darkened other card (playmat art) or a flat noisy colour
    other = x.roll(3, 0)
    mat = gaussian_blur2d(
        F.interpolate(other[:, :, : H // 3, : W // 3], size=(H, W), mode="bilinear"), (15, 15), (6.0, 6.0)
    ) * (0.3 + 0.5 * torch.rand(b, 1, 1, 1, device=DEV))
    flat = torch.rand(b, 3, 1, 1, device=DEV) * 0.8 + torch.randn(b, 3, H, W, device=DEV) * 0.03
    canvas = torch.where(torch.rand(b, 1, 1, 1, device=DEV) < 0.5, mat, flat)

    def quad(shift: torch.Tensor) -> torch.Tensor:
        s = FRAME_SMIN + (1 - FRAME_SMIN) * torch.rand(b, 1, 1, device=DEV)
        top = 1 - 0.25 * torch.rand(b, 1, 1, device=DEV)  # far edge narrower
        base = torch.tensor([[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]], device=DEV).expand(b, 4, 2).clone()
        base[:, :2, 0] = base[:, :2, 0] * top[:, :, 0]
        ang = (torch.rand(b, 1, 1, device=DEV) * 2 - 1) * math.radians(8)
        c, si = torch.cos(ang), torch.sin(ang)
        px = base[..., 0:1] * wh[0] * s
        py = base[..., 1:2] * wh[1] * s
        rx = px * c - py * si
        ry = px * si + py * c
        off = (torch.rand(b, 1, 2, device=DEV) * 2 - 1) * (1 - s) / 2 * wh
        return torch.cat([rx, ry], -1) + wh / 2 + off + shift

    ones = torch.ones(b, 1, H, W, device=DEV)
    # neighbours peeking in from a side
    side = torch.where(torch.rand(b, 1, 1, device=DEV) < 0.5, -1.0, 1.0)
    nshift = torch.cat(
        [
            side * W * (0.95 + 0.1 * torch.rand(b, 1, 1, device=DEV)),
            (torch.rand(b, 1, 1, device=DEV) * 2 - 1) * H * 0.1,
        ],
        -1,
    )
    mn = kornia.geometry.transform.get_perspective_transform(src, quad(nshift))
    nb = kornia.geometry.transform.warp_perspective(x.roll(1, 0), mn, (H, W))
    nm = kornia.geometry.transform.warp_perspective(ones, mn, (H, W)) * (torch.rand(b, 1, 1, 1, device=DEV) < 0.5)
    canvas = canvas * (1 - nm) + nb * nm
    mt = kornia.geometry.transform.get_perspective_transform(src, quad(torch.zeros(b, 1, 2, device=DEV)))
    card = kornia.geometry.transform.warp_perspective(x, mt, (H, W))
    cm = kornia.geometry.transform.warp_perspective(ones, mt, (H, W))
    return canvas * (1 - cm) + card * cm


FRAME_P = float(os.environ.get("FRAME_P", "0"))
FRAME_SMIN = float(os.environ.get("FRAME_SMIN", "0.7"))
HOLO_P = float(os.environ.get("HOLO_P", "0"))
DEFOCUS_P = float(os.environ.get("DEFOCUS_P", "0"))


def holo(x: torch.Tensor) -> torch.Tensor:
    """Rainbow foil sheen: diagonal hue bands at low alpha over part of the card."""
    b = x.shape[0]
    yy, xx = torch.meshgrid(torch.linspace(0, 1, H, device=DEV), torch.linspace(0, 1, W, device=DEV), indexing="ij")
    ang = math.pi * torch.rand(b, 1, 1, device=DEV)
    freq = 4 + 14 * torch.rand(b, 1, 1, device=DEV)
    ph = (xx * torch.cos(ang) + yy * torch.sin(ang)) * freq * 2 * math.pi + 2 * math.pi * torch.rand(
        b, 1, 1, device=DEV
    )
    rgb = torch.stack([0.5 + 0.5 * torch.sin(ph + k * 2.094) for k in range(3)], 1)
    cx, cy = torch.rand(b, 1, 1, device=DEV), torch.rand(b, 1, 1, device=DEV)
    m = torch.exp(-(((xx - cx) / 0.6) ** 2 + ((yy - cy) / 0.6) ** 2)).unsqueeze(1)
    a = (0.1 + 0.3 * torch.rand(b, 1, 1, 1, device=DEV)) * (torch.rand(b, 1, 1, 1, device=DEV) < HOLO_P) * m
    return x * (1 - a) + rgb * a


FOIL_P = float(os.environ.get("FOIL_P", "0"))
REAL_P = float(os.environ.get("REAL_P", "0"))
TEXBANK = os.environ.get("TEXBANK", "")
_bank = None
if REAL_P > 0 and TEXBANK:
    _bank = torch.from_numpy(np.load(TEXBANK)["layers"].astype(np.float32)).permute(0, 3, 1, 2).to(DEV)  # [n, 9, H, W]


def real_effect(x: torch.Tensor) -> torch.Tensor:
    """Apply effect layers measured on real foil photos: x' = a*x + b + R (gain/offset/residual), random strength."""
    b = x.shape[0]
    use = (torch.rand(b, 1, 1, 1, device=DEV) < REAL_P).float()
    k = torch.randint(len(_bank), (b,), device=DEV)
    layer = _bank[k]
    flip = torch.rand(b, device=DEV) < 0.5
    layer = torch.where(flip[:, None, None, None], layer.flip(-1), layer)
    rot = torch.rand(b, device=DEV) < 0.5
    layer = torch.where(rot[:, None, None, None], layer.flip(-1, -2), layer)
    lam = (0.5 + 0.5 * torch.rand(b, 1, 1, 1, device=DEV)) * use
    a = 1 + lam * (layer[:, 0:3] - 1)
    off = lam * layer[:, 3:6]
    res = lam * layer[:, 6:9]
    return (a * x + off + res).clamp(0, 1)


# second stage: a larger real bank (foil textures + deck-photo sleeve layers) and procedural sleeve banks,
# all kept in host memory (the GPU is shared with other services); each step gathers only the layers it uses.
REALBANK = os.environ.get("REALBANK", "")
PROCBANK = os.environ.get("PROCBANK", "")
PROC_P = {k: float(os.environ.get(f"{k.upper()}_P", "0")) for k in ("wr", "hs", "misc")}
# own probability, so adding the sleeve bank does not dilute the first-stage foil bank (TEXBANK/REAL_P)
REAL2_P = float(os.environ.get("REAL2_P", str(REAL_P)))
_rb = None
if REALBANK and REAL2_P > 0:
    _z = np.load(REALBANK)
    _rb = (torch.from_numpy(_z["ab"]), torch.from_numpy(_z["res"]))  # [n,6,H/2,W/2], [n,3,H,W] fp16
_pb: dict[str, torch.Tensor] = {}
if PROCBANK:
    _z = np.load(PROCBANK)
    _pb = {k: torch.from_numpy(_z[k]) for k in PROC_P if PROC_P[k] > 0}


def _theta(b: int, smin: float) -> torch.Tensor:
    """Random crop-zoom, mirror and 180-degree turn, as an affine_grid matrix (one per sample)."""
    s = smin + (1 - smin) * torch.rand(b, device=DEV)
    fx = torch.where(torch.rand(b, device=DEV) < 0.5, -1.0, 1.0)
    fy = torch.where(torch.rand(b, device=DEV) < 0.5, -fx, fx)  # fy == fx -> mirror or 180 turn
    tx, ty = ((torch.rand(b, device=DEV) * 2 - 1) * (1 - s), (torch.rand(b, device=DEV) * 2 - 1) * (1 - s))
    th = torch.zeros(b, 2, 3, device=DEV)
    th[:, 0, 0], th[:, 1, 1], th[:, 0, 2], th[:, 1, 2] = s * fx, s * fy, tx, ty
    return th


def _sample(layer: torch.Tensor, th: torch.Tensor) -> torch.Tensor:
    grid = F.affine_grid(th, [layer.shape[0], layer.shape[1], H, W], align_corners=False)
    return F.grid_sample(layer, grid, mode="bilinear", padding_mode="border", align_corners=False)


def real2_effect(x: torch.Tensor) -> torch.Tensor:
    b = x.shape[0]
    use = (torch.rand(b, 1, 1, 1, device=DEV) < REAL2_P).float()
    ab_bank, res_bank = _rb

    def pick() -> tuple[torch.Tensor, torch.Tensor]:
        k = torch.randint(len(ab_bank), (b,))
        th = _theta(b, 0.55)
        return (
            _sample(ab_bank[k].to(DEV, non_blocking=True).float(), th),
            _sample(res_bank[k].to(DEV, non_blocking=True).float(), th),
        )

    ab, res = pick()
    mix = torch.rand(b, 1, 1, 1, device=DEV) < 0.5  # half the time blend two photos' lighting
    ab2, res2 = pick()
    w = torch.where(mix, 0.3 + 0.4 * torch.rand(b, 1, 1, 1, device=DEV), torch.ones(b, 1, 1, 1, device=DEV))
    ab, res = w * ab + (1 - w) * ab2, w * res + (1 - w) * res2
    lam = (0.5 + 0.6 * torch.rand(b, 1, 1, 1, device=DEV)) * use
    return ((1 + lam * (ab[:, 0:3] - 1)) * x + lam * ab[:, 3:6] + lam * res).clamp(0, 1)


def proc_effect(x: torch.Tensor, bank: torch.Tensor, p: float) -> torch.Tensor:
    b = x.shape[0]
    use = (torch.rand(b, 1, 1, 1, device=DEV) < p).float()
    ab = _sample(bank[torch.randint(len(bank), (b,))].to(DEV, non_blocking=True).float(), _theta(b, 0.7))
    lam = (0.6 + 0.4 * torch.rand(b, 1, 1, 1, device=DEV)) * use
    return ((1 + lam * (ab[:, 0:3] - 1)) * x + lam * ab[:, 3:6]).clamp(0, 1)


def foil(x: torch.Tensor) -> torch.Tensor:
    """Full-card foil (evolve cards, SL): smooth rainbow interference over the whole card plus sparkle grain."""
    b = x.shape[0]
    use = (torch.rand(b, 1, 1, 1, device=DEV) < FOIL_P).float()
    yy, xx = torch.meshgrid(torch.linspace(0, 1, H, device=DEV), torch.linspace(0, 1, W, device=DEV), indexing="ij")
    ang = math.pi * torch.rand(b, 1, 1, device=DEV)
    freq = 1.5 + 5 * torch.rand(b, 1, 1, device=DEV)
    warp = 0.15 * torch.sin(yy * 7 + 6.28 * torch.rand(b, 1, 1, device=DEV)) * torch.cos(xx * 5)
    ph = ((xx * torch.cos(ang) + yy * torch.sin(ang)) * freq + warp) * 2 * math.pi
    rgb = torch.stack([0.5 + 0.5 * torch.sin(ph + k * 2.094) for k in range(3)], 1)
    a1 = (0.1 + 0.3 * torch.rand(b, 1, 1, 1, device=DEV)) * use
    x = x * (1 - a1) + (x * 0.5 + rgb * 0.5) * a1  # tint, keeps the art visible
    grain = gaussian_blur2d(torch.randn(b, 1, H, W, device=DEV), (3, 3), (0.6, 0.6))
    spark = (grain > (1.6 + 0.6 * torch.rand(b, 1, 1, 1, device=DEV))).float() * rgb
    a2 = (0.2 + 0.5 * torch.rand(b, 1, 1, 1, device=DEV)) * use
    x = x + spark * a2
    flat = 1 - (0.1 + 0.25 * torch.rand(b, 1, 1, 1, device=DEV)) * use  # foil washes out contrast
    x = (x - 0.5) * flat + 0.5 + 0.05 * use
    return x.clamp(0, 1)


def augment(x: torch.Tensor) -> torch.Tensor:
    """x float [B,3,H,W] in 0..1, clean official images."""
    b = x.shape[0]
    # card surface effects first, so the table/background around the card stays plain
    if HOLO_P > 0:
        x = holo(x)
    if FOIL_P > 0:
        x = foil(x)
    if _bank is not None:
        x = real_effect(x)
    if _rb is not None:
        x = real2_effect(x)
    for k in ("wr", "hs", "misc"):  # creases first, lamps over them, stickers/rims on top
        if k in _pb:
            x = proc_effect(x, _pb[k], PROC_P[k])
    if FRAME_P > 0:
        use = torch.rand(b, 1, 1, 1, device=DEV) < FRAME_P
        x = torch.where(use, frame_scene(x), x)
    # residual corner error after rectification (and occasionally much larger)
    j = torch.where(torch.rand(b, 1, 1, device=DEV) < 0.85, 0.04, 0.10) * torch.rand(b, 1, 1, device=DEV)
    src = torch.tensor([[0, 0], [W - 1, 0], [W - 1, H - 1], [0, H - 1]], device=DEV, dtype=torch.float32)
    src = src.expand(b, 4, 2)
    dst = src + (torch.rand(b, 4, 2, device=DEV) * 2 - 1) * j * torch.tensor([W, H], device=DEV)
    mat = kornia.geometry.transform.get_perspective_transform(src, dst)
    x = kornia.geometry.transform.warp_perspective(x, mat, (H, W), padding_mode="zeros")
    mask = kornia.geometry.transform.warp_perspective(torch.ones(b, 1, H, W, device=DEV), mat, (H, W))
    bg = torch.rand(b, 3, 1, 1, device=DEV) * 0.6 + torch.randn(b, 3, H, W, device=DEV) * 0.05
    x = x * mask + bg * (1 - mask)
    # occlusion: another card or a skin-coloured block over an edge/corner
    occ = torch.rand(b, device=DEV) < 0.3
    if occ.any():
        other = x.roll(1, 0)
        skin = torch.tensor([0.8, 0.6, 0.5], device=DEV).view(1, 3, 1, 1).expand_as(x)
        src_img = torch.where(torch.rand(b, 1, 1, 1, device=DEV) < 0.5, other, skin)
        fh = (0.2 + 0.35 * torch.rand(b, 1, 1, device=DEV)) * H
        fw = (0.3 + 0.5 * torch.rand(b, 1, 1, device=DEV)) * W
        yy = torch.arange(H, device=DEV).view(1, H, 1).float()
        xx = torch.arange(W, device=DEV).view(1, 1, W).float()
        top = torch.rand(b, 1, 1, device=DEV) < 0.5
        left = torch.rand(b, 1, 1, device=DEV) < 0.5
        my = torch.where(top, yy < fh, yy > H - fh)
        mx = torch.where(left, xx < fw, xx > W - fw)
        m = (my & mx & occ.view(b, 1, 1)).unsqueeze(1).float()
        x = x * (1 - m) + src_img * m
    x = glare(x, 0.6, 0.6)
    # photometric
    gains = 1 + (torch.rand(b, 3, 1, 1, device=DEV) * 2 - 1) * 0.12
    x = x * gains
    x = (
        (x - 0.5) * (1 + (torch.rand(b, 1, 1, 1, device=DEV) * 2 - 1) * 0.3)
        + 0.5
        + (torch.rand(b, 1, 1, 1, device=DEV) * 2 - 1) * 0.15
    )
    gray = x.mean(1, keepdim=True)
    x = gray + (x - gray) * (1 + (torch.rand(b, 1, 1, 1, device=DEV) * 2 - 1) * 0.35)
    x = x.clamp(0, 1) ** (0.75 + 0.55 * torch.rand(b, 1, 1, 1, device=DEV))
    # resolution: 4 groups at different capture sizes
    parts = []
    for chunk in x.chunk(4):
        th = int(np.random.uniform(64, 336))
        small = F.interpolate(chunk, size=(th, max(8, int(th * W / H))), mode="bilinear", antialias=True)
        sig = (0.01 + 1.4 * torch.rand(chunk.shape[0], device=DEV)) * (th / 336) + 0.01
        small = gaussian_blur2d(small, (7, 7), torch.stack([sig, sig], 1))
        parts.append(F.interpolate(small, size=(H, W), mode="bilinear", align_corners=False))
    x = torch.cat(parts)
    if DEFOCUS_P > 0:
        sig = 1.0 + 2.0 * torch.rand(b, device=DEV)
        blurred = gaussian_blur2d(x, (13, 13), torch.stack([sig, sig], 1))
        x = torch.where(torch.rand(b, 1, 1, 1, device=DEV) < DEFOCUS_P, blurred, x)
    x = x + torch.randn_like(x) * (0.03 * torch.rand(b, 1, 1, 1, device=DEV))
    x = x.clamp(0, 1)
    q = 35 + 60 * torch.rand(b, device=DEV)
    x = jpeg_codec_differentiable(x, q).clamp(0, 1)
    return x


def to_input(x: torch.Tensor) -> torch.Tensor:
    x = F.interpolate(x, size=(IN_H, IN_W), mode="bilinear", antialias=True, align_corners=False)
    return (x - MEAN) / STD


# ---------------- model ----------------
class Embedder(nn.Module):
    def __init__(self, arch: str, dim: int = 256):
        super().__init__()
        self.backbone = timm.create_model(arch, pretrained=True, num_classes=0)
        with torch.no_grad():
            feat = self.backbone.eval()(torch.zeros(1, 3, IN_H, IN_W)).shape[1]
        self.head = nn.Linear(feat, dim)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return F.normalize(self.head(self.backbone(x)), dim=1)


@torch.no_grad()
def embed_uint8(model: nn.Module, arr, bs: int = 256) -> torch.Tensor:
    outs = []
    for i in range(0, len(arr), bs):
        x = torch.from_numpy(np.ascontiguousarray(arr[i : i + bs])).to(DEV).permute(0, 3, 1, 2).float() / 255
        with torch.autocast("cuda", dtype=torch.bfloat16):
            outs.append(model(to_input(x)).float())
    return torch.cat(outs)


def load_queries(cond: str) -> np.ndarray:
    return np.stack(
        [
            cv2.cvtColor(cv2.imread(str(OUT / "queries" / cond / f"{i}.jpg")), cv2.COLOR_BGR2RGB)
            for i in range(len(qidx))
        ]
    )


@torch.no_grad()
def evaluate(model: nn.Module, gallery_np: np.ndarray, queries: dict, tag: str) -> list[str]:
    model.eval()
    g = embed_uint8(model, gallery_np)
    lines = []
    for cond, q in queries.items():
        sim = torch.maximum(
            embed_uint8(model, q) @ g.T, embed_uint8(model, np.ascontiguousarray(q[:, ::-1, ::-1])) @ g.T
        )
        top = sim.topk(60, dim=1).indices.cpu().numpy()
        res = {"seen": [0, 0, 0], "held": [0, 0, 0]}
        for qi, row in enumerate(top):
            t = qidx[qi]
            got: list[str] = []
            for gi in row:
                if card_of[gi] not in got:
                    got.append(card_of[gi])
                if len(got) == 3:
                    break
            k = "held" if held[t] else "seen"
            res[k][0] += int(got[0] == card_of[t])
            res[k][1] += int(card_of[t] in got)
            res[k][2] += 1
        line = (
            f"{tag}\t{cond}\tseen top1={res['seen'][0] / res['seen'][2]:.3f} top3={res['seen'][1] / res['seen'][2]:.3f}"
            f" (n={res['seen'][2]})\theld top1={res['held'][0] / max(1, res['held'][2]):.3f}"
            f" top3={res['held'][1] / max(1, res['held'][2]):.3f} (n={res['held'][2]})"
        )
        log(line)
        lines.append(line)
    model.train()
    return lines


def main() -> None:
    arch, epochs = sys.argv[1], int(sys.argv[2])
    tag = sys.argv[3] if len(sys.argv) > 3 else arch
    from sve_scan import gallery as GL

    GL.current(OUT)  # refuse an image array built for other gallery rows
    gallery_np = np.load(OUT / "gallery.npy")
    gallery_t = torch.from_numpy(gallery_np)
    queries = {c: load_queries(c) for c in COND}
    log("held cards", len(held_cards), "held printings", int(held.sum()), "train classes", len(train_idx))

    model = Embedder(arch).to(DEV)
    init = os.environ.get("INIT", "")  # fine-tune from an earlier model instead of ImageNet weights
    lr_scale = float(os.environ.get("LR_SCALE", "1"))
    if init:
        model.load_state_dict(torch.load(OUT / f"model_{init}.pt", map_location=DEV))
        # start the proxies at that model's embeddings of the clean images, so nothing it knows is reset
        proxies = nn.Parameter(embed_uint8(model.eval(), gallery_np[train_idx]).clone())
        model.train()
        log("init from", init, "lr scale", lr_scale)
    else:
        proxies = nn.Parameter(F.normalize(torch.randn(len(train_idx), 256, device=DEV), dim=1))
    bs = 64
    steps = epochs * (len(train_idx) // bs)
    opt = torch.optim.AdamW(
        [
            {"params": model.backbone.parameters(), "lr": 6e-4 * lr_scale, "weight_decay": 1e-4},
            {"params": model.head.parameters(), "lr": 3e-3 * lr_scale, "weight_decay": 1e-4},
            {"params": [proxies], "lr": 3e-2 * lr_scale, "weight_decay": 0.0},
        ]
    )
    base = [g["lr"] for g in opt.param_groups]
    s_scale, margin = 30.0, 0.2
    model.train()
    t0 = time.time()
    perm = np.random.default_rng(2).permutation(train_idx)
    pos = 0
    for step in range(steps):
        warm = min(1.0, (step + 1) / 300)
        cosf = 0.5 * (1 + math.cos(math.pi * step / steps))
        for g, b0 in zip(opt.param_groups, base):
            g["lr"] = b0 * warm * cosf
        if pos + bs > len(perm):
            perm = np.random.default_rng(step).permutation(train_idx)
            pos = 0
        idx = perm[pos : pos + bs]
        pos += bs
        x = gallery_t[torch.from_numpy(idx)].to(DEV, non_blocking=True).permute(0, 3, 1, 2).float() / 255
        with torch.no_grad():
            x = augment(x)
        cls = torch.from_numpy(cls_of[idx]).to(DEV)
        target = torch.zeros(bs, len(train_idx), device=DEV)
        for r, gi in enumerate(idx):
            sib = same_card[sib_key[gi]]
            if len(sib) > 1:
                target[r, sib] = 0.2 / (len(sib) - 1)
                target[r, cls_of[gi]] = 0.8
            else:
                target[r, cls_of[gi]] = 1.0
        with torch.autocast("cuda", dtype=torch.bfloat16):
            e = model(to_input(x))
        cos = e.float() @ F.normalize(proxies, dim=1).T
        logits = s_scale * (cos - margin * F.one_hot(cls, len(train_idx)))
        loss = -(target * F.log_softmax(logits, dim=1)).sum(1).mean()
        opt.zero_grad(set_to_none=True)
        loss.backward()
        opt.step()
        if step % 100 == 0:
            acc = (cos.argmax(1) == cls).float().mean().item()
            log(
                f"step {step}/{steps} loss {loss.item():.3f} acc {acc:.3f} "
                f"{(step + 1) * bs / (time.time() - t0):.0f} img/s mem {torch.cuda.max_memory_allocated() / 2**30:.1f}G"
            )
        if step == 0:
            torchvision_save(x[:16])
    lines = evaluate(model, gallery_np, queries, tag)
    with open(OUT / "train_results.tsv", "a") as f:
        f.write("\n".join(lines) + f"\n# {tag} epochs={epochs} minutes={(time.time() - t0) / 60:.1f}\n")
    torch.save(model.state_dict(), OUT / f"model_{tag}.pt")
    log("done", f"{(time.time() - t0) / 60:.1f} min")


def torchvision_save(x: torch.Tensor) -> None:
    grid = torch.cat([torch.cat(list(x[i * 8 : (i + 1) * 8]), dim=2) for i in range(2)], dim=1)
    img = (grid.permute(1, 2, 0).float().cpu().numpy() * 255).astype(np.uint8)
    cv2.imwrite(str(OUT / "train_aug_sample.jpg"), cv2.cvtColor(img, cv2.COLOR_RGB2BGR))


if __name__ == "__main__":
    main()
