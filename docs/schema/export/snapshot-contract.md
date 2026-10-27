# 快照機器契約與共用樣本

套件 Schema、Python reader 與 TS 解碼器使用唯一的 `4.0.0` profile，整合
[公開 annotation 契約](public-annotation.md)的欄序、形狀、來源指標；格式與語義檢查在匯出及上傳前離線執行。
[public-annotation.schema.json](public-annotation.schema.json)及
[固定案例](public-annotation-cases.json)仍供指定邊界的元件驗證，完整快照以套件 Schema 與共用 golden 驗收。
基本目錄裝檔與容量量法依[容量契約](size-budget.md)，不以既有輸出或桌面測試代替手機驗收。

欄位語意依 [快照格式](snapshot-format.md)，傳輸與版本規則依 [傳輸契約](snapshot-transport.md)。
機器資源位於 `carddb/src/sve_carddb/contracts/schema/v3/contract.schema.json`，隨 carddb wheel 打包；
採 JSON Schema Draft 2020-12，識別為 `urn:sve-kit:snapshot:4.0.0`，所有 `$ref` 都在檔內。

此資源釘候選 format `4.0.0`、bucket_count=64；它是可核算的契約配置，不宣稱正式容量凍結。
正式配置仍依傳輸契約 §5 量測；更換配置須同步 Schema、樣本與 reader 支援表。
公開 enum 是固定集合；正式凍結後，新增值須依傳輸契約 §1.1 升 minor 並提高 `min_reader_version`。
候選期間直接同步 Schema 與 golden，不改變既有 enum 值的語義。

