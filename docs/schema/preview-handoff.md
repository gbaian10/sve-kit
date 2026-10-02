# JP preview 建置與前端接線

preview 使用正式傳輸契約與共用匯出器，但不是正式發布。`data_version` 必須有
`preview-` 前綴，`regions` 固定為 `jp`。payload 的欄位、分片 N、bootstrap/detail
分工不因 preview 或容量目標而改變。

## 建置

從 repo 根目錄執行：

```bash
SVE_PREVIEW_DIR=/explicit/isolated/preview \
SVE_CDN_DIR=/explicit/formal/cdn \
uv --directory carddb run sve-carddb snapshot export --inputs /private/jp-inputs.json
```

也可用 `--preview-dir`、`--cdn-dir` 明確指定；兩者都必填，preview 沒有預設位置。
`SVE_PREVIEW_DIR` 指到含 `snapshots/` 的那一層，而非 `snapshots/` 或 `snapshots/preview/`。
解析 symlink 後，兩個 root 不得相同或互相包含；preview 亦不得與輸入 repo／封存庫
相同或互相包含。root 內的輸出 symlink 不得逸出 preview root。

`--inputs` 是呼叫端明確提供的 JSON 配方，欄位如下；路徑與實際來源 pin 留在私人建置環境，
不進 repo。配方、詞彙綁定與衍生產物可能含官方文字，適用相同的私人資料邊界。

| 欄位 | 意義 |
| --- | --- |
| `repo`、`archive`、`store_id` | 唯讀 repo、封存庫與 store 識別 |
| `card_batch`、`image_batch` | JP HTML 批次與校正證據圖批次的完整 `sha256:` pin |
| `revision`、`parser_version` | 40 碼程式／authored revision 與明確 parser 配方 |
| `vocabulary` | 私人 `Vocabulary` JSON；`bindings` 每項有 `region,kind,raw,code,special_kinds` |
| `languages` | 明確語言列：`code,display_name,fallback_order` |
| `as_of`、`data_version`、`published_at` | 查核日期、preview 批次識別與 UTC 時間 |
| `feedback_url`、`grammar_version`、`normalizer_version` | 公開 config；normalizer 應與目標前端一致 |

建置只讀凍結批次與 authored，不開 live manifest、不抓官網。封存來源逐份驗 hash，
輸入記錄驗 source-use 閉包，建置 DB 驗 FK／JSON，公開投影驗參照閉包；匯出後由
獨立 reader 重接所有欄分工、位置引用與分片，完整文字包亦須等於分片結果。

公開版次取自 `TextPlan.publication_identity()`。診斷 staging 可以保留未核可父列，
公開投影先以發布版次篩選再做區域閉包；不拿 `diagnostic_exclusions` 當發布名單。
卡文未定及有勘誤連結本身不排除卡；實際身分未核可／校正衝突等排除逐版次列理由。
沒有 QA／勘誤／CR／禁限來源覆蓋的配方，清單窗口維持空集合，語意為 unknown；
稀疏卡號列表不能證明「沒有勘誤／QA」，缺規則來源也不能宣稱合法。

## 輸出與容量

分片與完整文字包存於 `snapshots/blobs/<sha256>.json`，另存固定時間戳、等級 9 的
gzip。若提供 `--brotli-command /explicit/compressor`，該程式須接受 `--version` 及
`-q 11 -c`（stdin 原始 bytes，stdout 壓縮 bytes）；報告釘住其版本與程式 hash。
沒有明確 Brotli compressor 時，br 數字為 null，不能宣稱通過 br 容量目標。

清單存於 `snapshots/manifests/<sha256>.json`。所有不可變成員驗證／寫入後才原子更新
`snapshots/preview/current.json`，內含 `manifest_path,manifest_sha256`。
不寫 `snapshots/versions/index.json` 或 pages，也不碰正式 active／快取。
已有同名不可變檔案的 bytes 不符即停止，保留既有 preview 指標。

`reports/<manifest-hash>.json` 保存輸入 hash、JP 範圍、逐表數量、真正排除清單、
未定卡文數、容量及未完成的正式閘門；輸入記錄留在 `private/inputs/<input-hash>.json`。
這些檔案與輸出都不進 git、Actions cache／artifact 或測試 fixture。
容量以每個所選批次各自計算；清單與 config 計一次，完整文字包不與分片重複加總，
卡圖另計。以 br 為準，啟動包 1 MiB 是約略目標，超過時照實列數字，不自行修改格式。

`snapshot publish MANIFEST` 在任何寫入前拒絕 `preview-` 產物。正式發布其餘閘門與
append-only index 屬 #34，目前命令在正式版號下也會停止；改掉前綴不能把 preview
直接升為正式發布，正式批次須重新建置並通過完整發布閘門。

## M3 載入

```bash
SVE_CDN_DIR=/explicit/formal/cdn \
SVE_PREVIEW_DIR=/explicit/isolated/preview \
mise exec -- bun run --cwd sim/web dev
```

前端 dev server 以 `/cdn-preview` 提供唯讀檔案。開啟查卡畫面，用前端開發徽章內的 preview 切換
選擇該 root；client 讀 `snapshots/preview/current.json`，驗清單 hash，載入 config 與
bootstrap，卡片詳情再取 detail。preview 切換使用獨立 client 與記憶體狀態，不修改
正式選版、永久分享或回放 pin。正式 CDN 可以仍指向合成 fixture。

卡文未定會有候選／展示文字與 pending 狀態，不能把顯示文字當現行已確認卡文。
`errata_card_ids`／`qa_card_ids` 為空不代表沒有勘誤／問答；應以 `source_windows` 為空
判讀來源覆蓋為 unknown，不可將稀疏卡號清單當作完整性證明。
M3 畫面驗收與其餘正式閘門應分別確認，不因成功載入 preview 宣稱完成正式資料驗收。

## 卡圖接點

此配方的 image batch 是來源校正證據，不是可發布圖片清單。轉好的 WebP 要在 #35
接入 `image_asset`、`printing_image`、`image_variant` 與 images 分片，提供明確版次／面
對應、核可與可用狀態、各尺寸 path／格式／尺寸／bytes，並讓 images 描述與 config
尺寸契約一致。不能由卡號推算 URL。接入前前端使用無圖佔位，現有轉檔庫保持唯讀。
