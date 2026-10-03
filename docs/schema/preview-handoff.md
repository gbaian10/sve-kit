# JP preview 建置與前端接線

本文既有命令與內容定址圖片輸出描述的是已實作 1.x；2.0 規格不表示這些入口已完成升級。
2.0 須同步 [圖片發布契約](image-variants.md#20-圖片-url版本與新鮮度) 與
[傳輸 §5.4](snapshot-transport.md#54-format-200-卡包-media-與-id-圖片)：輸出卡包 media 的版本／尺寸，
從建置 hash path 產生固定 ID key，圖片可受控覆寫、JSON 不可變，驗新 v 後才切指標；
preview 配號與快取仍隔離，不寫正式 current／previous 索引。圖片只取 current，metadata 僅最新＋前一版。
正式 publisher 仍拒絕 preview 版號；不得把 preview 直接升格或把本文件的舊 create-only 規則當 2.0 已驗收。

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

若要接入已轉好的 JP 卡圖，同時提供 `--image-assets-dir /readonly/webp-library` 與
`--image-cache-dir /readonly/recipe-cache`。前者指到含 `images/` 的那一層，後者指到含
`image-variants/` 的那一層。兩者皆唯讀，只重用通過來源批次／配方／hash／解碼尺寸
驗證的 #152 資產；缺件或快取不符即停止，不自動轉檔或修補。使用最多 4 個 worker。
圖片庫與配方快取、preview／正式 root、repo／封存庫須為絕對路徑且彼此隔離，
圖片輸入不可含 symlink。未提供這對選項時仍可建置文字 preview。

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

公開 WebP 存於 `images/sha256/<前兩碼>/<64hex>.webp`。先驗證／寫入圖片，再寫 images
分片與其餘快照成員；只複製公開 `printing_image` 引用且可用、核可的變體，依 hash 去重。
不複製原始 PNG、數位卡圖或圖片庫的其他檔案。切換前再驗公開資產的 hash、bytes 與
實際解碼格式／尺寸；中斷可以留下未引用的完整資產，但既有完整 preview 與指標不變。

清單存於 `snapshots/manifests/<sha256>.json`。所有不可變成員驗證／寫入後才原子更新
`snapshots/preview/current.json`，內含 `manifest_path,manifest_sha256`。
不寫 `snapshots/versions/index.json` 或 pages，也不碰正式 active／快取。
已有同名不可變檔案的 bytes 不符即停止，保留既有 preview 指標。

`reports/<manifest-hash>.json` 保存輸入 hash、JP 範圍、逐表數量、真正排除清單、
未定卡文數、容量及未完成的正式閘門；輸入記錄留在 `private/inputs/<input-hash>.json`。
preview 根下的 `private/` 與 `reports/` 不屬於公開內容；正式上傳只傳 `snapshots/` 與
`images/`，不可把整個 preview 根直接上傳。
含圖建置另記 `image_assets` 的來源／綁定／變體數，以及 `images` 的公開去重檔數／bytes。
實際執行時間與快取命中數放在命令 stdout 的 `image_execution`，不混入不可變清單或
報告，確保相同輸入重建的逐檔 bytes 一致；唯讀重用的新轉檔時間為 0。
這些檔案與輸出都不進 git、Actions cache／artifact 或測試 fixture。
完整文字容量以批次的同一 File 聯集計算，完整文字包不與分片重複加總；卡圖另計。
啟動則依使用者所選日版／英版各自以 Brotli 計完整新清單＋config＋首屏實際必載片／依賴，
每個版本的共用／混區 File 整檔計入，不按語言比例分攤。約 1 MiB 是盡量的約略目標，
2 MiB 可接受；更大須停下交維護者決定該配置。raw／gzip 另報，不設 gzip 啟動 1 MiB gate。

[傳輸契約 §5.1](snapshot-transport.md#51-format-110-固定配置) 的 1.1.0 是獨立協商的新配置，
舊 1.0.0／N=1 預覽仍可使用；producer／reader 未同步前不切到新 minor。新配置移動的
只有同名兩表的整表儲存與固定分片配置，pending／名稱／facet／公開欄位不縮減。
每個實際資料 File（含 types）raw≤512 KiB，完整文字仍守 40／8／10 MiB。
首屏後預取所有 images metadata bytes，不建立全庫物件索引、也不預取全部圖片 blob；
所選頁面才解析必要 image 列與下載可見圖。saveData 可延後背景工作，未完成須標進度。
完整 metadata 傳輸與冷／暖頁成本另報，不能把延後下載當成節流或離線已完成；
CacheStorage 的已驗 bytes 成功保存且未被清除時，翻回暖頁不向外重抓。

`snapshot publish MANIFEST` 在任何寫入前拒絕 `preview-` 產物。正式發布其餘閘門與
current／previous 版本索引依 snapshot-format §4.1，屬 #34 的發布工作；目前命令在正式版號下也會停止；改掉前綴不能把 preview
直接升為正式發布，正式批次須重新建置並通過完整發布閘門。

Cloudflare 開發部署、公開目錄上傳與未登入入口驗收，見
[Cloudflare 開發環境設定清單](../deployment/cloudflare-development.md)。

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

含圖配方使用同一凍結 image batch 作來源校正證據與圖片來源；圖片引用取自凍結頁面的
實際 `<img src>` 與已採用的 `source_face_map`，不能由卡號推算 URL，也不按邏輯面順序
猜測正反面。`printing_image` 以版次／面指向 `image_asset`，`image_variant` 提供每個
尺寸的 path／WebP 格式／實際尺寸／bytes；images 分片描述與 config 的尺寸契約一致。

`unfetched`／`missing` 只有 metadata，沒有變體或假路徑；`pending`／`withdrawn` 亦無
公開變體。第三方 approved 圖片須通過 DB 的逐圖片 confirmed 審核與來源證據閘門；
writer 必須拿到該圖片的已驗證審核集合，不能拿另一張的審核替代。官方來源的核可
不等同於第三方審核，且 preview 不能繞過既有資料完整性規則。
此集合由 builder 在 `project()` 完成 DB 驗證後取得；writer 只核對集合成員，
不會獨立查核私人審核決定或來源證據。

M3 可直接選 `purpose=art` 的 `art_s`／`art_m`（上限 160×120／384×288，實際維持
4:3，配方為 integer-4x3-v2），改用 producer 提供的 path，無須再套用前端裁切公式。
詳情／正反面使用 `card_s`／`card_m`／`card_l`；縮圖不可放大原圖，應以變體實際尺寸
為準。缺圖或狀態未核可時維持佔位，不把 metadata 的來源網址當公開資產路徑。
