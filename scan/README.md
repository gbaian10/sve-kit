# scan：拍卡辨識的模型與卡片索引

手機拍卡（單張對準、整副牌照片）用的兩個模型與卡片索引，從卡圖庫、訓練、匯出、評估到發佈檔，都在這裡產生。
推論本身在 `sim/web`（瀏覽器裡的 ONNX Runtime）；這裡的 Python 推論只用來評估與標註，網頁版是準。

- **找卡模型**（det）：256×256 輸入，找出畫面中每張卡的四個角
- **辨識模型**（rec）：拉正後的卡 → 256 維向量，在卡片索引裡找最像的卡
- **卡片索引**：所有卡圖的向量（int8）＋hub 校正值＋卡片清單，依卡面版本分 jp／en

產物格式見 [docs/schema/scan/artifacts.md](../docs/schema/scan/artifacts.md)。

`scan/` 是獨立的 uv 專案，不在根目錄的 workspace 裡（PyTorch 不進 carddb 的環境）。
讀匯出只透過 `sve_carddb.export.read_api`，和 `publish/` 相同。

## 安裝

```bash
uv --directory scan sync                  # 輕量：索引、發佈檔、評估已匯出的模型、測試
uv --directory scan sync --group train    # 加上 PyTorch（CUDA 13.0 wheel），訓練與匯出要用
uv --directory scan run pytest            # CI 跑的就是這組，不需要 GPU 或私有資料
```

- 系統的舊版 cuBLAS 若經由 `LD_LIBRARY_PATH` 混進來，torch 的卷積會閃退；跑訓練前用 `env -u LD_LIBRARY_PATH`
- 長時間工作請限制 CPU 與記憶體（例如 `systemd-run --user --scope -p CPUQuota=300% -p MemoryMax=12G`）

## 資料位置

所有路徑都由環境變數給，repo 裡不寫任何本機路徑。

| 變數 | 內容 |
| --- | --- |
| `SVE_SCAN_EXPORT` | 公開匯出根：`snapshots/` 與 `images/<size>/<int_id>.webp`（卡圖庫的來源） |
| `SVE_SCAN_WORK` | 產生的檔案：卡圖庫、紋理庫、checkpoint、ONNX、索引、評估結果、記錄 |
| `SVE_SCAN_DATA` | 私有照片與由它衍生的素材，永遠不發佈、不進 repo |

`SVE_SCAN_DATA` 底下的結構：

| 路徑 | 內容 |
| --- | --- |
| `real/<資料夾>/manifest.jsonl` | 標好卡號的實拍照，每列 `{file, region, card_no, [conditions], [exclude]}` |
| `real/phone-2026-10-18/`、`real/det-train/` | 維護者自己拍的瞄準存檔與側面連拍，只當驗收 |
| `web-x/sample-events-orig/<賽事>/` | 社群網站玩家貼文的牌組照片；`auto-labels/` 放分類與確認標籤 |
| `web-det/pokemon-card-segmentation/` | 公開的 CC0 卡片分割資料，找卡模型的實拍訓練資料 |

資料來源只寫成「二手拍賣平台的商品照」「社群網站的玩家貼文」。紋理庫與實拍照不散布，只發布做紋理庫的程式。

### 驗收資料隔離

`sve_scan/splits.py` 列的六場賽事（開始日起 8 天內的貼文）是驗收集：不拿來訓練、不做找卡標籤、不做任何紋理庫。
當紋理來源的照片資料夾也永遠不當驗收。新的照片集先跑 `labelcheck`（人工標籤約有 3% 錯）。

## 流程

每一步都是 `uv --directory scan run python -m sve_scan <指令>`（不帶參數會列出全部指令）。
下面的 tag 只是工作目錄裡的檔名；對外的版本是 `rec-1`、`det-1` 這種整數編號，由發佈時指定。

### 1. 卡圖庫

```bash
python -m sve_scan gallery          # gallery.json、gallery.npy、合成查詢照、gallery-meta.json（data_version）
python -m sve_scan fetch-bg         # CC0 桌面紋理
python -m sve_scan fetch-rooms      # CC0 室內全景截圖
```

卡圖庫＝匯出快照裡每個已核可、可用的版次卡面圖（日英兩版），異圖分組用快照的 `art_id`。

### 2. 紋理庫（需要私有資料）

```bash
python -m sve_scan deck-label <rec tag> <det tag> labels.json <out dir>   # 牌組照片的 SIFT 確認標籤
python -m sve_scan texbank texbank.npz <資料夾> ...                         # 閃卡紋理（二手拍賣商品照）
python -m sve_scan sleeve-bank <out dir>                                         # 卡套反光層（非驗收賽事的牌組照片）
python -m sve_scan banks texbank.npz <out dir>/sleeve_material.npz realbank.npz procbank.npz
```

### 3. 辨識模型

目前的模型分兩段訓練：

