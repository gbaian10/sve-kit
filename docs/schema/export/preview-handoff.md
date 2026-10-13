# preview 建置與前端接線

preview 供隔離開發使用，匯出器產出 3.0.0。圖片投影依 [圖片發布契約](../images/image-variants.md#20-圖片-url版本與新鮮度)
與 [傳輸 §5.4](snapshot-transport.md#54-卡包-media-與-id-圖片)：從建置 hash path
產生固定 ID key，卡包 media 提供版本／尺寸；圖片可受控覆寫，JSON 不可變。
圖片流程獨立維護私有圖片索引、版本狀態與快取；文字匯出只產出 JSON。`sve-publish upload` 讀公開 JSON 根與明確指定的圖片根，
在開發桶的 current／previous 索引寫入 `preview-` entry；這不是正式發布，不能把 preview 改名升格。

preview 使用正式傳輸契約與共用匯出器，但不是正式發布。`data_version` 必須有
`preview-` 前綴，`regions` 固定為 `en`、`jp`。payload 的欄位、分片 N、bootstrap/detail
分工不因 preview 或容量目標而改變。

## 建置

從 repo 根目錄執行：

```bash
SVE_EXPORT_DIR=/explicit/isolated/preview \
SVE_CARDDB_PRIVATE_DIR=/private/preview-state \
uv --directory carddb run sve-carddb snapshot export-offline --inputs /private/inputs.json \
  --bundle-dir /private/bundle --image-index "/private/image-index/<sha256>.json"
```

`--private-dir` 或 `SVE_CARDDB_PRIVATE_DIR` 必填（CLI 優先），放文字輸入記錄與報告，須與 preview 根互不包含；
圖片 producer 的私有根另外跨次沿用並備份。

先以 `sve-carddb images build-index` 產出圖片與私有索引，操作見[圖片索引契約](../images/image-index.md)。
文字匯出以 `--image-index` 釘住內容定址索引，只核對 metadata，不需要掛載 PNG、WebP、轉檔快取或圖片版本狀態。
不提供索引時只產出無圖片 metadata 的文字預覽；已採用的圖片校正仍須提供索引。

也可用 `--preview-dir` 明確指定（優先於 `SVE_EXPORT_DIR`）；兩者至少提供一個，preview 沒有預設位置。
`SVE_EXPORT_DIR` 指到含 `snapshots/` 的那一層，而非 `snapshots/` 或 `snapshots/preview/`。
`SVE_PREVIEW_DIR` 只供 Web 的 `/cdn-preview` 使用，不再作為匯出器的環境變數。
解析 symlink 後，preview 不得與輸入 repo／封存庫相同或互相包含，也不得包含配方；
bundle 不得與 repo、封存庫或配方重疊。root 內的輸出 symlink 不得逸出 preview root。

`--inputs` 是呼叫端明確提供的 JSON 配方，欄位如下；路徑與實際來源 pin 留在私人建置環境，
不進 repo。配方、詞彙綁定與衍生產物可能含官方文字，適用相同的私人資料邊界。

| 欄位 | 意義 |
| --- | --- |
| `repo`、`archive`、`store_id` | 唯讀 repo、封存庫與 store 識別 |
| `sources` | 依 `en`、`jp` 排序的兩筆 `{region,card_batch,image_batch,parser_version}`，pin 為完整 `sha256:` |
| `revision` | 40 碼程式／authored revision |
| `as_of`、`data_version`、`published_at` | 查核日期、preview 批次識別與 UTC 時間 |
| `feedback_url`、`grammar_version`、`normalizer_version` | 公開 config；normalizer 應與目標前端一致 |
| `name_policy` | 選填；目前只接受 `approved-frozen-v1` |

建置只讀凍結批次與 authored，不開 live manifest、不抓官網。文字封存來源逐份驗 hash，圖片真實性由獨立 producer 驗證，
輸入記錄驗 source-use 閉包，建置 DB 驗 FK／JSON，公開投影驗參照閉包；匯出後由
獨立 reader 重接所有欄分工、位置引用與分片，完整文字包亦須等於分片結果。

公開版次取自 `TextPlan.publication_identity()`。診斷 staging 可以保留未核可父列，
公開投影先以發布版次篩選再做區域閉包；不拿 `diagnostic_exclusions` 當發布名單。
卡文未定及有勘誤連結本身不排除卡；實際身分未核可／校正衝突等排除逐版次列理由。
沒有 QA／勘誤／CR／禁限來源覆蓋的配方，清單窗口維持空集合，語意為 unknown；
稀疏卡號列表不能證明「沒有勘誤／QA」，缺規則來源也不能宣稱合法。

## Docker 與效能量測

將 `carddb/docker/.env.example` 複製為同目錄的 `.env`，填入配方使用的絕對路徑；
在 repo 外的執行目錄放 `recipe.json`，執行 `carddb/docker/export.sh <執行目錄>`。
容器以呼叫者的 uid／gid 執行，repo／人寫資料與封存庫唯讀掛載；
`EXPORT_CPUS`、`EXPORT_MEMORY` 設 CPU／記憶體硬上限，swap 停用。
`SVE_IMAGE_INDEX` 指定索引檔；容器不掛載圖片庫或轉檔快取。`EXPORT_BROTLI=0` 略過 Brotli，`EXPORT_PROFILE=1` 啟用 py-spy。
每完成一段即寫 stderr；`SVE_STAGE_LOG` 指定追加時間、CPU 與記憶體的 TSV，
Docker wrapper 固定寫入執行目錄的 `stages.tsv`，中斷仍保留已完成段落。
`SVE_PROFILE_FIELD_STRIDE=100` 讓日文套翻譯階段每 100 欄只處理 1 欄，供約 1% 取樣量測；
未設定時處理全部欄位。取樣產物只供效能試跑，完整匯出須移除這個設定。

## 輸出與容量

分片與完整文字包存於 `snapshots/blobs/<sha256>.json`，另存固定時間戳、等級 9 的
gzip。`snapshot export-offline` 可用 `--brotli` 額外產生 `.br`；
預設為 `--no-brotli`，br 數字為 null，不能宣稱通過 br 容量目標。
啟用時直接使用 `uv.lock` 鎖定的 PyPI `brotli`，固定 generic mode、quality 11、
lgwin 22；報告的私人壓縮 recipe 記錄套件版本，不放本機執行檔 hash。
不再提供 `--brotli-command` 或呼叫外部 encoder，亦不需準備系統 libbrotli。

`sve-publish upload` 原樣上傳清單所列的 raw／gzip／br，不重壓；選檔、標頭與 CDN 驗證見
[R2 上傳](../../../publish/README.md)，保留窗口見 [發布契約](snapshot-format.md#41-發布窗口圖片新鮮度與回收)。

圖片 producer 寫固定 ID key，文字匯出只寫公開影像 metadata 及快照 JSON，完全不複製或驗讀 bitmap。
預覽服務與上傳需另外接入成功圖片建置的圖片根；上傳使用 `--image-dir` 指定，仍全量核對圖片。

清單存於 `snapshots/manifests/<sha256>.json`。所有不可變成員驗證／寫入後才原子更新
`snapshots/preview/current.json`，內含 `manifest_path,manifest_sha256`。
匯出不寫 `snapshots/versions/index.json` 或 pages，也不碰正式 active／快取；index 只由上傳寫入。
已有同名不可變檔案的 bytes 不符即停止，保留既有 preview 指標。

私有目錄的 `reports/<manifest-hash>.json` 保存輸入 hash、地區範圍、逐表數量、真正排除清單、
未定卡文數、容量及未完成的正式閘門；輸入記錄在 `inputs/<input-hash>.json`，
圖片版本狀態另由圖片 producer 的私有根保存。這些都不在 preview 根內，不會被 dev server 提供或上傳。
含圖文字建置記錄 `image_index` 的 SHA-256。
圖片 CLI stdout 另記 `image_execution` 的快取命中、新轉檔數與 `written_images`，不混入快照清單。

這些檔案與輸出都不進 git、Actions cache／artifact 或測試 fixture。
容量設計依 [size-budget](size-budget.md)：全部文字分片＋manifest 的 raw ≤40 MiB；單一版本完整文字閉包 Brotli ≤8 MiB、gzip ≤10 MiB，含完整來源、counterpart、共享字典、卡號索引與必要跨區檔。
text_all 的封套與整檔另報，不與分片重複加總；卡圖另計。
基本目錄冷載依所選日版／英版各計完整 manifest＋config＋實際必載片及依賴，共用／混區 File 整檔計、不按語言比例分攤。
Brotli 2 MiB 為分界，略超報精確差額、明顯超出才交維護者；raw／gzip 另報，不另設硬門檻。
基本目錄容器整檔 raw ≤2 MiB，詳情與其他資料檔 ≤512 KiB，含 types／字典／base metadata。
N0 現有量測 API 與[傳輸 §5.1](snapshot-transport.md#51-固定配置) 的 bands 不代表已完成新裝檔或單區完整文字帳；不因容量減少 pending／名稱／facet／公開欄位。

只載可見面的卡包 media 與圖片；全域來源詳情按需，不全量預取圖片或建立全庫影像索引。
未完成的下載須標進度，CacheStorage 已驗 bytes 保存成功且未清除時，暖頁 metadata 不向外重抓。

正式發布閘門尚未實作（#34）；目前僅支援開發桶的 preview 上傳。正式發布的閘門與
current／previous 版本索引依 snapshot-format §4.1，屬 #34 的發布工作；上傳僅接受受支援的 preview；改掉前綴不能把 preview
直接升為正式發布，正式批次須重新建置並通過完整發布閘門。

Cloudflare 開發部署、R2 2.0 發布與未登入入口驗收，見
[Cloudflare 開發環境設定清單](../../deployment/cloudflare-development.md)。

## M3 載入

```bash
SVE_EXPORT_DIR=/explicit/formal/cdn \
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
尺寸的 WebP 格式／實際尺寸／bytes；公開 path 由永久 int_id、face ordinal 與 size_key 組成；images 分片描述與 config 的尺寸契約一致。

`unfetched`／`missing` 只有 metadata，沒有變體或假路徑；`pending` 亦無公開變體。
目前只產生官方圖，preview 不能繞過既有資料完整性規則。

M3 可直接選 `purpose=art` 的 `art_s`／`art_m`（上限 160×120／384×288，實際維持
4:3，配方為 integer-4x3-v2），依 producer 的 media 版本組出固定 ID URL，無須再套用前端裁切公式。
詳情／正反面使用 `card_s`／`card_m`／`card_l`；縮圖不可放大原圖，應以變體實際尺寸
為準。缺圖或狀態未核可時維持佔位，不把 metadata 的來源網址當公開資產路徑。
