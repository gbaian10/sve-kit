# 快照機器契約與共用樣本

欄位語意依 [快照格式](snapshot-format.md)，傳輸與版本規則依 [傳輸契約](snapshot-transport.md)。機器資源位於 `carddb/src/sve_carddb/snapshot/schema/v1/contract.schema.json`，隨 carddb wheel 打包；採 JSON Schema Draft 2020-12，識別為 `urn:sve-kit:snapshot:1.0.0`，所有 `$ref` 都在檔內。

此資源釘候選 format `1.0.0`、bucket_count=1；它是可核算的契約配置，不宣稱正式容量凍結。正式配置仍依傳輸契約 §5 量測；更換配置須依其版本規則同步 Schema、樣本與 reader 支援表。

公開 enum 是固定集合，新增值會讓持有舊 Schema 的 reader 拒收含新值的快照。format `1.0.0` 仍為候選、尚無正式發布快照，候選期間直接修訂 Schema 與 golden；正式凍結後，同類新增值須依傳輸契約 §1.1 升 minor 並提高 `min_reader_version`，同步 reader 支援，不改變既有 enum 值的語義。

**使用者核可 2026-10-01（未核對 EN 繁中）**：FieldTranslation.basis 新增 `shared_jp_unchecked`，tuple 欄序不變。它表示同卡身分／面對應已確認、文字尚未核對，reader 必顯示「日英文字尚未核對」；已知相關 divergence 不可用此值。這是 docs 契約擴充，現有 Schema/golden/reader 尚未同步，不能宣稱現行機器契約已接受新值。首次產出前須在翻譯投影實作中同步三者並驗未核對／已核對切換；候選期依上段修訂，若實作前已正式凍結，則升 minor、提高 min_reader_version 並協商 `unchecked-jp-translation-v1` capability，舊 reader 拒收以免漏標示。

**使用者核可 2026-10-01**：`product.product_type` 的既有 tuple 位置可為 null，與官方商品無 exact confirmed `family.public_code` 型別對應時的 DB 欄位一致；人工 product 輸入仍必填 Code。本候選 format 尚無正式發布快照，直接同步 nullable Schema、type descriptor 與獨立 golden，維持候選 `1.0.0`；正式凍結後的同類相容性變更須另依傳輸契約升版與同步 reader。

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

日期、時間、版本及 HTTPS URL 使用明確的 pattern，不依賴可選的 `format` assertion 或驗證器外掛。數字限定 ASCII；日期驗 Gregorian 閏年與月日，UTC 時間驗 00–23 時、00–59 分秒。URL 驗 URI 字元、percent encoding 與 authority（含 IPv6），Unicode 文字須先編碼為 URI；不得以瀏覽器的容錯正規化代替驗證。

## Schema 產生與維護

`contract.schema.json` 是提交並隨套件發布的產生結果，不直接手改。`schema/v1/source.json` 保存物件定義、tuple 欄位與約束；欄位的 `from` 指向完整邏輯 tuple，讓分割列共用型別與 descriptor。產生器 `sve_carddb.snapshot.generate_schema` 展開固定長度、欄序、Types、fragment 與 changes 的主鍵／欄位白名單；`schema_patterns.py` 組合日期與 URI pattern，來源中的 `pattern: {use: ...}` 只供產生器使用，不會出現在公開 Schema。

修改對應的來源定義或 pattern 後，從 repo 根目錄重產：

```bash
uv --directory carddb run python -m sve_carddb.snapshot.generate_schema
uv --directory carddb run pytest tests/test_snapshot_contract.py
```

`test_schema_regeneration_matches_committed_bytes` 從來源重新產生並逐位元組比對提交版；另以未啟用 format checker 的 Draft 2020-12 驗證器拒絕共用形狀反例。產生器只產生 Schema，不生成 golden 或預期 logical view。欄位變更仍須對照 snapshot-format／snapshot-transport 審核，並依傳輸契約判定版本變動。

## 共用合成樣本

索引為 `tests/fixtures/snapshot-contract/v1/index.json`。所有內容都是手寫合成資料，不含官方卡文；沒有爬取、建置 DB 或 producer 依賴。`expected-logical.json` 的物件列與 wire tuples 分別撰寫，不從 reader 解碼或 producer 匯出產生預期值。

