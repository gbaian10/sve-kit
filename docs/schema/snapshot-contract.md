# 快照機器契約與共用樣本

欄位語意依 [快照格式](snapshot-format.md)，傳輸與版本規則依 [傳輸契約](snapshot-transport.md)。機器資源位於 `carddb/src/sve_carddb/snapshot/schema/v1/contract.schema.json`，隨 carddb wheel 打包；採 JSON Schema Draft 2020-12，識別為 `urn:sve-kit:snapshot:1.0.0`，所有 `$ref` 都在檔內。

此資源釘候選 format `1.0.0`、bucket_count=1；它是可核算的契約配置，不宣稱正式容量凍結。正式配置仍依傳輸契約 §5 量測；更換配置須依其版本規則同步 Schema、樣本與 reader 支援表。

## 資源入口

| `$defs` 名稱 | 用途 |
| --- | --- |
| Manifest、File、Blob、FileRef | 清單、精確檔案依賴與內容定址描述 |
| Config、Types | 設定與固定巢狀 descriptor |
| Container | 40 文字及 3 影像集合的 fragment 容器 |
| 集合名、`集合名_partition` | 完整邏輯 row tuple、各欄位分割 row tuple |
| RegionView、PrintingFace、Section 等 | 25 種巢狀 tuple，含 PrintingFaceBootstrap／Detail |
| ParameterSchema、CorrectionValue | 保留為 JSON 的有限值域；field 與更正值另於所屬 tuple 綁定 |
| Programs、TextAll | 空程式包及文字容器聯集 |
| Changes、Index、IndexPage | 公開變動摘要與永久版本索引形狀 |

可直接以 `#/$defs/Manifest` 等片段作驗證入口。`x-columns`、`x-types`、`x-primary-key`、`x-fragments`、`x-tables` 是供固定 accessor 使用的註記；接受／拒絕 JSON 形狀使用標準 keywords。資料內 types 另以 const 驗完整 descriptor，不能讓 payload 的註記改變解碼方式。

Schema 驗欄序、tuple 長度、required-nullable、額外欄及 enum。跨值比較（例如 minimum≤maximum）、排序、依賴 DAG、hash、引用閉包與 join 由 reader 驗證；不能只通過 JSON Schema 就宣稱整份快照可用。JSON number 在不同語言可能抹去 `1.0` 與 `1` 的差別，canonical bytes 邊界必須在失去原始表示前驗證。

## 共用合成樣本

索引為 `tests/fixtures/snapshot-contract/v1/index.json`。所有內容都是手寫合成資料，不含官方卡文；沒有爬取、建置 DB 或 producer 依賴。`expected-logical.json` 的物件列與 wire tuples 分別撰寫，不從 reader 解碼或 producer 匯出產生預期值。

| 檔案 | 驗收用途 |
| --- | --- |
| manifest.json、payloads/*.json | bootstrap、detail、history、config、images、空 programs；雙面、兩版次與跨片翻譯 |
| expected-logical.json | join 後全部 43 集合；未使用集合為 []，無 row_index／face_ordinal |
| text-all.json | 與個別文字下載得到相同 logical view |
| schema-valid.json | 各集合／巢狀型別與附屬容器的正例；形狀例不要求獨立形成引用閉包 |
| schema-invalid.json | 共用形狀反例；有 raw_json 時先驗原始 JSON bytes 邊界 |
| reader-invalid.json | 依賴 hash、row_index、face_ordinal、descriptor、欄序、引用等反例 |
| vectors.json | canonical 控制字元與 Unicode 排序、固定 SHA-256 分片向量 |

樣本為便於審核的排版 JSON，manifest 的 hash／bytes 指向其 canonical 表示。Harness 先解析排版樣本並 canonical 序列化，交 reader 驗 bytes；不可更新 manifest 來掩蓋未預期差異。實際下載的 payload 直接驗收到的未壓縮 bytes，不先重序列化修復。`raw_json` 反例必須保留原字串，不先 parse/stringify 消除錯誤。

reader-invalid 每例以 target（manifest 或 payload 邏輯鍵）、path（物件鍵／陣列位置序列）、value 指定一次替換。rehash=true 時只更新該 payload 的 File hash／bytes／path、text_all.contains 的同鍵 hash；detail 刪列案例還同步該片 row_counts，以確保錯誤發生在 join。它不修 base/dependencies、不重建 text_all；這些案例使用個別下載入口。rehash=false 保留原封套。所有反例均須拒收，不得補值或 fallback。

## Python 獨立 reader

從 repo 根目錄執行：

```bash
uv --directory carddb run pytest tests/test_snapshot_contract.py
```

`sve_carddb.snapshot.reader.read_snapshot(manifest_value, payloads)` 接受已解析的 manifest 與邏輯鍵→未壓縮 canonical bytes；`read_text_all` 驗聯集後走相同 join。Schema 由套件資源讀取，不在執行期解析 Markdown、不抓網路、不寫版本索引。

這是供契約驗收的小型記憶體 reader：回傳完整 logical objects，便於比對獨立 oracle。前端正式 store 仍須遵守 snapshot-format 的逐片解析／淘汰規則。壓縮傳輸長度、資產本體下載、兩版 changes 差異及建置端證據屬對應下載／發布流程的驗收，不由這個已解壓 bytes 入口代驗。

## TS 端應驗項目

M3 的 TS reader／harness 使用相同資源和樣本，不建立第二份 golden，也不依賴 Python producer。驗收至少包含：

- 載入同一 JSON Schema，逐項接受 schema-valid、拒絕 schema-invalid；固定欄序、每個 tuple 少格／多格、nullable 格不能省略、未知 enum 與額外物件鍵均拒絕。
- 逐項比對固定 types，解碼巢狀 tuple；資料提供的 descriptor 不可重定義型別，缺漏、未知或循環引用不得接受。
- 比對 vectors 的 canonical bytes／hash；拒絕重複 JSON 鍵、浮點表示、不安全整數、Bool 冒充整數、BOM、未配對 surrogate。排序依 Unicode code point，不能直接假定 JS UTF-16 預設排序等價。
- 驗 files 的精確 key/hash、檔案大小、內容定址路徑、依賴 DAG、row_counts、format／最低 reader 版本／capabilities；缺 programs 失敗，非空 programs 失敗。
- 套用 reader-invalid 全部反例；row_index 按已排序 base 定位且完全覆蓋，face_ordinal 取永久 face.ordinal，一對一合併並拒絕缺漏、越界、重複及錯 base。
- 對照 current/history 集合、translation 的欄位分割與合併鍵、公開 ID／詞彙引用閉包；不得跨快照查最新目標補洞。參數上下界與名稱唯一等動態約束另驗。
- 個別下載與 text_all 都恰等於 expected-logical；完整遍歷 43 集合，不忽略空集合、null 或巢狀陣列次序。對替代容器另驗 contains／members／依賴閉包，不能拿聯集逃避單檔 hash 驗證。

CI 路徑分流須讓 `carddb/src/sve_carddb/snapshot/schema/**` 觸發 web 契約測試，讓 `tests/fixtures/snapshot-contract/**` 觸發 Python 與 web 共用契約測試；這些路徑亦供引擎測試接線使用。TS 實作與 CI job 的接線由各元件維護，這份文件只固定共用入口及驗收責任。
