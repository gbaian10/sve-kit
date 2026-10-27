# 私有圖片索引

`images build-index` 獨立產圖、驗證圖片來源及裁切、配發事件版本，產出私有圖片索引。
`snapshot export-offline --image-index` 只讀索引與文字來源，不開 PNG／WebP，不複製圖片，也不讀寫圖片版本狀態。
公開影像清單的欄位、固定 ID key、三尺寸及 `?v=` 語意不變。
文字建置 DB 不存圖片或尺寸表，公開 media 三集合由此索引加入文字投影。

## 圖片建置

```bash
uv --directory carddb run sve-carddb images build-index --inputs /private/inputs.json \
  --image-assets-dir /private/webp-library --image-cache-dir /private/recipe-cache \
  --image-output-dir /private/display-images --private-dir /private/image-state
```

配方與 [文字匯出](../export/preview-handoff.md) 相同，圖片建置只使用 identity、卡頁、圖片批次與裁切資料，
不跑翻譯、QA 或完整文字 DB。`--workers` 預設 2、範圍 1～4。
四個輸出／快取根必須為絕對且互不包含的路徑，也不得與 repo、封存庫或配方重疊；使用的圖片檔不得經 symlink 逸出根。

- `image-assets-dir`：內容定址 WebP，維持 `images/sha256/<prefix>/<hash>.webp`。
- `image-cache-dir`：原圖 SHA-256＋裁切覆寫設定＋完整 recipe 的轉檔快取，查找先於 PNG 解碼。
- `image-output-dir`：可供預覽及上傳的固定 ID key（含 `images/`）。只寫變動版本或缺少的檔案。
- `private-dir`：持久 `media-state.json`、`image-index/<sha256>.json` 與本次裁切診斷 `image-report.json`。命令 stdout 的單行 JSON 回報完整索引路徑、執行計量及四項裁切診斷的摘要。報告格式與診斷規則見[裁切覆寫契約 §3](image-crop-overrides.md#3-解析建置驗證與報告)。

日常圖片建置核對封存 raw 的存在及大小，命中快取時亦只核對 WebP 的存在／大小。
新來源或快取未命中時讀取、驗 hash 及解碼；新寫出的固定 key 驗回 hash。
`--verify-all` 完整重驗封存來源與 WebP hash／格式／尺寸，供定期或發布前查核。
同尺寸的靜默損壞可能延至完整查核才發現；來源封存、備份及 restore 的完整驗證不變，原 PNG 不刪除。

首次改用本流程時，將既有成功匯出的 `media-state.json` 搬入指定圖片私有根並保留備份；
`media-state.json` 是配號的唯一依據，換機器或換圖片私有根時必須一起搬並備份，由同一 producer 跨次沿用。
缺 state 時命令會拒絕執行，只有明示 `--initial` 才可建立全新的配號狀態；已有 state 時也拒絕 `--initial`。
未變圖片沿用原 v，裁切只影響 art 組；失敗配號不回收。
圖片建置失敗時不得發布其部分寫出的圖片，須重跑成功後使用該次索引。若圖片根亦存有 preview 指標，
覆寫不同 bytes 前先移除該指標，避免中斷後載入不完整的圖片組。

## 索引邊界

索引是圖片流程的衍生資料，檔名 SHA-256 是文字匯出的明確輸入 pin，不以文字 repo revision 當圖版本。
格式 `sve-image-index-v1` 由 `export.image_index.ImageIndex` 驗證：

| 欄位 | 消費用途 |
| --- | --- |
| `format`、`recipe` | 索引格式與完整轉檔 recipe hash |
| `store_id`、`pins` | 明確的區域／圖片批次，須與文字配方相同 |
| `evidence` | 已驗的校正圖片來源 pins，含實際用到的歷史來源；沿用 `Source` 型別 |
| `bindings` | 版次／面與永久 int_id／ordinal，核對固定 key 身分 |
| `tables` | 已配號的 `image_asset`、`printing_image`、`image_variant` 三種既有公開列 |

索引不含 bitmap、本機圖片路徑或第二份卡表。文字端用已解析卡頁核對每面原始 `img src`、解析 URL、區域與永久 ID，
並核對公開狀態、三尺寸及版本 metadata；校正核對精確的區域／URL／hash／來源 pin，不重驗圖本體。
索引與校正來源 pins 只留在私人建置環境，不放公開根、git 或測試 artifact。

`sve-publish images upload` 使用卡圖上傳描述與 `--image-dir` 的固定 key 圖片根，只比對清單並驗讀變動候選。
`sve-publish snapshot upload --export-dir` 只讀公開 JSON 根與遠端清單；文字匯出與圖片根須使用相符的成功圖片索引。

## 公開上傳交接

`images build-index` 在圖片與私有版本狀態成功後，另寫公開 `image-upload/<sha256>.json`，stdout 的 `image_upload` 指向該檔。
格式為 `sve-image-upload-v1`，包含 `image_index_sha256`、producer `revision` 與按 key 排序的 `objects`；
每列只有公開 `key`、所屬 card/art 事件 `version`、`sha256`、`bytes`。逐檔 hash 直接取自已驗產圖資料，
不以 media-state 的群組指紋代替，也不為交接重讀未變圖片。不含來源證據、URL 或私人路徑。
只有成功建置的描述與未變動圖片根可交給 `sve-publish images upload`；
公開 reader `load_image_upload` 驗內容定址與欄位，publisher 不讀私有索引或版本狀態。
遠端清單與失敗恢復見 [publish](../../../publish/README.md)。

## 指定產圖範圍

`images build-index` 可重複帶 `--scope BP22 --scope PR`，或用可重複的
`--printing-id p:…` 指定版次，兩者取聯集。卡包按登錄的 `home_set_id` 精確比對；
未知卡包與未登錄版次都失敗，不從卡號或來源 URL 推測歸屬。未帶範圍維持全量處理。

指定範圍必須沿用既有 `private-dir` 的版本狀態，不能帶 `--initial`，並提供：

| 參數 | 輸入 |
| --- | --- |
| `--previous-index` | 上一版內容定址的私有圖片索引 |
| `--previous-image-upload` | 釘住該索引的公開卡圖上傳描述 |
| `--previous-image-dir` | 上一版固定 key 的完整 WebP 根 |
| `--previous-reuse-evidence` | 釘住同一索引的私有圖片沿用證據 |

成功建置另產出私有 `image-reuse/<sha256>.json`，stdout 的 `image_reuse` 指向該檔。
格式 `sve-image-reuse-v1` 保存 `image_index_sha256`、完整 `recipe` 定義與 `crops`
輸入記錄；沒有命中記錄表示沒有覆寫。它不重複維護圖片 hash，WebP 的 hash 與 bytes
仍取自卡圖上傳描述。缺少此證據的舊建置須先完整重建基準，不能假定目前裁切就是歷史裁切。
不回填或重建轉檔快取。

範圍外每個繫結都核對兩次凍結批次的區域、URL、來源 sha256 與身分，及新舊裁切覆寫座標。
上一版 recipe hash 與完整 recipe 定義必須等於目前定義，含尺寸、品質、裁切演算法與
Pillow、libwebp、LittleCMS 版本；不同就要求全量重轉。每個範圍外 WebP 都讀取並核對
sha256、bytes，再複製到輸出，不解碼或編碼範圍外來源、不使用其轉檔快取。
來源或裁切變更、檔案缺失或內容不同時失敗並指出需擴大範圍的版次。

新索引仍包含全部繫結與三種公開表。範圍外的繫結、圖片列、尺寸及事件版本須與舊索引逐列相同；
版本狀態不符、範圍外版次被移除或改綁定，都拒絕完成。多個版次共享來源時，只要其中一個在
範圍內就可能轉檔，但仍須通過範圍外每個繫結的完整檢查。

範圍建置的裁切診斷只涵蓋目前版次引用的來源；全量建置則涵蓋整個凍結圖片批次。
因此未被版次引用的來源若有裁切覆寫，在範圍建置中會列為 `unused`，不影響完整索引或圖片沿用驗證。
所有索引、沿用證據及公開交接檔寫完後才提交版本狀態；中斷時沿用相同輸入與命令重跑。

成功建置也產出公開 `image-scope/<sha256>.json`，stdout 的 `image_scope` 指向該檔。
格式 `sve-image-scope-v1` 的 `image_upload_sha256` 釘卡圖上傳描述；`bindings` 列出
`printing_id`、`home_set_id` 與固定 `keys`，全部 key 必須恰好覆蓋該描述，且版次及 key 不重複。
這份清單只保存公開範圍歸屬，不含來源 URL、來源證據或本機路徑。上傳同一範圍時帶
`--scope-manifest` 使用此清單；既有圖片索引與卡圖上傳描述的格式不變。