| 檔案 | 驗收用途 |
| --- | --- |
| manifest.json、payloads/*.json | bootstrap、detail、history、config、images、空 programs；雙面、兩版次、跨片翻譯、印刷面 sections／更正、五種 image_variant、text_symbol／多語 hints |
| expected-logical.json | join 後全部 43 集合；未使用集合為 []，無 row_index／face_ordinal |
| text-all.json | 與個別文字下載得到相同 logical view |
| schema-valid.json | 各集合／巢狀型別與附屬容器的正例；形狀例不要求獨立形成引用閉包 |
| schema-invalid.json | 共用形狀反例，含非法日期／時間、非 ASCII 數字與 URI；有 raw_json 時先驗原始 JSON bytes 邊界 |
| reader-invalid.json | 依賴 hash、row_index、face_ordinal、descriptor、欄序、引用、參數域與多語宣告不一致等反例 |
| vectors.json | canonical 控制字元與 Unicode 排序、固定 SHA-256 分片向量 |

樣本為便於審核的排版 JSON，manifest 的 hash／bytes 指向其 canonical 表示。Harness 先解析排版樣本並 canonical 序列化，交 reader 驗 bytes；不可更新 manifest 來掩蓋未預期差異。實際下載的 payload 直接驗收到的未壓縮 bytes，不先重序列化修復。`raw_json` 反例必須保留原字串，不先 parse/stringify 消除錯誤。

reader-invalid 每例以 target（manifest 或 payload 邏輯鍵）、path（物件鍵／陣列位置序列）、value 指定一次替換。rehash=true 時只更新該 payload 的 File hash／bytes／path、text_all.contains 的同鍵 hash；detail 刪列案例還同步該片 row_counts，以確保錯誤發生在 join。它不修 base/dependencies、不重建 text_all；這些案例使用個別下載入口。rehash=false 保留原封套。可選 error 是 Python harness 的錯誤訊息片段，用來確認反例觸及預期檢查；其他 reader 須驗相同失敗原因，不要求相同訊息文字。所有反例均須拒收，不得補值或 fallback。

## Python 獨立 reader

從 repo 根目錄執行：

```bash
uv --directory carddb run pytest tests/test_snapshot_contract.py
```

`sve_carddb.snapshot.reader.read_snapshot(manifest_value, payloads)` 接受已解析的 manifest 與邏輯鍵→未壓縮 canonical bytes；`read_text_all` 驗聯集後走相同 join。Schema 由套件資源讀取，不在執行期解析 Markdown、不抓網路、不寫版本索引。

這是供契約驗收的小型記憶體 reader：回傳完整 logical objects，便於比對獨立 oracle。前端正式 store 仍須遵守 snapshot-format 的逐片解析／淘汰規則。壓縮傳輸長度、資產本體下載、兩版 changes 差異及建置端證據屬對應下載／發布流程的驗收，不由這個已解壓 bytes 入口代驗。

## TS 端應驗項目

M3 的 TS reader／harness 使用相同資源和樣本，不建立第二份 golden，也不依賴 Python producer。驗收至少包含：

- 載入同一 JSON Schema，逐項接受 schema-valid、拒絕 schema-invalid；固定欄序、每個 tuple 少格／多格、nullable 格不能省略、未知 enum 與額外物件鍵均拒絕。日期／URI 不依賴 format 外掛；拒絕非 ASCII 數字、不存在的日期、越界時間與未編碼 URI 字元。
- 逐項比對固定 types，解碼巢狀 tuple；資料提供的 descriptor 不可重定義型別，缺漏、未知或循環引用不得接受。
- 比對 vectors 的 canonical bytes／hash；拒絕重複 JSON 鍵、浮點表示、不安全整數、Bool 冒充整數、BOM、未配對 surrogate。排序依 Unicode code point，不能直接假定 JS UTF-16 預設排序等價。
- 驗 files 的精確 key/hash、檔案大小、內容定址路徑、依賴 DAG、row_counts、format／最低 reader 版本／capabilities；缺 programs 失敗，非空 programs 失敗。
- 套用 reader-invalid 全部反例；row_index 按已排序 base 定位且完全覆蓋，face_ordinal 取永久 face.ordinal，一對一合併並拒絕缺漏、越界、重複及錯 base。
- 對照 current/history 集合、translation 的欄位分割與合併鍵、公開 ID／詞彙引用閉包；不得跨快照查最新目標補洞。另驗參數上下界、名稱唯一及排序，Spelling 必須引用已宣告且啟用對應域的參數，同一裁定的多語 hints 必須有完全相同的參數宣告。
- 個別下載與 text_all 都恰等於 expected-logical；完整遍歷 43 集合，不忽略空集合、null 或巢狀陣列次序。對替代容器另驗 contains／members／依賴閉包，不能拿聯集逃避單檔 hash 驗證。

CI 路徑分流須讓 `carddb/src/sve_carddb/snapshot/schema/**` 觸發 web 契約測試，讓 `tests/fixtures/snapshot-contract/**` 觸發 Python 與 web 共用契約測試；這些路徑亦供引擎測試接線使用。TS 實作與 CI job 的接線由各元件維護，這份文件只固定共用入口及驗收責任。
