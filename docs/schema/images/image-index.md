# 私有圖片索引

`images build-index` 獨立產圖、驗證圖片來源及裁切、配發事件版本，產出私有圖片索引。
`snapshot export-offline --image-index` 只讀索引與文字來源，不開 PNG／WebP，不複製圖片，也不讀寫圖片版本狀態。
公開影像清單的欄位、固定 ID key、五尺寸及 `?v=` 語意不變。
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
並核對公開狀態、五尺寸及版本 metadata；校正核對精確的區域／URL／hash／來源 pin，不重驗圖本體。
索引與校正來源 pins 只留在私人建置環境，不放公開根、git 或測試 artifact。

`sve-publish images upload` 使用成功圖片描述與 `--image-dir` 的固定 key 圖片根，只比對清單並驗讀變動候選。
`sve-publish snapshot upload --export-dir` 只讀公開 JSON 根與遠端清單；文字匯出與圖片根須使用相符的成功圖片索引。

## 公開上傳交接

`images build-index` 在圖片與私有版本狀態成功後，另寫公開 `image-upload/<sha256>.json`，stdout 的 `image_upload` 指向該檔。
格式為 `sve-image-upload-v1`，包含 `image_index_sha256`、producer `revision` 與按 key 排序的 `objects`；
每列只有公開 `key`、所屬 card/art 事件 `version`、`sha256`、`bytes`。逐檔 hash 直接取自已驗產圖資料，
不以 media-state 的群組指紋代替，也不為交接重讀未變圖片。不含來源證據、URL 或私人路徑。
只有成功建置的描述與未變動圖片根可交給 `sve-publish images upload`；
公開 reader `load_image_upload` 驗內容定址與欄位，publisher 不讀私有索引或版本狀態。
遠端清單與失敗恢復見 [publish](../../../publish/README.md)。
