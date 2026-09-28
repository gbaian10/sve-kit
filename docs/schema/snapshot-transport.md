# 卡表快照傳輸契約 v1

本文件補足 [snapshot-format.md](snapshot-format.md) 的 JSON 容器、欄序與版本契約；公開邏輯欄位仍以該文件 §2 為唯一白名單。這些記錄不是新增的玩家集合或建置表。所有物件拒絕未列出的欄位；所有列出的欄位必須存在，`T?` 表示 `T` 或 JSON null，不能省略。空陣列表示已知無成員，來源是否完整另看 coverage。

## 1. 基本型別與 canonical bytes

`ID/Text/Code/UInt/Int/Bool/Date/Instant/Region/Lang/Hash` 沿用 [build-db.md §2](build-db.md#2-共通型別來源與採納政策)。JSON 的 Bool 只能是 true/false。`Hash` 統一為 `sha256:` 加 64 個小寫 hex；內容定址 path 的檔名只取 hex，不含前綴。`Path` 是相對資料根的非空路徑，不含 scheme、前導斜線、反斜線、空段、`.`、`..`、query 或 fragment；不可帶本機路徑。`URL` 是公開 HTTPS URL。

所有 JSON 的序列化規則以 [build-db.md §14 的 canonical-json-v1](build-db.md#14-不可變雜湊僅建置) 為唯一依據。hash 與 bytes 都針對這組未壓縮 bytes。示例為便於閱讀的排版，需 canonical 序列化後才計 hash。

集合以其鍵排序並去重；複合鍵按欄序逐項比較，整數按數值、字串按 Unicode code point；不以本地語言排序。陣列中有業務順序的保留順序：faces 按固定 ordinal、sections 按 ordinal、fallback_order 按優先順序、keyword.actions 按既定 action 順序。tuple 欄序不是 object key 排序。

### 1.1 版本與相容判斷

- `format_version`、`min_reader_version` 為不含 prerelease/build 的 SemVer；後者屬獨立的快照 reader 契約版本，不是 web、Python 套件或 Git tag 版號。reader 明示自身契約版本、支援的 format 範圍及 capabilities。
- 正式 `data_version` 為 `YYYYMMDDTHHMMSSZ-NNNN`，NNNN 是同 UTC 秒內從 0001 起的四位流水號。預覽為 `preview-YYYYMMDDTHHMMSSZ-NNNN`；只在隔離根中分配且不能進正式版本索引。`published_at` 是該批次 UTC Instant；預覽填產出時間，不代表公開發布。
- `required_capabilities` 是排序、唯一的非空 Code 陣列，v1 至少含 `column-partition-v1`、`fragment-container-v1`。reader 需同時滿足 format 範圍、最低契約版本、所有 capabilities，才能啟用；不因 major 相同就接受未知 minor。
- 改 tuple 欄序／型別／nullable、移除欄、改 enum 語義、變動 canonical 規則或 join 語義，升 format major。新增可協商功能／新分片配置以 minor 升版並更新 Schema、共用 golden、reader 能力與最低版本。patch 只修正不改 wire bytes 解釋的規格問題。資料修字、來源新增與新卡只升 data_version。
- 分片 bucket 的算法與數目由每個 format 版本的配置釘死；調整至少升 minor，不能只更換 data_version。`search.grammar_version/normalizer_version` 是搜尋契約的獨立 Code，不宣稱特定搜尋實作已完成；reader 必須明示支援這一對值，未知值停用搜尋並告知，不能用舊 normalizer 建錯索引。

例如 reader 支援 `1.0.0` 且具有上述兩能力，可以讀兩個 data_version 不同的 `1.0.0` 快照。`1.1.0` 改 bucket 數後，尚未宣告支援 `1.1.0` 的 reader 留在最近相容快照；支援該版本者依新 manifest 重建索引。`2.0.0` 改欄序不能沿用 `1.x` accessor。`preview-20260929T010203Z-0001` 即使格式相容，也不能交給正式發布器。

## 2. 快照清單與檔案描述

manifest 是一般 JSON object，不使用 tuple。完整欄位如下；引用皆限定在同一份 manifest，不查最新版本補值。

| 欄位 | 型別與約束 |
| --- | --- |
| format_version, data_version, published_at | §1 的版本、批次、Instant |
| regions, languages | `[Region]`、`[Lang]`，各自非空、排序去重；表示本次收錄範圍，不替缺資料區偽造列 |
| min_reader_version, required_capabilities | §1.1 |
| engine_support_target | `{engine_version:Text?,engine_build_hash:Hash?,validation_policy_id:Code?}`，全 null 或全非 null |
| files | `[File]`，按 key 排序且 key 唯一 |
| config_ref | `FileRef`，恰指向唯一 role=config 的 File |
| text_all | `TextAll?`，null 表示未提供替代下載容器，分片仍完整 |
| partitioning | `{algorithm:"sha256-mod-v1",bucket_count:UInt}`，數目正值且符合此 format 固定配置，見 §5 |
| coverage | §6 的 `Coverage` |
| mechanic_universe_id | Hash；取 keyword 的 id/kind/definition_unit_id/actions，按 id 排序後的 canonical 陣列 |
| restriction_coverage | `[RestrictionCoverage]`，按 profile_id/from_date 排序 |
| source_windows | `[SourceWindow]`，按 kind/region/scope_key/from_date/as_of 排序 |
| qa_card_ids, errata_card_ids | `[ID]`，本次公開 card ID 的排序唯一集合，見 §6 |
| changes_ref | `Blob?`，首版或預覽可 null；正式續版必填且指向 §7 的變動摘要 |

`FileRef = {key:Text,sha256:Hash}`；必須同時吻合 files 中該 key 的 hash。`Blob = {path:Path,sha256:Hash,bytes:UInt,compressed_bytes:{br:UInt?,gzip:UInt?}}`，bytes > 0；壓縮值非 null 時亦 > 0。

`File` 的完整形狀是 `{key,path,sha256,bytes,compressed_bytes,row_counts,dependencies,role}`。其中 Blob 五欄同上，另外：

- `key` 是此快照內的邏輯檔案鍵，不是永久卡片 ID，也不是下載 path。相同檔案鍵在下一快照可以有新 hash。
- `role` 為 `bootstrap/text/config/images/programs`；config 為單一物件，images 為三影像集合，programs 為 DSL 附件，其餘為文字欄位分割容器。
- `row_counts` 是 `[{table:Code,owner:Owner,bucket:UInt,partition:Partition,count:UInt}]`，逐一對應容器 fragment（含零列），count 是該 fragment 的 rows 長度，不是 join 後實體數。config 為 []；programs 也為 []，entries 長度由附件驗證。
- `dependencies` 是按 key 排序的唯一 `[FileRef]`；只表達解析／join 所需的先行檔案，須為 DAG，不包含所有邏輯 FK。所有邏輯 FK 的整體閉包仍由 producer 驗證，不能藉省 dependencies 規避。image 檔依賴 config，detail 至少依賴其 base 檔，bootstrap 不反向依賴 detail。

JSON blob 使用 `snapshots/blobs/<64hex>.json`；br/gzip 分別在同 path 加 `.br`／`.gz`。br 固定品質 11、gzip 固定等級 9，gzip header mtime=0、無檔名；壓縮器及版本由 producer recipe 釘住，兩次 clean export 壓縮 bytes 須相同。null 表示沒有該表示，不能當作零 bytes。下載後先核實壓縮長度，解壓後核 canonical bytes 長度與 sha256；不是拿壓縮內容算 File.sha256。HTTP 自動解壓的 client 要在能觀測傳輸長度的層驗證，不能把已解壓長度拿來比 compressed_bytes。

## 3. config 與巢狀 tuple 型別

config 是 `{format_version,languages,digital_endpoints,shop_links,image_sizes,search,catalog_feedback_url,third_party_image_policy,deck_eligibility_policy}`。除了 format_version，其餘欄位完整形狀如下。config 及 manifest 的內嵌記錄用 object；只有表格容器的巢狀記錄使用 tuple。

| 欄位 | 形狀／排序鍵 |
| --- | --- |
| languages | `[{code:Lang,fallback_order:[Lang],display_name:Text}]`，按 code；fallback 不含自己、不重複、目標存在 |
| digital_endpoints | `[{game:sv1/svwb,card_url_template:Text,language_map:{Lang:Text},status:unknown/available/unavailable,refresh_policy:frozen/on_sve_release}]`，按 game |
| shop_links | `[{id:ID,url_template:Text,parameters:[Code],feature_key:Code,enabled_dev:Bool,enabled_prod:Bool}]`，按 id；parameters 排序去重 |
| image_sizes | `[{key:Code,purpose:card/art,max_width:UInt,max_height:UInt}]`，按 key；五檔數值見 [image-variants.md](image-variants.md) |
| search | `{grammar_version:Code,normalizer_version:Code}`，兩者非空、按 reader 支援表判斷 |
| catalog_feedback_url | URL |
| third_party_image_policy | 固定 `mirror_reviewed` |
| deck_eligibility_policy | 固定 `regional_decklog` |

URL 模板展開後限 HTTPS；shop 參數只允許已列出的具名欄位，v1 為 `card_no/region`，替換值逐個 URL encode。digital 模板只允許 `official_id/provider_lang`，language_map 的 key 是 UI Lang，值是供應者語言碼，缺對應不能猜碼。不得附 token、憑證或任意可執行的轉換式。config_ref 讓文字與影像讀同一物件；不另產獨立 image_sizes 檔。

### 3.1 types descriptor

`types` 是具名 tuple 型別到 descriptor 的 object。descriptor 為 `{columns:[Code],items:[Type]}`，兩陣列等長；Type 恰為下列其中一種：

- `{"scalar":"Text"}`：沿 §1／邏輯欄位的純量型別；允許 ID、Text、Code、UInt、Int、Bool、Date、Instant、Region、Lang、Hash、URL、Path。
- `{"enum":["a","b"]}`：固定字串集合，不由資料新增值。
- `{"ref":"Section"}`：引用同檔 types 的具名 tuple。
- `{"array":{"ref":"Section"}}` 或其他 Type：同型元素陣列。
- `{"nullable":{"scalar":"URL"}}`：明示允許 null。
- `{"json":"CorrectionValue"}`：具名、由 format Schema 固定的 JSON 值，不是任意 JSON escape hatch。

巢狀 Type 可遞迴組合；具名 tuple 引用圖不能有循環。producer 與 reader 都須比對 format 中的完整 descriptor，不能只信資料附的型別。未知名稱／欄序／欄型、缺 descriptor、tuple 長度不符及遞迴引用都拒絕。

例如 `Section`：

```json
{
  "Section": {
    "columns": ["ordinal", "text_unit_id", "kind"],
    "items": [{"scalar":"UInt"}, {"scalar":"ID"}, {"scalar":"Code"}]
  }
}
```

`[0,"t:ja:0123456789abcdef","effect"]` 是該型別的一列。Section.kind 的允許值仍須遵守邏輯欄位約束；descriptor 不放寬 enum。snapshot-format §2 既有具名型別的 columns 一律按 [snapshot-format.md §2](snapshot-format.md#2-公開表完整欄位與玩家用途) 表列順序，`program_ref` 的 tuple 型別名統一為 `ProgramRef`。下列原本匿名記錄也有固定名稱／欄序：

| 出現位置 | 型別名 | columns（依序） |
| --- | --- | --- |
| face.current | Current | region, revision_id, basis |
| art.artists | ArtArtist | artist_id, role |
| PrintingFace.stamps | PrintingStamp | stamp_id, position, color |
| ErrataVersion.changes | ErrataChange | face_id, field, before, after |
| ErrataVersion.printings | ErrataPrinting | printing_id, scope |
| ruling_revision.replaced_scopes | ReplacedScope | scope_unit_id, replacement_revision_id |
| ruling_revision.hints | RulingHint | lang, text_unit_id, parameter_schema |
| rules_profile.revisions | ProfileRevision | id, effective_from, effective_until, cr_version_id, default_copy_limit, construction_rules_ref |
| restriction.members | RestrictionMember | rules_name_id, choice_option, deck_scope |
| keyword.actions | KeywordAction | action, label_unit_id |
| card_engine_support.overrides | SupportOverride | region, support |
| card_engine_support.region_blocks | RegionBlock | region, reasons |

每列的型別、nullable 與 enum 繼承邏輯白名單及建置同名定義。`parameter_schema` 保留 object，遵守其既有參數白名單；`corrected_from`、errata before/after 保留 JSON，是所指 field 的玩家可讀原值／改值，不是 text_unit ID。文字為 Text、數值為 Int?、特性等集合為 `[Text]`；field 必須由 format Schema 明列與對應值型別綁定，不能帶建置端欄位或任意 object。既有 authored 更正的 effect/card_type 兩欄均為來源表記 Text，不套用 type_code enum 改寫原值。DSL ast 完整遵守 `dsl/` Schema，不經 tuple 轉換。

`Correction.source_url` 沒有可公開官方頁 URL 時填 null；`card_related.applicable_regions` 非 reskin 時填 null。所有 tuple 仍佔原位置；空字串、缺欄、少一格不等於 null。

例如以下是手寫的 Correction tuple，不含官方卡文；最後一格不能省略，也不能用卡圖 URL 填補：

```json
["effect", "更正前的合成範例", true, "修正測試文字", null]
```

## 4. fragment 容器與 join

容器完整形狀為 `{format_version,types,tables:{table_name:[Fragment]}}`，同表以陣列容納不同欄位分割／owner／bucket，解決 current/history 的 columns 不同但表名相同的情形。檔中沒有列的集合可省該 table entry；邏輯聯集中不存在的集合視為空陣列，不改為 null。

`Fragment = {owner,bucket,partition,base,columns,rows}`。`Owner = {kind:home_set/global,id:ID?}`；home_set 的 id 是永久 product_family ID，global 的 id 固定 null。`Partition = bootstrap/detail/history`。唯一 fragment 身分是 `(table_name,owner.kind,owner.id,bucket,partition)`，同一 manifest 的所有 files 合計不得重複；檔案如何將 fragments 裝在一起不影響這個身分。每表 fragments 依上述身分排序。

role=bootstrap 只能裝 bootstrap fragments，role=text 只裝 detail/history；role=images 的三個影像集合只用 detail 且無欄位分割。歷史 face_revision 才用 history，其他集合內嵌的歷史沿各自完整列出貨。role 不代替 partition，text_all 仍保留原 File 分組。

`base` 在 printing/detail 與 current face_revision/detail 必填為 `{file:FileRef,table:Code,owner:Owner,bucket:UInt,partition:"bootstrap"}`；其他 fragment 一律 null。base 指精確檔案與 fragment，不允許指自己或跨 manifest。其 FileRef 必列在該 File.dependencies，owner、bucket、table 與 detail 相同。base 存在 payload 裡，故 bootstrap bytes/hash 變更必使相關 detail bytes/hash 變更，即使 row_index 數值恰好不變。

### 4.1 完整欄序

一般集合在其允許的 partition 使用 snapshot-format §2 表列的全部欄位，完全照表列順序；省略建置欄位而非任意省略公開欄位。§3.1 中 text_unit/translation 的分割是按列分割，兩側各保留完整 columns。歷史 face_revision 保留完整邏輯列。只有以下欄位分割使用特別欄序：

| fragment／巢狀型別 | columns（依序） |
| --- | --- |
| printing/bootstrap | id, card_id, region, card_no, card_no_state, catalog_state, listing_confidence, review_level, reference_urls, variant_key, rarity_code, rarity_raw, premium, serial_total, int_id, decklog_available, decklog_verification, decklog_source_url, decklog_checked_on, faces |
| PrintingFaceBootstrap | face_id, art_id, frame_code, signed, embellishment_state, stamps |
| printing/detail | row_index, faces |
| PrintingFaceDetail | face_ordinal, printed_name_unit_id, printed_effect_unit_id, flavor_unit_id, printed_text_state, sections, translations, corrections |
| face_revision/bootstrap | id, face_id, region, name_unit_id, class_code, type_code, cost, attack, defense, traits, titles, special_kinds, translations |
| face_revision/detail | row_index, revision, effective_from, effective_until, temporal_status, change_kind, effect_unit_id, sections, translations, corrections |

printing/bootstrap.faces 使用 PrintingFaceBootstrap，detail.faces 使用 PrintingFaceDetail；依 face.ordinal 排序，face_ordinal 取永久 face.ordinal，不是 arbitrary array index。只有所屬 card 的實際面可出現。face_revision 兩邊 translations 分別只收 field=name 與非 name；其他所有值的型別與邏輯欄位相同。

row_index 為從 0 起的 UInt，指 base fragment 已按 PK 排序的 rows。detail 按 row_index 排序，每個 base row 必須且只能有一列 detail，包含文字未知／空陣列的卡，不能藉缺列改變 unknown 語意。printing detail 每列須恰有與 bootstrap 相同的 faces 集合，從 base 的 face_id 連回 face.ordinal 來定位。join 以 base 還原 id／face_id，不輸出 row_index／face_ordinal；translations 按 `(field,ordinal,target_lang)` 合併並拒絕同鍵重複，合併後依該鍵排序（null ordinal 在數字前）。

`current_ref` 只是從所有 face.current.revision_id 推導的去重集合，不是 manifest 欄位、檔案或第 41 個集合。集合內 revision 只用 bootstrap/detail，集合外才用 history；同一 revision 不可同時兩邊出現。current 切換時舊 revision 進 history、新 revision 進 current，仍保留永久 ID。所有分割 join 後必須恰等於公開邏輯投影：無遺失、無重複欄、無額外列。

例如 base 有依 ID 排序的兩列 revision，detail 的 `[1,...]` 只可指第二列。缺 base hash、hash 指舊片、index=2、重複 index=1、漏 index=0，或 printing 同一 face_ordinal 兩次，都必須拒收，不能 fallback 到最新 bootstrap。

### 4.2 text_all 是容器聯集

`TextAll` 是 Blob 五欄加 `contains:[FileRef]`。contains 精確列出 files 中所有 role=bootstrap/text/config 的鍵與 hash，按 key 排序；不含 images、programs 或自身。分片依賴閉包不能指向此集合外的檔案，圖片／AST 的公開 reference 由 producer 整體驗證，不是文字離線下載的強制依賴。

完整文字包 payload 是 `{format_version,members:[{key,sha256,payload}]}`，按 key 排序。每個 payload 就是該 File 的 JSON 值（config 亦然），重新 canonical 序列化後 bytes/hash 須精確等於原 File；外層 Blob.sha256 驗整個聯集容器。members 與 contains 一對一，不能將 fragments 合併成全欄 rows、重新分配 row_index 或另放重複 config。reader 可從聯集還原與個別下載相同的驗證檔案，再走同一 join。

text_all 是替代表示，不列入 files，不在容量合計重算。文字離線備妥要求整個 contains（未提供 text_all 時按相同規則從 files 推導）及其依賴均驗過；只完成 bootstrap 不算。容量另計 manifest、config、changes 及其 QA／errata 摘要，不因 metadata 移出文字容器而免計；啟動預算包含 manifest、config 與全部 bootstrap。

### 4.3 檔案描述與容器對照例

以下是手寫合成片段，假設示例 format 配置 N=1；不代表正式 bucket 數目已定，也不是完整可發布卡表。未顯示的 manifest/config／引用者需另行補齊。此 text_unit 屬名稱閉包，容器如下（排版不計入 canonical bytes）：

```json
{
  "format_version": "1.0.0",
  "types": {},
  "tables": {
    "text_unit": [
      {
        "owner": {
          "kind": "global",
          "id": null
        },
        "bucket": 0,
        "partition": "bootstrap",
        "base": null,
        "columns": [
          "id",
          "lang",
          "text"
        ],
        "rows": [
          [
            "t:ja:be3847dc77c33905",
            "ja",
            "合成範例"
          ]
        ]
      }
    ]
  }
}
```

對應 files 元素如下，hash 與 bytes 取上述值的 canonical 序列化；未提供壓縮表示時兩格皆 null：

```json
{
  "key": "bootstrap/text/0",
  "path": "snapshots/blobs/47709eb127600386c6319ff78d16753286e7e1c4936a5647c1fe6fda8af98166.json",
  "sha256": "sha256:47709eb127600386c6319ff78d16753286e7e1c4936a5647c1fe6fda8af98166",
  "bytes": 233,
  "compressed_bytes": {
    "br": null,
    "gzip": null
  },
  "row_counts": [
    {
      "table": "text_unit",
      "owner": {
        "kind": "global",
        "id": null
      },
      "bucket": 0,
      "partition": "bootstrap",
      "count": 1
    }
  ],
  "dependencies": [],
  "role": "bootstrap"
}
```

text_all.contains 使用同一 key/sha256，其 members 的 payload 是上述整個容器；不能只抽出 rows 丟掉 fragment 身分。

## 5. owner、bucket 與量測後定版

card、face、face_revision、card_engine_support、mechanic_projection、card_mechanic_coverage、card_related、digital_link、digital_link_coverage、card_voice 跟所屬 card.home_set_id；card_related 用 from_card_id。printing、printing_product、printing_image 跟永久 printing.home_set_id；art、digital_art_link 跟 art.card_id 的 home_set。其餘集合用 global owner；從全球共用的 image_asset/image_variant 到 text_unit、translation、QA、CR 都不跟最近引用者搬家。owner 只用建置資料推導，不因此將 printing.home_set_id 加入公開邏輯欄位。

bucket 使用 `sha256-mod-v1`：分片鍵一律為 JSON 陣列，再依 §1 引用的 canonical-json-v1 取得 bytes。card 系列的鍵為 `[card_id]`，printing 系列為 `[printing_id]`，art 系列為 `[art_id]`；其他集合使用依公開 PK 欄序排列的完整主鍵值陣列，單欄 PK 也保留陣列外層。對這組 bytes 算 SHA-256，全 256 bit 視為無號 big-endian 整數，對 bucket_count 取餘數。所有同主實體欄位分割／所有 revision 共用此鍵；ID 保持字串型別，hash 不截斷。bucket 範圍 `[0,bucket_count)`，空 bucket 不必出檔。owner 分組與 bucket 一起定位，不由檔案下載順序決定。

可核算向量：card ID `c:example` 的鍵是 `["c:example"]`；canonical bytes 長度 13，hex 為 `5b22633a6578616d706c65225d`，SHA-256 為 `6ff93079f7688d35b704b55a2eea460f7d087079d15d238d8980a5d4b0eaea9f`。`bucket_count=4` 時餘數為 **3**（末 byte `0x9f` 對 4 取餘數亦為 3）。producer／reader 的共用 golden 須固定此向量，確保字串鍵的引號與陣列括號都參與 hash。

正式配置凍結前，用候選 N（正整數，依次 1、2、4、8…）量實際 JP 資料，再量 EN 與三語閉包；依 [size-budget.md](size-budget.md) 驗總量、最大分片、bootstrap 大小。選擇通過單片預檢的最小 N，若 bootstrap／總量超標，回到投影與裝檔調整，不能只增加 N 冒稱通過。量測須含 row_index 依賴造成的重建片數與一次增量更新大小。

每一候選配置也須有明示的 format 版本，且 Schema／golden／reader 支援表釘住對應 N；manifest 不得自選同版本的另一個 N。正式發布前以雙區實測（未收錄區域則如實列明）凍結配置；增加資料後若需改 N，升 format minor 並同步契約與 reader，舊快照仍照舊配置可讀。此規則不預先宣稱某個未量測數目足以承載全庫。

## 6. 覆蓋與 QA／errata 摘要

`Coverage = {reviews:[ReviewCoverage],translations:[TranslationCoverage],mechanics:[MechanicCoverage]}`；陣列可空，空不等於已完整查核。

| 型別 | 完整欄位與計數 |
| --- | --- |
| ReviewCoverage | `{entity:printing/art,region:Region,total:UInt,unreviewed:UInt,model_reviewed:UInt,sampled:UInt,confirmed:UInt}`；按 entity/region 排序，四種狀態數合計 total，按出貨永久 ID 去重；art 可在兩個 region 各計一次 |
| TranslationCoverage | `{region:Region,target_lang:Lang,total_fields:UInt,translated_fields:UInt,missing_fields:UInt}`；按 region/target_lang 排序，translated+missing=total；計 current face_revision 的 name/effect 及每個 section 的使用位置，不計歷史或 printing 倍數；target_lang 等於原文語言的組合不列；實際可選 FieldTranslation 才算 translated |
| MechanicCoverage | `{region:Region,scope:Code,total_cards:UInt,any_annotated_cards:UInt,fully_annotated_cards:UInt,unknown_cards:UInt,eligibility:"all_non_retired_cards_in_region",by_keyword:[{keyword_id:ID,complete_cards:UInt}]}`；按 region/scope 排序、keyword 按 ID 排序；沿 snapshot-format §8 的 fresh／EN block／分母規則 |
| SourceWindow | `{kind:errata/qa/cardlist/cr,region:Region,scope_key:Text,from_date:Date,until_date:Date?,as_of:Date,state:complete/partial,source_url:URL}`；scope_key 限 `region:*` 或 `product:<id>` |
| RestrictionCoverage | `{profile_id:ID,from_date:Date,until_date:Date?,state:complete/partial,source_url:URL}`；source_id 投影為公開來源 URL，不出建置 FK |

窗口是 `[from_date,until_date)`；until=null 只表示未另記有限終點，不保證未來來源完整。SourceWindow 查詢還須 date≤as_of；RestrictionCoverage 無獨立 as_of 時最多證明到 manifest.published_at 的 UTC 日期。重疊窗口有 partial／矛盾時不得挑 complete 宣稱完整；producer 應合併無矛盾窗口或隔離衝突。缺窗口、日期／region／scope 不涵蓋皆 unknown。

`qa_card_ids` 從本次出貨的 qa.current_version_id 對應 qa_version.cards 聯集推導；歷史版本的已移除關聯不算現行 QA。`errata_card_ids` 從本次 errata.versions.changes.face_id 及 printings.printing_id 對應的 card 聯集推導；包含已公布但未生效的公告，UI 另讀日期。兩者先驗引用閉包、再排序去重；有 ID 表示 present。沒 ID 只有在相應 kind／region／收錄 scope 的完整來源窗口涵蓋查詢日期時才為 absent，其餘 unknown。摘要不表示裁定適用已人工確認，也不能由一區的 absent 推另一區。

## 7. changes 元素

changes 是一般物件 `{format_version,from_data_version,to_data_version,added,modified,retired,errata,new_qa_versions,identity_changes,coverage_changes,support_changes}`。from_data_version 為 Text?（首版才可 null），to 必須等於 manifest；正式續版指精確前一發布版。所有陣列存在，即使無變更也填 []。不放原始官方卡文或私有稽核資料。

| 欄位 | 元素完整形狀 |
| --- | --- |
| added, modified, retired | `{entity:Code,key:{欄名:主鍵值},changed_fields:[Text],reason:Text}`；entity 限公開 40＋3 集合，key 恰含其 PK 欄且型別相同；changed_fields 是排序唯一的公開頂層欄名，modified 非空，added/retired 為 [] |
| errata | `{errata_id:ID,version_id:ID,card_ids:[ID],reason:Text}`；新公布／改版的 ErrataVersion，不把所有舊公告重列 |
| new_qa_versions | `{qa_id:ID,version_id:ID,card_ids:[ID]}`；包含新增題與同題新 revision |
| identity_changes | `{identity_change_id:ID}`；引用既有公開 identity_change，不另定身分修復事件格式 |
| coverage_changes | `{section:reviews/translations/mechanics/source_windows/restriction_coverage,before:JSON?,after:JSON?,reason:Text}`；JSON 精確為 §6 該 section 的一列，至少一側非 null；兩側都有值時鍵相同，改鍵表示刪舊＋增新 |
| support_changes | `{card_id:ID,region:Region,before_status:Code?,after_status:Code?,before_reasons:[Code],after_reasons:[Code],reason:Text}`；status 限既有五態，null 表示該區卡尚未／不再出貨；reasons 排序去重，缺側為 [] |

added/modified 的 key 必存在新快照；retired 指前版存在而新版不存在的公開列，不代表撤銷永久身分或刪掉永久 registry。陣列按 entity＋canonical key／各元素 ID 排序，coverage_changes 按 section＋其列鍵排序，support_changes 按 card_id/region 排序；不得重複。before／after 的來源版本要與 from／to 一致。QA／errata card_ids 排序去重且須與對應新版本的關聯一致。

support_changes 比較套用 override/block 後的有效狀態；同狀態但 reasons 或 manifest engine_support_target 改變也列出受影響卡區，reason 明示目標變動。changes_ref 為 null 不宣稱「無變更」。ETag／抓取時間改變但公開投影不變不列 modified；coverage 的查核日期變動屬公開投影變動。完整發布閘門仍驗兩版實際差異，摘要不能代替資料閉包驗證。