有效 JP 繁中依同卡同面及精確來源檢查取完整效果，不以已知 divergence 或日英段落數不同禁止，
亦不按 EN ordinal 拼接。繁中顯示不建立 aligned、官方 counterpart 或自動支援資格。
來源選用依[翻譯契約 §1／§7.2](../domains/translation-contract.md#1-來源與顯示原則)，
讀取閉包依[公開 annotation 契約](public-annotation.md)。

當前 `Translation` 為 `id/source_unit_id/target_lang/text_unit_id/origin/authority/low_confidence/annotation_set_id/annotation_kind` 九欄；
第八格可為 null，第九格 `annotation_kind` 必填，兩者都不能省略。origin、authority 與 low_confidence 各自保留，不允許 scalar coercion。
bootstrap／detail descriptor 與完整 tuple 一致，不輸出 translation.status 或內部決定／清冊。
共用 v3 golden 保留 project/false 名稱與 machine/true 效果；非空 annotation 的原生合成成品另走 C→P→reader 管線，
驗文字、ranges、概念及來源閉包。共用反例涵蓋 tuple 形狀、品質及 annotation 的指定拒絕邊界。

## 資源入口

| `$defs` 名稱 | 用途 |
| --- | --- |
| Manifest、File、Blob、FileRef | 清單、精確檔案依賴與內容定址描述 |
| Config、Types | 設定與固定巢狀 descriptor |
| Container | 43 文字及 3 影像集合的 fragment 容器 |
| 集合名、`集合名_partition` | 完整邏輯 row tuple、各欄位分割 row tuple |
| RegionView、PrintingFace、Section 等 | 固定巢狀 tuple，含 PrintingFaceBootstrap／Detail |
| ParameterSchema、CorrectionValue | 保留為 JSON 的有限值域；field 與更正值另於所屬 tuple 綁定 |
| Programs | 空程式包 |
| Changes、Index、IndexEntry | 變動摘要與 index_format=2 的 current／previous 入口 |

可直接以 `#/$defs/Manifest` 等片段作驗證入口。`x-columns`、`x-types`、`x-primary-key`、`x-fragments`、`x-tables` 是供固定 accessor 使用的註記；接受／拒絕 JSON 形狀使用標準 keywords。資料內 types 另以 const 驗完整 descriptor，不能讓 payload 的註記改變解碼方式。

Schema 驗欄序、tuple 長度、required-nullable、額外欄及 enum。跨值比較（例如 minimum≤maximum）、排序、依賴 DAG、hash、引用閉包與 join 由 Python reader 驗證；不能只通過 JSON Schema 就宣稱整份快照可用。JSON number 在不同語言可能抹去 `1.0` 與 `1` 的差別，canonical bytes 邊界必須在失去原始表示前驗證。

日期、時間、版本及 HTTPS URL 使用明確的 pattern，不依賴可選的 `format` assertion 或驗證器外掛。數字限定 ASCII；日期驗 Gregorian 閏年與月日，UTC 時間驗 00–23 時、00–59 分秒。URL 驗 URI 字元、percent encoding 與 authority（含 IPv6），Unicode 文字須先編碼為 URI；不得以瀏覽器的容錯正規化代替驗證。

## Schema 產生與維護

`contract.schema.json` 是提交並隨套件發布的產生結果，不直接手改。`schema/v3/source.json` 保存物件定義、tuple 欄位與約束；欄位的 `from` 指向完整邏輯 tuple，讓分割列共用型別與 descriptor。產生器 `sve_carddb.contracts.generate_schema` 展開固定長度、欄序、Types、fragment 與 changes 的主鍵／欄位白名單；`schema_patterns.py` 組合日期與 URI pattern，來源中的 `pattern: {use: ...}` 只供產生器使用，不會出現在公開 Schema。

修改對應的來源定義或 pattern 後，從 repo 根目錄重產：

```bash
uv --directory carddb run python -m sve_carddb.contracts.generate_schema
uv --directory carddb run pytest tests/contracts/test_snapshot_contract.py
```

`test_schema_regeneration_matches_committed_bytes` 從來源重新產生並逐位元組比對提交版；另以未啟用 format checker 的 Draft 2020-12 驗證器拒絕共用形狀反例。產生器只產生 Schema，不生成 golden 或預期 logical view。欄位變更仍須對照 snapshot-format／snapshot-transport 審核，並依傳輸契約判定版本變動。

## 格式指紋鎖定

`.github/ci/snapshot-format.lock` 記錄
`format_version → {fingerprint, frozen}`，保留歷史凍結紀錄；目前 `4.0.0` 為未凍結。
`.github/ci/snapshot_format.py` 是正式的本機 hook／CI 檢查，不需私有資料或匯出根。

指紋的來源只有正式定義：

- 重新產生的 `contract.schema.json`，包含 tuple 欄序、型別、nullable、enum、固定 descriptor、主鍵、fragment 配置及准入欄位。提交版須與來源的契約一致。
- `contracts/profiles.py` 的 `PROFILES` 資料值直接序列化，包含版號、資源、bucket 數、文字／影像 band 寬度與家族例外、capabilities、最低 reader 版本。producer 與 reader 仍使用同一份 profile 常數，不另寫一份配置清單。
- 共用合成 golden `tests/fixtures/snapshot-contract/v3/**` 的逐檔內容 SHA-256 與相對檔名，遞迴包含 wire tuples、`expected-logical.json`、`reader-invalid*`、`bucket-cases.json`、canonical 向量及 README；新增、刪除、改名或改內容均受檢查。這些樣本本來就是 Python、Web、Rust 共用的 reader 契約，不另建立第二份案例清單。

Schema 以鍵排序的 JSON 正規化，排除 `$comment`、title、description、examples 註記，並將 enum、required、type 聯集及組合 Schema 當集合處理；tuple／descriptor 陣列保留順序。literal const 及同名公開欄位不當註記刪除。profile 直接轉成 JSON 資料；不解析 Python 原始碼或 AST，因此 reader 區域變數改名、搬檔、效能重構或 Python 升級本身不改變指紋。指紋是上述三項結構的 SHA-256，不讀實際快照資料、套件建置版號或部署狀態。

golden 取 exact bytes 的內容 hash，不移除排版、註解或合成資料值；即使只改其中的 `data_version` 字串也會觸發。這是契約樣本變更，與實際匯出批次換 `data_version` 不同，凍結前須更新鎖定，凍結後依傳輸 §1.1 處理。canonical／join 等語義變動須同步正式契約、共用 golden 及契約測試；未反映在這些來源的語義差異無法由指紋單獨偵測，檢查通過不能取代契約測試完整性的審查。

本機 pre-commit、pre-push 與 CI 的 repo job 都會跑 `snapshot-format-lock`。
CI 用事件的 base commit 比對既有凍結紀錄；本機預設用 `origin/main`，其他基準可用 `--base-ref` 指定，缺少基準會失敗。
檢查程式本身與鎖定檔路徑的變更須由維護者審核，避免修改檢查或搬移路徑繞過凍結保護。
未凍結版本需要更新時，從 repo 根目錄執行並提交鎖定檔 diff：

```bash
uv --directory carddb run python ../.github/ci/snapshot_format.py --update
uv --directory carddb run python ../.github/ci/snapshot_format.py
```

`--update` 只更新未凍結版本；凍結版差異會失敗並提示依[傳輸 §1.1](snapshot-transport.md#11-版本與相容判斷)升版。凍結狀態由維護者明示修改，不會因跑 update 或部署自動改變。

## 共用合成樣本

索引為 `tests/fixtures/snapshot-contract/v3/index.json`。所有內容都是手寫合成資料，不含官方卡文；沒有爬取、建置 DB 或 producer 依賴。`expected-logical.json` 的物件列與 wire tuples 分別DSL，不從 reader 解碼或 producer 匯出產生預期值。

| 檔案 | 驗收用途 |
| --- | --- |
| manifest.json、payloads/*.json | bootstrap、detail、history、config、images、空 programs；雙面、兩版次、跨片翻譯、印刷面 sections／更正、三種 image_variant、text_symbol／多語 hints |
| expected-logical.json | join 後全部 46 集合；未使用集合為 []，無 row_index／face_ordinal |
| schema-valid.json | 各集合／巢狀型別與附屬容器的正例；形狀例不要求獨立形成引用閉包 |
| schema-invalid-core.json、schema-invalid.json | 共用核心與圖片形狀反例，含非法日期／時間、非 ASCII 數字與 URI；有 raw_json 時先驗原始 JSON bytes 邊界 |
| reader-invalid-core.json、reader-invalid.json | 核心與圖片反例：依賴 hash、row_index、face_ordinal、descriptor、欄序、引用、參數域與多語宣告不一致等反例 |
| vectors.json、bucket-cases.json | canonical 控制字元與 Unicode 排序、固定 SHA-256 分片向量 |

樣本為便於審核的排版 JSON，manifest 的 hash／bytes 指向其 canonical 表示。Harness 先解析排版樣本並 canonical 序列化，交 reader 驗 bytes；不可更新 manifest 來掩蓋未預期差異。實際下載的 payload 直接驗收到的未壓縮 bytes，不先重序列化修復。`raw_json` 反例必須保留原字串，不先 parse/stringify 消除錯誤。

reader-invalid 每例以 target（manifest 或 payload 邏輯鍵）、path（物件鍵／陣列位置序列）、value 指定替換；setup 是依序套用的前置變更。核心與圖片反例都定位到 4.0 的實際 fragments，不依賴舊版單檔位置。
測試 harness 重算受影響 File 的 hash／bytes／path／row_counts，並重釘原本有效的 FileRef；故意錯誤的 pin 不修復。manifest 本身的錯誤在重釘後套用，確保觸及該守門。rehash=false 保留目標原封套；這些反例使用個別下載入口。
error 釘住預期失敗原因；其他 reader 不要求相同訊息文字。所有反例均須拒收，不得補值或 fallback。

## Python 獨立 reader

從 repo 根目錄執行：

```bash
uv --directory carddb run pytest tests/contracts/test_snapshot_contract.py
```

`sve_carddb.export.reader.read_snapshot(manifest_value, payloads)` 接受已解析的 manifest 與邏輯鍵→未壓縮 canonical bytes。Schema 由套件資源讀取，不在執行期解析 Markdown、不抓網路、不寫版本索引。

這是供契約驗收的小型記憶體 reader：回傳完整 logical objects，便於比對獨立 oracle。前端正式 store 仍須遵守 snapshot-format 的逐片解析／淘汰規則。壓縮傳輸長度、資產本體下載、兩版 changes 差異及建置端證據屬對應下載／發布流程的驗收，不由這個已解壓 bytes 入口代驗。

## 驗證與瀏覽器讀取責任

| 檢查 | 執行端／入口 |
| --- | --- |
| 嚴格 UTF-8 JSON、重複鍵、浮點／不安全整數、surrogate、canonical bytes | `core/json.py` 的 `parse`／`canonical`，`export/reader.py` 的 `_payload`；`read_api.py` 的 `_manifest` 驗 manifest 原始 bytes |
| Schema、固定 descriptor／tuple、row_counts、檔案與依賴 DAG、引用閉包、join | Python `read_snapshot` 的 `_files`／`_container`／`_base`／`_join`／`_closure` |
| owner／partition／bucket、跨列排序、圖片、數位同名、config、標註 | `export/semantics.py` 的 `validate_fragments`／`validate_view`／`validate_placement`／`validate_config`，`reader_media.py` 與 `reader_annotations.py` |
| 匯出驗收 | `export/transport/__init__.py` 的 `Snapshot.verify` 比對完整 logical view，並驗分片、依賴及 canonical bytes 的 hash 與長度 |
| 上傳前離線驗收 | `read_api.load_export` 驗完整分片及資產；`sve_publish.commands.upload_command` 先取得此結果，再取得憑證與上傳 |
| 瀏覽器下載／快取完整性 | `integrity.ts` 的 `transferDigest` 使用 WebCrypto；client／SearchBlobs／MetadataBytes 驗 hash 與 bytes，decoder 不重算；衍生快取紀錄另驗自身 hash |
| 瀏覽器解碼與准入 | 原生 `JSON.parse`、固定 `x-columns`／`x-types` 解碼及 detail join；保留版本／最低 reader／能力判斷，解析或必要欄位使用失敗顯示載入錯誤 |
| 瀏覽器版本索引核對 | `index-entry.ts` 的 `indexEntries`／`checkEntry`：current／previous 順序、entry 與 manifest 六個准入欄位一致；client 另在工作階段內拒絕 revision 倒退與同 revision 內容改變 |

TS 不再維護完整離線驗證器或只被反例測試呼叫的舊驗證程式；Schema／reader-invalid 反例由 Python 契約測試把關。
Web 測試驗下載 hash 失敗、JSON 無法解析、必要欄位缺失的錯誤狀態，以及正常搜尋、合併、標註與圖片顯示。
canonical 與 SHA-256 bucket 向量仍供 fixture 產製及按需定位驗收；payload 的 types 不改變固定解碼欄序。

共用 schema／樣本變動仍須觸發 Python 契約測試與 Web 解碼／顯示測試。

候選格式的 `card_related`／`digital_link` tuple 不含建置列 ID，以全部公開欄位作複合主鍵。Python／TS accessor 從同一 Schema 取得欄序與主鍵，publish 的獨立合成投影亦須使用此形狀；不得補造 hash 或以列位置還原 ID。建置端先完成審核／來源／數位面 join，再省略 ID 並按完整公開值去重。

## 數位名字政策的准入邊界

名字依獨立規則取詞仍走既有translation／FieldTranslation；same_name瀏覽是另一個需明示能力與reader升級的路徑。
兩份規則與排除清單依[digital-name-policy](../domains/digital-name-policy.md)，不因名字可用就推導真人same_card、coverage或圖／語音。
新relation與機械／真人review的投影須同步producer／reader，未實作不發布、不略去不認識的能力。

## 圖片與有限保留的機器契約

4.0 資源從 source descriptor 生成；共用 golden 位於 v3。
required_capabilities 已固定包含 digital-same-name-links-v1；即使 same_name 列為空，也不得省略能力、相關 Schema 或 reader 驗證。
必要能力必須完整，不能裁掉已宣告能力以通過驗證。

同步項目包括 printing_image 的 media 欄序／ImageDisplayVariant、移除公開 image_variant.path、
固定 N／band width／dependencies、image-id-url-v1 與同名規則能力，以及獨立 index_format=2 的 current／previous。
索引只允許 current／previous 兩筆入口，不產 pages 或每代索引歷史。
Index 的 revision／Entry 欄位、JSON 閉包及 changes 非遞迴規則依 snapshot-format §4.1；
unknown index_format 必拒收，不能把它當成空 pages。

Python／TS 共用合成向量須覆蓋 int_id、f0／f1、JP／EN、手動版次、card／art 分組版本、
pending／missing、尺寸不足、同 image_id 多 printing、回復 bytes 不重用 v。
新增 changes 的 PK／changed_fields 白名單；資料版次 A→B→C 只保 C／B，B.changes.from 不保留 A。
另驗無相容 current／previous、本機 active、落後多版全量更新、下載中輪替導致缺檔後重抓 current。
JSON hash 在瀏覽器下載邊界維持；canonical／FK／base 一致性由匯出與上傳前的 Python reader 驗證；圖片 SHA 由產製／發布端驗，不用歷史圖 hash 拒絕新 bytes。
CDN query 分離已實測，但 browser／SW、新 v 負快取與發布中斷須在相應元件整合驗，不以 Schema 通過代替。
