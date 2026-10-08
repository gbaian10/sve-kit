# 不可變來源歸檔與凍結輸入

本文件定義建置與萃取的持久輸入；[build-db.md](build-db.md) 的 source_record 是其建置投影，不是歷史來源的唯一保存處。原始來源包含 HTML、PDF、API JSON、卡圖 PNG，以及本批實際使用的其他來源 bytes。公開快照與原始來源各自保留：前者依 [有限保留契約](snapshot-format.md#41-發布窗口圖片新鮮度與回收) 供當前查詢／更新過渡、不承諾歷史回放，後者仍永久保存以供重新解析、查核與重建，不能互相取代。

## 1. 保存邊界

| 資料 | 性質與保留要求 |
| --- | --- |
| 抓取 manifest | 保存抓取、來源連結、discovery generation 與最新資源狀態；不可刪，備份須用 SQLite backup API |
| latest cache | URL 對應的可替換工作副本；不是歷史真值。刪除或替換前，所需舊版本必已在歸檔中且可驗證 |
| 不可變來源歸檔 | 內容定址 raw blobs、來源版本及觀測收據；已引用版本不可覆寫／刪除，官網回寫後無法靠重抓還原 |
| 凍結輸入批次 | 經驗證的 manifest 副本、來源 inventory、歸檔引用與封存收據；封存後不改，失敗的 staging 不供建置 |
| derived、建置 DB、快照候選、WebP 暫存 | 可由釘住的輸入與工具版本重建；清理不能連帶移除來源歷史 |

來源歸檔與版本 inventory 是 manifest 之外第二類不可重建資料，放 repo 外，不進 Git 或公開快照。歸檔保留來源中必要的官方文字，不把測試資料 repo 當來源歸檔，也不把歸檔放進測試 Actions cache／artifact。原 PNG 必須納入凍結來源；公開 WebP 的存在不構成刪除原 PNG 的理由。

## 2. 內容、來源版本與 inventory

以下 JSON 記錄採 [build-db.md §14 的 canonical-json-v1](build-db.md#14-不可變雜湊僅建置)，Hash 為 `sha256:` 加完整 64 小寫 hex；接入既有 manifest 的裸 hex 時只加前綴，不重新定義 hash。raw hash 是 HTTP client 交給 Writer 的 body bytes、解開本機 zstd 後的 exact bytes；不做文字正規化，不把本機壓縮檔 hash、registry-observation-v1 或語義 bundle hash 混用。圖片 hash 是原始圖片檔 bytes，不能改算解碼後像素。

| 記錄／鍵 | 定義 |
| --- | --- |
| raw blob | `raw/sha256/<前兩碼>/<64hex>.raw`，path 中 hash 是原始 bytes；同內容跨 URL／批次只存一次。v1 歸檔直接保存 raw bytes，本機 latest 的 zstd 表示先解壓驗證 |
| source_key | 對 canonical `{provider,kind,url}` 算完整 Hash；provider 使用來源識別 jp/en/sv1/svwb，數位來源不因此成為 SVE region；kind 取抓取 manifest 的 `Resource.kind.value` 原值（`manifest.Kind`，例如 card），不取建置 source_record.kind（例如 official_page）；url 沿抓取 manifest 的 canonical URL，不另猜卡號或去 query |
| source_version_id | `src:v1:<64hex>`；hex 為 canonical `{source_key,raw_sha256}` 的 SHA-256。相同來源同 bytes 重用 ID；同 URL 新 bytes 或不同來源即另有版本 ID，不以本機路徑／抓取順序配號 |
| version descriptor | `{archive_format:1,id,source_key,provider,kind,url,raw_sha256,raw_bytes,first_receipt_id}`；按 source_version_id 保存，不覆寫；first 指首次歸檔收據，不冒稱官網首次發布 |
| observation receipt | `{archive_format:1,source_key,raw_sha256,observed_at,manifest_sha256,resource}`，receipt_id 為此記錄 canonical Hash；resource 保留該 manifest 副本的完整 Resource 欄位（path 為相對路徑），時間未知維持 null，另記觀測不修改舊收據 |

source_key 的可核算合成範例：provider=`jp`、kind=`card`、url=`https://example.invalid/cards/one`，canonical bytes 是下列 73-byte JSON（不含結尾換行），Hash 為 `sha256:f8a7b8029c96e85db449fe62c981aa815f72712559a7132b0142529d97af6535`：

```json
{"kind":"card","provider":"jp","url":"https://example.invalid/cards/one"}
```

observed_at 是本次歸檔觀測時間；Resource 的 first_fetched_at 是 URL 的首次抓取，不能當成每個內容版本的首次抓取。last_changed_at／last_checked_at 各保留本義。HTTP 304、ETag 或檢查時間改變但 bytes 相同，不另配 source_version_id；可追加新收據保留觀測。內容 A→B→A 時，第三次仍用 A 的版本 ID、另有新收據，不抹掉中間 B。

version descriptor、receipt 及完成批次是追加式持久狀態；查詢索引可重建，不能只保留可變的「最新版本」指標。已存在版本沿用其 first_receipt_id，只追加本次觀測收據；相同 ID 的來源身分與 raw bytes 必須相同，碰撞即停止，不覆寫、不自動縮短或重配 ID。首次歸檔前已遺失的舊 bytes 不補造 descriptor。

metadata 依 hash 保存：`receipts/<64hex>.json`、`descriptors/<64hex>.json` 及 `manifests/<64hex>.sqlite`；descriptor 檔名取自身 canonical bytes hash，另由可重建索引對應 source_version_id。跨 store 複製仍核對同一 hash，不能把絕對 pathname 當身分。

### 2.1 凍結批次

批次目錄封存以下檔案，所有 path 都相對批次或具名 archive store，沒有私人絕對路徑：

- `manifest.sqlite`：SQLite backup API 產生且關閉的自足副本，記 bytes、Hash、schema version；不能靠 live 的 `-wal/-shm` 才能讀。
- `inventory.json`：`{input_format:1,created_at,manifest:{path,sha256,bytes,schema_version},scope,current,entries,missing,history_gaps}`。scope 是排序唯一的 `{provider,kind}` 陣列，兩欄沿 source_key 的抓取詞彙；current 為依 URL 排序的 `{url,source_version_id}` 陣列，對應最終 manifest 的有效 Resource。entries 包含 current 及 scope 內已歸檔的歷史版本，按 source_version_id 排序唯一，每筆 `{source_version_id,receipt_id,descriptor_sha256,blob:{store_id,path,sha256,bytes}}`。同 blob 可供不同來源版本引用。
- `missing` 每筆為 `{url,expected_raw_sha256,reason}`，reason 限 `missing_raw/hash_mismatch/unsafe_path/missing_history`；只作診斷。非空批次不得標記封存完成或當完整建置輸入。刻意縮小 preview 範圍須另建明示 scope 的新批次，不能靜默刪列後沿用舊 hash。
- `history_gaps` 使用相同缺失記錄，僅容納首次歸檔前已遺失、且本次建置／已採納資料沒有引用的舊版本；不能用它豁免缺少的必要輸入。批次可如實封存現有閉包，但不能聲稱完整來源歷史。舊版本被 adopted 資料或指定重建引用時，缺失必移入 missing 並阻擋建置。
- `seal.json`：`{input_format:1,inventory_sha256,inventory_bytes}`；批次 ID 為 inventory 的完整 Hash，不含 seal 自身 hash。seal 是成功標記，不用把進度狀態寫回 inventory。

批次 ID **刻意識別封存事件及其完整觀測證據，不是單純的來源內容識別**。created_at 保留於 inventory；兩次獨立封存即使 raw／source_version_id 相同，也可因 created_at、receipt 或 manifest 副本不同而有不同批次 ID。來源內容去重依 raw hash／source_version_id；比較建置是否使用相同內容須另比來源版本集合、current 選用及 authored／工具鎖定輸入，不能只比較批次 ID。重現性驗證釘住同一個已 sealed 的批次重跑，不要求重新封存得到原 ID。

釘住每個 entry 的 descriptor、receipt（含 descriptor.first_receipt_id）與 raw blob 閉包；receipt 可引用準備時或歷史的另一份 manifest 副本，該副本亦須以 hash 留存於歸檔 metadata 閉包。最終 inventory 的 manifest 是提交檢查時的一致副本，不能用準備副本冒充後續發生變更的 live 狀態。依 scope 選出的每個有效 Resource 恰有一筆 current 且其版本存在 entries；歷史 entry 不需冒充最新 Resource。archived／缺資料狀態須顯式診斷，不能因找不到檔案就少算分母。

批次只在同檔案系統 staging 目錄中完成，所有 raw／metadata／DB 及目錄 fsync 後，最後寫 seal 並以不覆寫既有目標的 rename 發布目錄，再 fsync 父目錄。staging 無論已有多少檔都不是 sealed。相同批次 ID 的重用適用於恢復同一份 staging inventory 或重複匯入既有封存：沿用原 created_at／收據，完整比對內容後重用，不覆寫舊完成目錄。實際 archive 根由本機設定的 store_id 對應；搬移磁碟只改設定，不改 inventory/hash。

### 2.2 建置 source_record 的投影

raw 來源的 `source_record.id` 使用 source_version_id，`sha256` 是 raw_sha256，`raw_locator` 是具名 store＋相對內容定址 path；來源 kind 依實際用途映射 official_page/official_api/official_pdf/image 等建置 enum。`url` 沿 descriptor；fetched_at／ETag 等採 first_receipt_id 的已知來源值，後續觀測留收據，不就地修改同一版本。不能把 observed_at 填成未知的原始 fetched_at。

**使用者核可（2026-10-01，F1 方案 A）**：同一 raw 來源版本只投影一列 `source_record`，`parser_version` 一律 null；每個實際使用的 `(source_version_id, usage, parser pin)` 另存於與 DB／report 一起輸出的建置輸入紀錄。不能以不同 parser 或用途配假 raw ID。authored 來源記實際 Git／檔案內容版號；parser_version 按入口為 registry-envelope-v1、product-authored-v1、translation-current-v2 或 catalog-current-v2，不混成爬取 raw，也不假造人工核可。

共用列的 id／url／raw hash 取 descriptor，raw_locator 為具名 store＋相對內容定址 path；fetched_at／ETag／Last-Modified 採 descriptor.first_receipt_id，fetched_at 使用該收據的 last_changed_at 並轉 UTC Z，不以 URL 首次抓取、批次最新 receipt 或 observed_at 替代。官方 JP／EN 卡片 HTML 的 kind 為 official_page，須驗來源身分與 HTML media type；不得按身分／商品用途分成不同 kind。相同 ID 的全部版本 metadata（含 kind、locator、HTTP metadata 及空 parser／authored 欄）逐欄相同才可重用；任一衝突整筆匯入交易回滾，禁止 `INSERT OR IGNORE` 或任取先寫入者。

換 parser 重新產生建置 DB，不改 raw 版本。QA 同官號、CR 同官方版本號但 raw hash 改變仍是不同來源版本；下游 QA／CR revision 另按建置契約追加，不由「官方版號沒改」覆寫歷史。

#### 2.2.1 建置輸入紀錄

模板來源清冊依[清冊契約](template-source-replay.md)每次建置用本次程式及指定來源產生。
建置輸入紀錄為 `input_format: 1`，包含 `context` 與排序唯一的實際 `uses`，採 canonical-json-v1。
它是本次輸入摘要，不作逐欄 expected 使用閉包的驗收證明；不含卡片效果文或私人絕對路徑。

- `context` 保存完整 40 碼 `program_revision` 與明示設定的 canonical JSON 字串 `configuration`。
  authored 讀當前工作樹，revision 只供追蹤；不驗 Git exact bytes、程式 bytes 或依賴 hash。
- 每個 use 為 `{source,usage,locator}`。source 保存來源 metadata、parser_version 與
  `archive:{store_id,batch_id,descriptor_sha256,first_receipt_id}`；source.id 即 source_version_id。
  按完整 canonical bytes 排序去重，同版本不同 parser／用途／batch／定位保留。
- raw 仍從具名 sealed batch 讀取，驗 descriptor、first receipt、metadata 與原始 bytes hash。
  同 raw 版本的 DB source_record 只共用 metadata 完全相同的列；parser_version 為 null。
  來源、文字 hash、語言與 owner 適用性在各入口檢查，DB 保留 STRICT、FK 與 transaction。

保存完成的建置時，直接用 SQLite backup API 複製本次已提交的 DB，與 `inputs.json`、
`report.json` 一起在新暫存目錄寫入及 fsync，再以不覆寫目標的 rename 安裝。
沒有 build seal、第二次填 DB 或 bundle 重播。失敗不發布半套目錄，既有目錄不覆寫。
這是可重建的建置產物；不可重建來源歸檔的 inventory、seal、備份與 restore 驗證規則維持不變。

## 3. 鎖定、準備與封存

### 3.1 沿用 manifest API 的保證範圍

live manifest 與 latest cache 的所有受控入口共用 `Settings.lock_path`，即資料根的 `manifest/.lock`，使用非阻塞 ExclusiveLock。crawl（含 dry-run）、manifest check、manifest backup、extract 均先拿鎖才開 live DB；鎖衝突立即退出，不偷讀或自己清掉鎖檔。flock 的持有者結束後由 OS 釋放，不能把鎖檔中的 PID 當作鎖本身。

`Manifest.open_live()` 以 `mode=ro` 開現有 DB、query_only 並驗 schema version；不建表、不 migration、不切 journal mode。SQLite 讀 live WAL 時仍可能建立／保留 sidecars，不能宣稱檔案系統零寫入。不存在的 manifest 不補造；crawl 首次 dry-run 使用 open_empty 的記憶體資料庫，不建立 live DB。真正的 crawl 寫入才走 Manifest.open 初始化／migration。

`Manifest.backup(dest)` 使用 SQLite backup API，含已提交 WAL，拒絕已存在的 dest，驗 integrity_check，關閉副本後計檔案 hash。API 本身不代拿 ExclusiveLock、不歸檔 raw，也不代表 dest 已原子發布；呼叫方持鎖並把輸出放本批 staging，失敗殘留只視為未完成。不可用 cp live DB 或 immutable=1 讀 live WAL 取代這個流程。

### 3.2 兩段持鎖，bytes 在鎖外準備

1. **第一次持鎖**：以 open_live＋backup 取得準備用副本，列出 scope 的資源及精確 path/hash/raw_bytes/stored_bytes／來源 metadata。對尚無可信歸檔 blob 的來源取得穩定 handle：受控 regular file 的開啟 FD，或同裝置 hardlink／可用的 reflink；檢查 resolved 路徑與 allow-root。記下來源裝置、inode、size、mtime_ns／ctime_ns 及路徑解析結果。這一步不掃描／複製整庫 PNG bytes，handle 數量需有上限，超過就分批準備。
2. **釋鎖準備**：只從釘住的 inode／clone／既有不可變 blob 讀取，做大量 hash、latest zstd 解壓、內容去重與必要複製，產候選 archive blob 與收據。檔案開啟前後身分與大小必須一致、raw hash 必須與準備副本吻合；不重新追 latest pathname。hash 驗證後 fsync 準備檔，保存其穩定身分及 hash 證據。使用 hardlink 的 inode 永不原地改寫。
3. **第二次持鎖**：重新開 live、以 backup 產最終副本。把所有選中資源及 scope／discovery 成員與準備副本比較，重驗 latest 的 resolved 目標／inode／size／mtime_ns／ctime_ns 及 manifest raw hash，也驗準備 blob 未變。自己的 link 操作引起 ctime 變化須在準備收據更新後比較，不能一律忽略 ctime。任何新增／改變／遺失即放棄本次提交，釋鎖後重新準備受影響部分，不能合併新 DB 與舊 raw。
4. **鎖內完成封存**：只安裝已驗證 blob 的內容定址目錄項、登錄 hash／版本／inventory，並完成 DB 副本與 seal 的發布。HTML／PDF／API JSON 也使用同一閉包保障；圖片 bytes 的複製與全檔 hash 掃描不得移到這段鎖內。完成目錄項與 fsync 後才釋鎖，從此建置只讀封存批次。

鎖外準備以「所有 writer 都不原地修改已釘住 inode」為前提；stat 相同本身不是內容未變的密碼學證明。若有非協議 writer、可被外部原地改寫的共享檔案，須先隔離來源或取得獨立 clone 並證明對應版本；無法建立可信穩定輸入即拒絕封存，不拿 stat 捷徑當驗證。批次重試不丟棄已驗證的內容定址 blob；只有未引用的 staging 可另清理。

### 3.3 hardlink、跨裝置與 symlink

- hardlink 僅同裝置可用；建立的是 regular file 的另一個名稱，不能 hardlink symlink 本身。latest 後續必須寫新 inode、fsync、atomic replace，禁止 truncate／原地寫入；chmod hardlink 會影響同 inode 的 latest，不能以此「保護」而破壞 writer。
- reflink 需確認檔案系統支援及獨立 copy-on-write 語意，完成後仍驗 raw hash；跨裝置不能假設 reflink 可行。無法 link／clone 時，從已釘 FD 在鎖外 stream copy 到 archive staging、驗 hash、fsync；禁止把跨裝置 rename 當原子操作。若也無法保持來源穩定或空間不足，停止封存及不安全的 refresh。
- 同裝置 hardlink 是去重手段，不是第二份備份。首輪 PNG 應優先重用已驗 blob／link／clone；也可在 refresh 前只封存將被替換的圖，未替換圖則先釘成不可變 blob 才納入 sealed 批次，不能讓 inventory 指向可替換 latest。
- `--allow-root`／`SVE_EXTRA_ROOTS` 是讀取 symlink 目的地的白名單，不是任意寫入許可；既有 manifest check 支援它，凍結入口須明示接入相同規則，不能假定其他命令已接入。逐次 resolve 必落在資料根或指定根，拒絕絕對 resource.path、`..` 逃逸、越界連結及 dangling link；用開啟 FD 與目錄身分重驗防止檢查後換連結。
- archive store 是明示配置的獨立寫入根，其內新建目錄／檔案不得經未核准 symlink；外部讀取根不自動成為 archive store。inventory 只記 store_id＋相對 path；備份須複製實體閉包，不能只備份指向外接磁碟的 symlink。

## 4. refresh 與中斷恢復

實際配置與啟用前置見[受保護抓取的操作條件](refresh-operation.md)；須完成前置條件並經維護者明示同意才可執行。協議完整接入或程式合併本身不授權對真實資料執行 refresh。

Writer 的檔案替換與 SQLite transaction 不是同一原子交易：既有順序為 temp write→驗證→fsync→replace→目錄 fsync→manifest commit。replace 後、commit 前中斷會成為 untrusted；不能宣稱拿到 ExclusiveLock 就能消除此空隙。

替換協議在這個順序前增加：核對舊 Resource／raw，確認舊 blob、版本 descriptor 及收據已耐久封存；新 bytes 亦先產出內容定址 blob 與候選寫入 metadata，才允許替換 latest／commit 新 Resource；候選不是 observation receipt，正式收據需在 commit 後由吻合的新 manifest 副本產生。大量圖片準備仍按 §3 在鎖外做，最終持鎖重驗舊狀態。無舊內容或 hash 不符時，列入缺失診斷並阻止覆蓋，不能先換新版再嘗試歸檔舊版。

新內容已封存但 manifest 尚未 commit 時，可留下未被批次引用的候選，不能當已生效的來源版本。恢復時以已提交 manifest 與舊／新 hash 檢查 pathname，重新連回已驗證的 blob 或重試寫入；不依 mtime 猜選哪版，不把未完成抓取標成成功。只有協議完整接入的 refresh 才可啟用；單有 backup 或原子 replace 不構成歷史保護。

封存工作不修改既有 manifest 的 resource／fetch_log／generation 狀態；原始抓取失敗不因歸檔變成功。`Resource.archived_at` 是抓取器既有本機狀態，不是本契約的「raw 歷史已安全備份」收據。

## 5. 只讀重建與缺失歷史

模板來源清冊依[清冊契約](template-source-replay.md)每次建置重新產生，不保存也不比對歷史輸出；
每次仍驗本節 sealed/raw 閉包，不借私人審核頁面的免重讀例外略過來源。
只用一個 BuildContext／F1，實際 program_revision、完整程式／lock 如實記當次執行 H；
逐群組保留歷史 producer R、凍結版本／manifest pins、context、預期／實際結果與實際環境差異。
環境值不同不先拒絕也不進語義 root；缺閉包／凍結 bytes 或輸出漂移仍拒絕。
同 raw 一列 source_record，不以 parser／producer 重配 raw ID；同 parser 跨 producer 的用途及 pin 歸屬
保留群組映射，不以最後一筆覆蓋。caller 獨立 expected uses、DB 與 archive pins、F1 四檔 bundle 驗證不變，
自算摘要不能代替獨立閉包，不增父子 executor。

建置／離線 extract 接受 sealed inventory 的 hash，先驗 seal、DB、副本 schema、所有 metadata 與 raw 閉包。只用 `Manifest.open_snapshot()`（`mode=ro&immutable=1`）讀關閉的副本，不跑 DDL／journal pragma；parser／extractor 只能透過 archive locator 讀來源，不能回查 live/latest 補資料。既有 extract 持鎖讀 live 是另一種受控入口；不能把該輸出自動稱為 sealed 重建。

重建固定輸入批次、authored commit／檔案 hash、parser／程式版本與配置。extract 只寫 derived 輸出及報告，不寫 manifest 或 authored；前後 DB／inventory／raw hash 不變。建置 source_record、QA／CR 歷史等每個引用的來源版本須能追回歸檔，不能只留一串 hash。

沒有舊 raw 時，從持久來源引用、manifest fetch_log／generation 及既有 adopted 資料收集可知的 URL／預期 hash，報 `missing_history`；舊 ETag／時間不是 bytes。找回備份後須以 expected hash 驗證才能補回；官網重抓只有 bytes 恰好相同才可補該版本，否則是新版本。從首次可封存觀測起建立歷史，不宣稱已恢復此前所有官方內容；有缺口不得當完整歷史驗收，也不能抹除舊引用來通過。

## 6. 磁碟預算、保留與備份

raw 歷史 logical bytes 為 `Σ(size(hash))`，只加總所有保留版本引用的不同 raw hash；來源個數／URL 數不是 blob 個數。同一 batch／不同 batch／相同內容不同 URL 均去重。每期成長為新出現內容 hash 的 bytes 總和，不是下載流量；304、重新壓縮或只變 ETag 不增加 raw 歷史。

| 量測 | 內容 |
| --- | --- |
| L：latest | 工作副本 logical／allocated bytes；zstd bytes 與 raw bytes 分列 |
| A：archive | 唯一 raw blob logical／allocated bytes、來源版本數、收據與 inventory bytes、DB 副本大小 |
| S：staging | 同時準備／重試／跨裝置 copy 的峰值；不能假設所有來源都能 hardlink |
| B：backup | 獨立磁碟上完整 metadata＋raw 閉包，量首次與增量；hardlink 去重不跨裝置共用 |
| W：衍生檔 | WebP、derived、建置 DB 與公開快照的本機工作量，另算，不能拿來抵扣 raw |

每個裝置的峰值需求為 `allocated_union(L,A)+S+本裝置B+W+保留餘裕`；logical 下界可用唯一 blob 大小估算，不能把 hardlink 多個名稱重複加總。reflink 實際共享及後續 copy-on-write 用檔案系統工具量測，不能只將各檔 st_blocks 相加當共享後大小。獨立備份裝置另算容量，第一次 copy 的全量峰值不可只用後續增量預估。

正式容量報告須列 JP／EN／其他 provider 的 HTML、PDF、API JSON、PNG 數量及大小、unique hash 去重率、hardlink／reflink 命中率、跨裝置 fallback bytes、來源每天／每批變更量、同時 staging 上限及安全餘裕。約 9 GB 的 PNG 是規劃量級，不是配置常數或已量測的本次數字；缺實測欄位標未量，不能先宣稱空間足夠。

### 6.1 保留與恢復驗收

封存的版本 descriptor、receipt、sealed 批次及其 raw／DB 閉包永久保留；不能因 build DB 可重建、來源暫不顯示或官網仍可下載就回收。latest 只有在歸檔／備份可驗回原版本後才可清理。可刪未封存且無引用的 staging／derived；清理以完整引用閉包判斷，不以檔案年齡或最新一批的引用替代。

每次完成歸檔，在獨立裝置備份抓取 manifest 的 backup API 副本及所有新 sealed 批次／來源閉包。備份時釘住要備份的批次 ID 集合；持鎖取得 live DB 副本後立即釋鎖，大量不可變 blob 傳輸在鎖外完成。先複製 bytes 再寫備份完成收據 `{manifest_sha256,input_batch_ids,blob_hashes,verified_at}`；中斷不能標成完整，也不能以此授權清理 latest。私人目的地與排程由維護者配置，不放正式文件。

驗收須在另一個空目錄／裝置恢復，不覆蓋 live：

1. 驗備份完成收據及每個 batch／descriptor／receipt／blob 的 hash；解析 store_id 到新的根，證明沒有偷偷依賴原 symlink 或原資料磁碟。
2. 以 open_snapshot 驗 manifest integrity_check／schema、比對 URL／版本數與 inventory，含一份有已提交 WAL 資料的來源備份案例；恢復副本不需附原 WAL 才能讀。
3. 離線執行固定版本 extract／建置，輸出 hash 與同批次重建一致；前後來源與副本 hash 不變。故意缺一 blob、錯一 hash、越界 symlink 或 schema 不相容時必須失敗，不聯網補最新資料。
4. 演練來源在兩段鎖之間替換、鎖衝突、原子 replace 後尚未 commit、中途磁碟滿及備份中斷；不得發布不一致批次、覆寫已封存版本或清除仍有引用的 blob。

同一次首輪歸檔即須完成此備份／restore 驗收；raw 歷史不是等到整個里程碑結束才備份。restore 報告列已驗批次及缺失，不能以「檔案已複製」代替閉包與 hash 驗證。

## 人工版次的 URL-only 參考邊界

[manual-printings-v1](manual-printings.md#4-來源類別日期與取得方式) 的 third_party_url 只保存第三方店家 URL、人工定位與查核收據，沒有消費第三方 raw，不產 source_version／receipt 或 ArchiveSourceUse。其 printing_reference.source_id 指完整 authored 封套，source_record.sha256 是該分片 exact bytes，不能以 H(URL) 或 canonical 記錄 hash 假裝第三方頁面 raw hash。此例外不適用官方來源、圖像鏡像、人工商品 evidence 或構築證據；已使用官方 raw 仍必驗歸檔完整閉包，完整 authored index／分片仍釘不可變 revision／bytes hash。URL-only 不能宣稱第三方內容可重播或仍為現行。

## 7. 已取得規則原檔的離線登錄

一次性取得、已有 URL／HTTP metadata／時間／內容 hash 的官方 HTML／PDF，依 [構築採納 §2](construction-adoption.md#2-一次性抓回原檔如何正式登錄與釘版) 及 [carddb 離線登錄入口](../../carddb/src/sve_carddb/source_import/README.md) 登錄：原 index／raw 唯讀驗證，在全新隔離 manifest 保存專用匯入收據，不開 live manifest、不造 HTTP fetch_log，再用本契約既有來源版本／inventory／seal 格式封存、備份與 restore-check。已實作的隔離格式使用 PRAGMA user_version=2，inventory 記同版號；v2 完整 schema 獨立凍結，reader 分版本驗完整表／欄位／約束、Resource 與收據閉包，不能只新增表卻仍宣稱版本 1。僅支援版本 1 的舊 reader 必須拒絕版本 2；現行 reader 支援版本 1／2，live writer 仍只寫版本 1 並拒絕版本 2，不自動升 live 或已封存批次。未壓縮來源用實際 raw／stored bytes 與 `.html`／`.pdf` path，不假用 `.zst`；缺 metadata 不補造。

登錄收據由隔離 manifest 的 backup hash 納入 metadata 閉包；不能手工補一個未被釘住的旁檔冒充完成。seal＋獨立備份＋restore-check 後，已引用的 DB 副本／收據／raw 永久保留，工作副本也不自動清理；未引用工作檔的回收須另行核對全部引用與授權。正式批次／政策核對採納完成前，研究樣本或成功抓取 log 均不能成為構築 SourceUse。入口落後公告或沒有個別公告連結，只影響知識覆蓋，不影響原檔的不可變留存；seal 成功不等於 restriction_coverage=complete。

### authored 的來源批次定位

authored 的 `source_ref`、商品／身分證據與 `source_batches` 只保存 `batch_id`／`source_version_id` 等來源定位欄位，不保存 `store_id`。執行端以既有 CLI、環境變數或 ignored 本機設定提供具名 store 與根目錄；解析器在明示配置的 stores 中要求批次恰有一處，找不到或多處皆拒絕，再依原契約驗完整封存閉包及 `entry.blob.store_id` 與設定名稱相等。來源版本不在指定批次時拒絕，不能借用其他批次或 latest cache。

封存 inventory 的 `blob.store_id` 與 batch 的 canonical bytes 雜湊維持不變；搬移磁碟只改本機 root，設定的名稱仍須與既有封存內容一致，不改名或重封。翻譯 inventory ID 的既有雜湊材料仍包含驗證後的歸檔 store 名稱，由執行端重建，不從 authored 讀取舊欄位；既有 inventory ID 與譯文對應因此維持不變。authored 舊 `store_id` 欄位作為未知欄位拒絕，不提供相容讀取。
