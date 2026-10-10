# 公開快照候選匯出器

匯出器產出 3.0.0，N0 已接線；圖片 URL、版本與新鮮度依 [圖片發布契約](../images/image-variants.md#20-圖片-url版本與新鮮度)
與 [傳輸 §5.4](snapshot-transport.md#54-format-200-卡包-media-與-id-圖片)。預覽配號狀態與快取放在公開根之外，
匯出不寫 current／previous 索引；開發桶的索引由上傳寫入，正式發布仍須另行建置並通過發布閘門。

本文件說明 N0 已接線的 Python producer 邊界；基本目錄容器與新容量帳是 [size-budget](size-budget.md) 的設計要求，現有量測 API 尚未據此重寫；公開欄位與分片語意以
[snapshot-format.md](snapshot-format.md)、[snapshot-transport.md](snapshot-transport.md)
為準，機器契約以套件內 Schema 為準。

## 輸入與輸出

`sve_carddb.export.transport.export_snapshot(projection, ownership, batch, brotli=...)`
接受公開 `Projection`、建置端的版次歸屬與預覽批次。`Ownership.from_database`
透過已驗證 SQLite 邊界讀取永久 `printing.home_set_id`；不能從卡號、商品收錄或
`card.home_set_id` 補猜版次歸屬，也不把此建置欄加入公開列。

`Batch` 只提供 `data_version,published_at,regions`，格式與完整快照清單由 Schema
驗證。此候選匯出器只接受 `preview-` 批次。回傳 `Snapshot` 包含快照清單、按 key
索引的 `Blob`、`text_all` 及建置用壓縮 recipe。寫檔、分配批次與發布由呼叫端負責；
此函式不讀 live manifest、不抓來源、不更新版本索引。

`Blob.raw` 是 canonical-json-v1 bytes；`br` 是可空 bytes，`gzip` 是固定 level 9、
mtime=0、無 filename 的 bytes。沒有提供 Brotli 時，快照清單的 br 長度明示 null，
容量量測也不宣稱 br 閘門通過。Brotli 由呼叫端提供釘住版本、固定 quality 11 的
`Brotli(version, compress)`；版本與參數屬建置 recipe，不放進可重用的資料 payload。
`sve_carddb.export.transport.compression.python_brotli()` 提供使用 `uv.lock` 鎖定的
PyPI `brotli` 的實作，固定 generic mode、quality 11、lgwin 22，recipe 記錄套件版本。
preview CLI 的 `export`／`export-offline` 用 `--brotli` 啟用；預設 `--no-brotli`。
不依賴系統 libbrotli、外部壓縮命令或執行檔 hash。呼叫端須將這份 recipe 與其他建置
依賴一起釘住；新套件仍遵守專案的依賴流程。換 encoder 後 `.br` bytes 與長度可能改變，
快照清單的 `compressed_bytes` 必須反映實際輸出，不能沿用舊長度或清單 hash。

上傳驗證使用同模組的 `verify_brotli(encoded, raw)` bounded 串流解碼逐段比對 raw。
完整流須內容及長度完全相等；無效、截斷、尾隨資料或解壓超出 raw 長度皆拒絕。
原生 `output_buffer_limit` 以 64 KiB 為停止增長的門檻，不代表每段精確為 64 KiB 或
整個 decoder 的記憶體上限；不先一次性配置整份不可信的解壓輸出。
驗證不要求重現 producer 的壓縮 bytes，亦不取代 manifest 的長度／hash／閉包檢查。

上傳讀回匯出根時保留既有 manifest／changes 的 raw、gzip、br 表示，
重驗後直接使用，不呼叫 producer 重壓，因此不需提供 producer codec。遠端不可變物件
比對確切 bytes，不因兩份 `.br` 解壓內容相同就允許換編碼覆寫。

## 固定分片與裝檔

bucket 數只讀取目前候選 Schema 的 `bucket_count.const`，不接受呼叫端任選 N。
分片鍵採 canonical 主實體／完整主鍵陣列與全 SHA-256；永久 ID 不縮短、不重配。
現行加 pending 暫顯的 `display_ref` 決定 revision 的 bootstrap/detail；其餘完整列
只進 history。text 與 translation 按名字、可用名字翻譯及啟動欄位的文字引用閉包分列。

2.0 使用 [傳輸契約 §5.1](snapshot-transport.md#51-format-200-固定配置) 的 N=64、
image 實體鍵與固定相鄰 bucket bands；BP01／CP04 是配置明列的 bootstrap 例外。
同 role／partition／owner 的 band 合檔，不合併邏輯列，不按資料大小動態換 width 或跨 owner 填裝。
同名兩表完整存於 detail，其他啟動欄位依既有閉包保留。
N0 的逐片 raw 512 KiB 預檢由現有量測 API 明示；新契約基本目錄容器 ≤2 MiB、其他資料檔 ≤512 KiB，按整個實際容器量測。超過上限仍可作容量診斷的候選產物，
不能宣稱正式通過、任選 bucket 數、重配 owner 或丟棄資料。base 檔案 bytes/hash
變動時，相關詳情片必須以新 base hash 重建，即使 row_index 本身不變。

printing 與 display revision 的詳情片一對一提供每個 base row，保留 unknown/null
值。printing 用永久 `face.ordinal`，不把陣列位置當 ordinal；revision 的 name 與
非 name translations 各存一次。每片只附實際使用的 nested types 閉包。

## 完整性與容量驗證

匯出完成必須通過交付點 A 的獨立 `read_snapshot`，再與輸入 logical view 逐欄相等。
未知 descriptor、缺 required、重複 PK、錯誤 base/dependency、循環、未閉合的公開引用、
錯誤列位置與非法 pending 呈現都會失敗。`text_all` 保留原 File 分組、member bytes
與 hash，透過同一 reader join，不另出全欄複本。

`Snapshot.assert_identical` 比較兩次輸出的 raw/br/gzip、文字聯集與 recipe；批次時間
及版號只影響快照清單，未變 payload bytes 可跨版重用。

`export.transport.measure.measure` 目前回報 N0 的全文／啟動包 raw／br／gzip、逐片 metadata 與超限檔；它把 manifest／config 計入，排除 images／空 DSL 與替代 text_all 重複量。
既有 `startup_by_region.jp`／`.en` 若仍載全部 bootstrap，兩區數字相同；其中 `target_br_1_mib`／`stop_for_maintainer` 沿用舊啟動目標與停點，不是現行分界。這些數字不是新基本目錄裝檔或單區完整文字帳的通過證據。

修訂後的報告依 [size-budget](size-budget.md)：全量 raw（全部文字分片＋manifest）≤40 MiB，單一版本完整文字閉包 Brotli ≤8 MiB、gzip ≤10 MiB，基本目錄容器 raw ≤2 MiB，其他資料檔 ≤512 KiB。
基本目錄冷載逐區計完整 manifest＋config＋實際 File 與依賴；混區／共用 File 整檔計，不分攤語言比例。
Brotli 2 MiB 是分界，略超報精確 bytes 差額與比例，明顯超出才交維護者，不當作 CI 硬閘門。
source／counterpart、必要跨區檔、卡號索引與字典須完整計入單區全文；基本文字就緒而日文依據／標註未備妥時亦不免計。
text_all 整檔與封套另報，不與分片重加；新量法不以舊豁免或現有 API 數字宣稱已驗收。

逐檔 owner／bucket／partition 報所有 fragments 的集合，不只取第一個。
另報全部 images metadata 的檔數／raw／br／gzip（不加進完整文字）；metadata 與
圖片 blob 分列，當頁小型 row 集合不能冒充實際下載量。背景全量 metadata 的
成本仍算首次 session／離線下載；實際成為首屏或基本目錄必載依賴者加回該冷載帳。

`export.transport.measure.update` 比較不可變 payload bytes，列出變更檔鍵與替換下載量（含新
快照清單）；base hash 變動即使 row_index 不變也會重建詳情片。選用 `text_all` 更新
時應另計整個聯集，不能與個別分片同時計算。

JP 量測不足以凍結正式配置；正式前還須量 EN、雙區與完整三語名字／facet 閉包，
包含零 pending、實際分布、全 pending、最大片與增量重建量。手機的 48/80 MiB
常駐／更新記憶體與解析時間仍按 [size-budget.md](size-budget.md) 獨立驗收。

## 唯讀匯出邊界

`carddb` 的 `sve_carddb.export.read_api` 提供發布端使用的唯讀入口。
`load_export(root)` 驗證 preview 指標、canonical JSON、檔名與內容 hash、完整分片引用、
壓縮旁檔及圖片尺寸／格式，再回傳 `Export`。`members` 包含公開 key、原始 bytes 與
`encoding`；`images` 包含永久 key、版本化 URL、尺寸與 bytes，`Export.image(item)`
每次重新讀取並核對圖片。傳輸 header 由發布端決定。

`read_index(raw)` 驗證 canonical 版本索引；`validate_index(value)` 驗證索引形狀。
GC 使用 `retained_manifest(raw, entry)` 核對保留的 manifest、`closure(path, manifest)`
取得 JSON 引用閉包，及 `current_image_keys(manifest, payloads)` 取得 current 圖片 keys。
`directory(root)` 與 `read_member(root, key)` 保留非 symlink 的本機讀取限制。
這個入口不載入 DB、建置流程、preview writer 或 R2 傳輸，也不在 import 時執行 I/O。

共用契約的唯一權威位於 `sve_carddb.contracts`，快照契約讀取／驗證位於 `contracts.snapshot`，包括 schema 資源、profiles、
欄位 descriptor 與模板參數形狀；build 與 snapshot 均依賴它。
CI 在既有 Python job 中建立 wheel，以獨立環境安裝後檢查資源、reader 與 CLI help，
避免 editable install 掩蓋遺漏的套件資源。
