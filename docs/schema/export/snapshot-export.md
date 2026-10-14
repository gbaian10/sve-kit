# 公開快照候選匯出器

匯出器產出 4.0.0；圖片 URL、版本與新鮮度依 [圖片發布契約](../images/image-variants.md#20-圖片-url版本與新鮮度)
與 [傳輸 §5.4](snapshot-transport.md#54-卡包-media-與-id-圖片)。獨立圖片 producer 的配號狀態與快取放在公開根之外，文字匯出只讀[圖片索引](../images/image-index.md)，
匯出不寫 current／previous 索引；開發桶的索引由上傳寫入，正式發布仍須另行建置並通過發布閘門。

本文件說明目前 4.0.0 的 Python producer 邊界。基本目錄按每包每區裝檔，容量依
[size-budget](size-budget.md) 的單區冷載與一跳完整文字閉包計算；公開欄位與分片語意以
[snapshot-format.md](snapshot-format.md)、[snapshot-transport.md](snapshot-transport.md)
為準，機器契約以套件內 Schema 為準。

## 輸入與輸出

`sve_carddb.export.transport.export_snapshot(projection, ownership, batch, brotli=...)`
接受公開 `Projection`、建置端的版次歸屬與預覽批次。`Ownership.from_database`
透過已驗證 SQLite 邊界讀取永久 `printing.home_set_id`；不能從卡號、商品收錄或
`card.home_set_id` 補猜版次歸屬，也不把此建置欄加入公開列。

`Batch` 只提供 `data_version,published_at,data_as_of,regions`，格式與完整快照清單由 Schema
驗證。此候選匯出器只接受 `preview-` 批次。回傳 `Snapshot` 包含快照清單、按 key
索引的 `Blob` 及建置用壓縮 recipe。寫檔、分配批次與發布由呼叫端負責；
此函式不讀 live manifest、不抓來源、不更新版本索引。

`Blob.raw` 是 canonical-json-v1 bytes；`br` 是可空 bytes。沒有提供 Brotli 時，快照清單的 br 長度明示 null，
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

上傳讀回匯出根時保留既有 manifest／changes 的 raw、br 表示，
重驗後直接使用，不呼叫 producer 重壓，因此不需提供 producer codec。遠端不可變物件
比對確切 bytes，不因兩份 `.br` 解壓內容相同就允許換編碼覆寫。

## 固定分片與裝檔

bucket 數只讀取目前候選 Schema 的 `bucket_count.const`，不接受呼叫端任選 N。
分片鍵採 canonical 主實體／完整主鍵陣列與全 SHA-256；永久 ID 不縮短、不重配。
現行加 pending 暫顯的 `display_ref` 決定 revision 的 bootstrap/detail；其餘完整列
只進 history。名稱所需的 text、translation 與 annotation_set 在 catalog 裝局部副本；
詳情字典仍沿全域固定 band 保存完整列。同 ID 的副本完整值須相等，不能省欄或換用途。
annotation_concept 固定放共用 catalog，並帶所需 label 文字閉包。

4.0.0 使用 [傳輸契約 §5.1](snapshot-transport.md#51-固定配置) 的 N=64，
每個 `(用途,地區,家族[,固定 band])` 一個家族檔。catalog 每包每區一檔；card／face
按區拆列，沒有該區印刷的狀態放 region-state，完整 reader 依固定規則合回。
default printing 的基本列進 catalog，其餘印刷的卡號定位三格按真正 printing home
裝進 catalog，與 default 基本列的聯集恰好覆蓋全部印刷；其完整基本列在背景 print 檔。
商品與引擎資料亦按用途放背景檔，不阻塞基本目錄冷載。

背景一般家族 width=64，BP01／CP04 固定 width=32（兩帶），global width=8（八帶）；
詳情全域字典 width=2（32 帶）。images 保留原 physical 成員：global width=1、
home_set width=32。Python producer 與 reader 共用 contracts 的寬度表，
各自計算分組與驗證；不按資料大小動態換 width，也不跨家族填裝。
同一 File 的相同 fragment 身分按完整 PK 合併排序，先定出 catalog／print base bytes
與 hash，再建立 row_index、exact base 與 dependencies。

producer 按實際 File 預檢 raw 上限：catalog ≤2 MiB，其餘 fragment 容器 ≤512 KiB，
types、字典、metadata 與 rows 全計入，config／programs 另報；超限直接失敗，不能任選 bucket 數、重配 owner
或丟棄資料。base 檔案 bytes/hash 變動時，相關詳情片必須以新 base hash 重建，
即使 row_index 本身不變。

printing 與 display revision 的詳情片一對一提供每個 base row，保留 unknown/null
值。printing 用永久 `face.ordinal`，不把陣列位置當 ordinal；revision 的 name 與
非 name translations 各存一次。每片只附實際使用的 nested types 閉包。

## 完整性與容量驗證

匯出完成必須通過交付點 A 的獨立 `read_snapshot`，再與輸入 logical view 逐欄相等。
未知 descriptor、缺 required、重複 PK、錯誤 base/dependency、循環、未閉合的公開引用、
錯誤列位置與非法 pending 呈現都會失敗。

`Snapshot.assert_identical` 比較兩次輸出的 raw/br 與 recipe；批次時間
及版號只影響快照清單，未變 payload bytes 可跨版重用。

`export.transport.measure.measure` 依 [size-budget](size-budget.md) 回報
`startup_by_region`、`complete_text_by_region`、`text_total`、`manifest` 與逐檔容量。
基本目錄冷載逐區計完整 manifest＋config＋所選區及共用 catalog 的實際 File 與強依賴；
共用檔整檔計，不分攤語言比例。Brotli 2 MiB 是分界，略超報精確 bytes 差額與比例，
明顯超出才交維護者，不當作 CI 硬閘門。

單區完整文字採一跳閉包：C0 是所選區與共用的 bootstrap／text／config 加強依賴，
再加入 C0 直接引用的 source／counterpart owner 所在檔及其呈現所需的卡片／面上下文，
最後補齊聯集的強依賴。來源檔自身的其他來源引用不再遞移展開。
Python 直接使用建置端 `Snapshot.source_files` 計算，不依賴公開 manifest 的 source_refs。
單區完整文字 Brotli ≤8 MiB；兩區全部文字 raw（全部文字分片＋manifest）每次報告、
明顯成長時檢討。基本文字就緒而日文依據／標註未備妥時亦不免計來源資料。

manifest 的 raw／Brotli 獨立列出，亦計入各閉包；無 Brotli 表示時不宣稱壓縮閘門通過。
逐檔報 purpose／region／segment，以及所有 fragments 的 owner／partition 集合與總列數，
不只取第一個；逐檔 raw 上限依 catalog ≤2 MiB、其他 fragment 容器 ≤512 KiB 驗證。

另報全部 images metadata 的檔數／raw／br（不加進完整文字）；metadata 與
圖片 blob 分列，當頁小型 row 集合不能冒充實際下載量。背景全量 metadata 的
成本仍算首次 session／離線下載；實際成為首屏或基本目錄必載依賴者加回該冷載帳。

`export.transport.measure.update` 比較不可變 payload bytes，列出變更檔鍵與替換下載量（含新
快照清單）；base hash 變動即使 row_index 不變也會重建詳情片。

JP 量測不足以凍結正式配置；正式前還須量 EN、雙區與完整三語名字／facet 閉包，
包含零 pending、實際分布、全 pending、最大片與增量重建量。手機的 48/80 MiB
常駐／更新記憶體與解析時間仍按 [size-budget.md](size-budget.md) 獨立驗收。

## 唯讀匯出邊界

`carddb` 的 `sve_carddb.export.read_api` 提供發布端使用的唯讀入口。
`load_export(root)` 驗證 preview 指標、canonical JSON、檔名與內容 hash、完整分片引用、
壓縮旁檔及圖片尺寸／格式，再回傳 `Export`。`members` 包含公開 key、原始 bytes 與
`encoding`；`images` 包含永久 key、版本化 URL、尺寸與 bytes，`load_export(root, image_root=...)` 可指定獨立固定 key 圖片根；`Export.image(item)`
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