```bash
# 第一段：從 ImageNet 預訓練開始，40 輪，只用閃卡紋理
SIBLINGS=art FRAME_P=0.6 FRAME_SMIN=0.4 HOLO_P=0.2 DEFOCUS_P=0.3 FOIL_P=0.3 REAL_P=0.4 TEXBANK=texbank.npz \
  python -m sve_scan rec-train mobilenetv3_large_100.ra_in1k 40 rec-base

# 第二段：接續第一段，學習率 ×0.25，12 輪；閃卡紋理維持原比例，另外加卡套反光與程序紋理
INIT=rec-base LR_SCALE=0.25 SIBLINGS=art FRAME_P=0.6 FRAME_SMIN=0.4 HOLO_P=0.2 DEFOCUS_P=0.3 FOIL_P=0.3 \
  REAL_P=0.4 TEXBANK=texbank.npz REAL2_P=0.3 REALBANK=realbank.npz PROCBANK=procbank.npz WR_P=0.2 HS_P=0.2 MISC_P=0.1 \
  python -m sve_scan rec-train mobilenetv3_large_100.ra_in1k 12 rec-ft

python -m sve_scan rec-export mobilenetv3_large_100.ra_in1k rec-ft   # rec-ft.w8.onnx（只量化權重）
python -m sve_scan rec-hub mobilenetv3_large_100.ra_in1k rec-ft      # hub 校正值
python -m sve_scan index rec-ft                                       # index/rec-ft/{jp,en}.*
```

- 一次從頭訓練、把所有紋理混在一起會退步（閃卡紋理的比例被稀釋）。新版的規則是：任何一組驗收都不能比前一版差
- 靜態 int8（連激活值一起量化）會讓實拍第一名從 91% 掉到 69%，所以只量化權重
- 換辨識模型或卡圖庫時，hub 值和索引一定一起重算；漏掉 hub 會讓某幾張卡一直搶第一名
- 出新卡包：重跑 `gallery`、`rec-hub`、`index`，模型不用重訓（訓練時有約 8% 的卡故意不訓練，用來確認這件事）

### 4. 找卡模型

目前的模型分四段，每段接續前一段：

```bash
python -m sve_scan det train det-a 15000 48                                    # 只用合成場景
python -m sve_scan det-real-build                                              # realdet_pokemon.jsonl
INIT=det-a REAL=realdet_pokemon.jsonl REAL_FRAC=0.3 LR=1e-3 python -m sve_scan det train det-b 4000 48
INIT=det-b REAL=realdet_pokemon.jsonl REAL_FRAC=0.3 LR=1e-3 REAL_P=0.3 TEXBANK=texbank.npz \
  REAL2_P=0.5 REALBANK=realbank.npz PROCBANK=procbank.npz WR_P=0.25 HS_P=0.35 MISC_P=0.15 \
  python -m sve_scan det train det-c 4000 48
DETS=det-c:1.04,det-b python -m sve_scan det-label <rec tag> photos.txt det-label.jsonl   # 非驗收賽事的牌組照片
python -m sve_scan det-real-index realdet_deck.jsonl realdet_pokemon.jsonl det-label.jsonl
INIT=det-c REAL=realdet_deck.jsonl REAL_FRAC=0.4 LR=5e-4 REAL_P=0.3 TEXBANK=texbank.npz \
  REAL2_P=0.5 REALBANK=realbank.npz PROCBANK=procbank.npz WR_P=0.25 HS_P=0.35 MISC_P=0.15 \
  python -m sve_scan det train det-d 4000 48
python -m sve_scan det export det-d det_det-d.w8.onnx
```

- 第三段的框約比卡邊小 4%，做標籤時要放大 1.04 倍（`DETS` 的 `:1.04`）；第四段學的是 SIFT 投影出的精確四角，不再放大（發佈的 `grow` 是 1.0）
- 目前的牌組照片標籤是用三個找卡模型聯集做的，其中一個是試過沒採用的版本；任何一組找卡模型都能做標籤
- `REC_TAG=<rec tag>` 時，訓練結束會在維護者的手機存檔與側面連拍上做端到端檢查
- 離線的四角誤差最好，不代表網頁上最好用；換模型一律以網頁整副牌讀取的結果為準

### 5. 評估

```bash
python -m sve_scan eval-rec rec-ft <驗收資料夾> ...                  # 單張：卡片層級 top-1／top-5（中央裁切、整張；有無 hub）
python -m sve_scan deck-label rec-ft det-d labels.json deck-out     # 再跑 eval-deck 得到整副牌召回
python -m sve_scan eval-deck deck-out
python -m sve_scan eval-det rec-ft det.json det-d:global            # 找卡：驗收賽事上的有用框與四角誤差
```

- 結果寫到 `SVE_SCAN_WORK/eval/`；同樣的輸入、同樣的模型，結果相同
- 整副牌召回以 SIFT 確認的卡為分母，偏向容易的卡，數字偏樂觀
- 網頁整副牌讀取的端到端結果才是準；這裡的 Python 版本是訓練時快速看效果用的

### 6. 發佈檔

```bash
python -m sve_scan release <out dir> 1 det-d 1 rec-ft
```

產出 `models/<sha256>.onnx`、`index/<sha256>.*` 與 `manifest.json`，給發佈步驟上傳到 R2 的 `scan/`（上傳要維護者同意）。
