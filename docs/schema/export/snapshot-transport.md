# 卡表快照傳輸契約（3.0.0，N0 已接線）

本文件補足 [snapshot-format.md](snapshot-format.md) 的 JSON 容器、欄序與版本契約；公開邏輯欄位仍以該文件 §2 為唯一白名單。這些記錄不是新增的玩家集合或建置表。所有物件拒絕未列出的欄位；所有列出的欄位必須存在，`T?` 表示 `T` 或 JSON null，不能省略。空陣列表示已知無成員，來源是否完整另看 coverage。

目前 producer／reader 支援 3.0.0，N0 已交付，圖片契約與配置見 §5.4；不提供 1.x 產出或相容讀取。機器資源依 [snapshot-contract](snapshot-contract.md) 維護。
3.0.0 的變更以[公開 annotation 契約](public-annotation.md)及本文 §8 為準；下列 2.0 歷史欄序不代表新版可接受舊 tuple。

## 1. 基本型別與 canonical bytes

`ID/Text/Code/UInt/Int/Bool/Date/Instant/Region/Lang/Hash` 沿用 [build-db.md §2](../build/build-db.md#2-共通型別來源與採納政策)。JSON 的 Bool 只能是 true/false。`Hash` 統一為 `sha256:` 加 64 個小寫 hex；內容定址 path 的檔名只取 hex，不含前綴。`Path` 是相對資料根的非空路徑，不含 scheme、前導斜線、反斜線、空段、`.`、`..`、query 或 fragment；不可帶本機路徑。`URL` 是公開 HTTPS URL。

所有 JSON 的序列化規則以 [build-db.md §14 的 canonical-json-v1](../build/build-db.md#14-不可變雜湊僅建置) 為唯一依據。hash 與 bytes 都針對這組未壓縮 bytes。示例為便於閱讀的排版，需 canonical 序列化後才計 hash。

集合以其鍵排序並去重；複合鍵按欄序逐項比較，整數按數值、字串按 Unicode code point；不以本地語言排序。陣列中有業務順序的保留順序：faces 按固定 ordinal、sections 按 ordinal、fallback_order 按優先順序、keyword.actions 按既定 action 順序。tuple 欄序不是 object key 排序。

### 1.1 版本與相容判斷

- `format_version`、`min_reader_version` 為不含 prerelease/build 的 SemVer；後者屬獨立的快照 reader 契約版本，不是 web、Python 套件或 Git tag 版號。reader 明示自身契約版本、支援的 format 範圍及 capabilities。
- 正式 `data_version` 為 `YYYYMMDDTHHMMSSZ-NNNN`，NNNN 是同 UTC 秒內從 0001 起的四位流水號。預覽為 `preview-YYYYMMDDTHHMMSSZ-NNNN`；只在隔離根中分配，只能進開發桶的版本索引，不能進正式版本索引。`published_at` 是該批次 UTC Instant；預覽填產出時間，不代表公開發布。
- `required_capabilities` 是排序、唯一的非空 Code 陣列，2.0 的固定集合依 §5.4。reader 需同時滿足 format 範圍、最低契約版本、所有 capabilities，才能啟用；不因 major 相同就接受未知 minor。
- 改 tuple 欄序／型別／nullable、移除欄、改 enum 語義、變動 canonical 規則或 join 語義，升 format major。新增可協商功能／新分片配置以 minor 升版並更新 Schema、共用 golden、reader 能力與最低版本。patch 只修正不改 wire bytes 解釋的規格問題。資料修字、來源新增與新卡只升 data_version。
- 分片 bucket 的算法與數目由每個 format 版本的配置釘死；調整至少升 minor，不能只更換 data_version。`search.grammar_version/normalizer_version` 是搜尋契約的獨立 Code，不宣稱特定搜尋實作已完成；reader 必須明示支援這一對值，未知值停用搜尋並告知，不能用舊 normalizer 建錯索引。

例如 reader 可以讀兩個 data_version 不同的 `2.0.0` 快照，但仍須驗證固定能力集合。
未知格式拒收；更新失敗可保留相容 current／previous 或本機已驗 active。
`preview-20260929T010203Z-0001` 即使格式相容，也不能交給正式發布器。

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
| image_sizes | `[{key:Code,purpose:card/art,max_width:UInt,max_height:UInt}]`，按 key；五檔數值見 [image-variants.md](../images/image-variants.md) |
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

`[0,"t:ja:0123456789abcdef","rule"]` 是該型別的一列。Section.kind 的允許值仍須遵守邏輯欄位約束；descriptor 不放寬 enum。snapshot-format §2 既有具名型別的 columns 一律按 [snapshot-format.md §2](snapshot-format.md#2-公開表完整欄位與玩家用途) 表列順序，`program_ref` 的 tuple 型別名統一為 `ProgramRef`。下列原本匿名記錄也有固定名稱／欄序：

| 出現位置 | 型別名 | columns（依序） |
| --- | --- | --- |
| face.current | Current | region, revision_id, basis |
| face.wording | WordingView | region, state, display, candidates, undated_printing_ids |
| WordingView.display | WordingDisplay | revision_id, basis |
| WordingView.candidates | WordingCandidate | printing_id, revision_id |
| PrintingFace.observations | ObservedText | revision_id, state, source_url |
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

每列的型別、nullable 與 enum 繼承邏輯白名單及建置同名定義。表記未定新增的 WordingView／WordingDisplay／WordingCandidate／ObservedText 是 [snapshot-format §2.3](snapshot-format.md#23-表記未定的公開呈現) 經使用者 2026-10-01 核可的公開呈現擴充，nullable／enum 依該節，不從 face_current 推斷。face.wording 接在 face.current 之後，只收 pending 項；settled region 不出項，全部 settled 時該 tuple 位置仍為空陣列，不能縮短欄序。PrintingFace.observations 接在 printed_text_state 之後（詳情分片同位置）；實作時須同步現有候選 Schema、types、fragment columns、golden 與 reader，本文件不表示機器契約已更新。表格 tuple 中保留的 JSON 值只接受 §3.2 的 `ParameterSchema` 與 §3.3 的 `CorrectionValue`，descriptor 分別用 `{"json":"ParameterSchema"}`／`{"json":"CorrectionValue"}`；不能帶建置端欄位或任意 object。DSL 程式包另依 §3.4，不經 tuple 轉換。

`Correction.source_url` 沒有可公開官方頁 URL 時填 null；`card_related.applicable_regions` 非 reskin 時填 null。所有 tuple 仍佔原位置；空字串、缺欄、少一格不等於 null。

例如以下是手寫的 Correction tuple，不含官方卡文；最後一格不能省略，也不能用卡圖 URL 填補：

```json
["effect", "更正前的合成範例", true, "修正測試文字", null]
```

### 3.2 公開參數宣告

`text_symbol.parameter_schema` 與 `RulingHint.parameter_schema` 共用 `ParameterSchema`，以下是這兩個公開位置的唯一物件定義。它是有限的參數宣告，不是可執行的 JSON Schema、regex 或 DSL；建置端 `sentence_template` 的參數與 card／term 引用不因此取得公開權限。

| 物件 | 完整欄位 | 約束 |
| --- | --- | --- |
| ParameterSchema | `parameters:[Parameter]` | 按 name 排序且唯一；無參數恰為 `{"parameters":[]}` |
| Parameter | `name:Code, uint:UIntRange?, variables:[Text]` | name 符合 `[a-z][a-z0-9_-]*`；uint 與 variables 至少啟用一種值域 |
| UIntRange | `minimum:UInt, maximum:UInt` | 閉區間，`0 ≤ minimum ≤ maximum ≤ 9007199254740991` |

表中所有鍵皆 required，未知鍵拒絕；只有 `uint` 可為 null，陣列及成員不可 null。`uint=null` 禁止整數值；`variables=[]` 禁止變數值。變數白名單固定為字串 `X`，variables 為其排序、唯一子集；不可從資料新增 Y 或其他名稱。純 variable 宣告使用 `uint=null, variables=["X"]`；純 uint 使用非 null 範圍與空 variables；兩者並存時接受兩值域的聯集。所有實際參數都必填、不得多出未宣告名稱，不設 default 或隱式轉型；整數值為 JSON integer，Bool、浮點數、數字字串均拒絕，變數值則為 exact 字串 `"X"`。

`Spelling.parameter_name` 在 parse_kind=uint／variable 時必須指向同一 text_symbol 宣告的 name，且該值域已啟用；literal 時為 null。uint 拼法只解析 ASCII 數字並驗上述界限，不吃字母；variable 拼法只接受宣告的 exact 變數值。字面拼法保留原文以供 roundtrip。`Localization` 的參數引用必須已宣告；同一 ruling_revision 的多語 hints 必須使用相同 ParameterSchema，不能因翻譯擴張值域。宣告只限制文字替換，不賦予遊戲規則或引擎語義。

合法合成例（兩個公開位置均適用）：name=value 接受整數 0 到 9，或字串 X；name 是參數名稱，X 是參數值，兩者不能混淆。

```json
{"parameters":[{"name":"value","uint":{"minimum":0,"maximum":9},"variables":["X"]}]}
```

合法的純 variable 例：

```json
{"parameters":[{"name":"value","uint":null,"variables":["X"]}]}
```

非法的 uint 合成例（minimum 大於 maximum）：

```json
{"parameters":[{"name":"value","uint":{"minimum":2,"maximum":1},"variables":[]}]}
```

非法的 variable 合成例（Y 不在格式白名單）：

```json
{"parameters":[{"name":"value","uint":null,"variables":["Y"]}]}
```

`{}`、省略 uint／variables、兩值域皆停用或 bounds 超出上述範圍也拒絕；無參數只能使用表列的空 parameters 表示。新增參數種類或變數值須依 §1.1 升版，不能靠更換 data_version 放寬。

### 3.3 公開更正值

`Correction.corrected_from` 與 `ErrataChange.before/after` 共用下表的 field 白名單與 `CorrectionValue` 型別對照；before、after 各自按同一 field 驗證。這些值是玩家可讀的來源原值／改值，不是 text_unit ID；field 也不是可任意存取公開欄位的路徑。

| field（固定 enum） | CorrectionValue | 語意 |
| --- | --- | --- |
| `effect`, `name`, `card_type`, `flavor` | Text | 保留來源表記；card_type 不改寫成 type_code enum |
| `cost`, `attack`, `defense` | Int? | 安全整數或 null；不以空字串表示未知 |
| `traits`, `titles`, `special_kinds` | `[Text]` | 來源表記陣列，保留次序；不替換為 vocabulary code／ID，空陣列是已知無成員 |
| `other` | Text | 其他更正的文字描述，不接受 object 或任意 JSON |

除數值列外皆不可 null；Text 可以空字串，陣列成員不可 null。所有值仍遵守 §1 的 canonical 整數界限，禁止 Bool／浮點數冒充 Int。未知 field、數值字串、額外建置物件一律拒絕。`Correction.is_corrected` 固定 true；source_url 的 required-nullable 規則見 §3.1。

例如 `field=cost, before=null, after=3` 合法，after="3" 非法；`field=traits, before=[], after=["合成特性"]` 合法，after=null 非法；`field=other` 的前後值只能是文字。人工更正的 authored 輸入仍受其自身格式白名單限制，公開值域不擴張該輸入格式的可寫欄位。

### 3.4 DSL 程式包與版本准入

DSL 程式包是物件 `{format_version,entries}`；兩鍵皆 required 且不得有額外鍵，format_version 與所屬 manifest 相同，entries 是陣列、不可 null。程式項目的封套為 `{id:ID,dsl_version:Text,ast:JSON}`，三鍵皆 required 且不得有額外鍵；id 在包內唯一並排序，dsl_version 採 `主版.次版`（非負十進位整數，除 0 外無前導零）。ast 保留 JSON，不轉 tuple，其合法形狀只由該 DSL 版本在 `dsl/` 的正式 Schema 定義。

format_version=`2.0.0` 的支援 DSL 版本集合固定為空：唯一可接受的 entries 為 `[]`。任何非空 entries 都拒絕整包，即使封套完整也不放行；不忽略項目、不轉用 astra/1、不使用任意 JSON 的 ast 驗證替代正式 Schema。此規則是版本契約，不因執行環境裝有某個引擎或 Schema 而改變。沒有程式項目可供引用時，非 null ProgramRef 亦無法通過引用閉包驗證。

此格式每份 manifest 必須恰有一個 role=programs 的 File，固定提供 format_version 與 manifest 相同且 entries=[] 的程式包（`{"format_version":"3.0.0","entries":[]}`）；row_counts 與 dependencies 都是 []，仍驗 canonical bytes、長度及 hash。不得以省略檔案表示沒有程式；reader 缺檔即拒收。此附件依 §4.2 不列入 text_all，下載文字分片不依賴它；驗完整快照時另外取得。

啟用正式 DSL 1.0 時須由新的 format 配置至少升 minor，明列支援 DSL 版本到 `dsl/` Schema 資源的映射、所需 capability 與最低 reader 版本，並依 §1.1 協商；reader 使用釘住的權威資源驗 ast，且拒絕未展開的作者巨集。未知 DSL 版本仍拒絕整包，不改寫既有 `2.0.0` 的空集合。

## 4. fragment 容器與 join

容器完整形狀為 `{format_version,types,tables:{table_name:[Fragment]}}`，同表以陣列容納不同欄位分割／owner／bucket，解決 current/history 的 columns 不同但表名相同的情形。檔中沒有列的集合可省該 table entry；邏輯聯集中不存在的集合視為空陣列，不改為 null。

`Fragment = {owner,bucket,partition,base,columns,rows}`。`Owner = {kind:home_set/global,id:ID?}`；home_set 的 id 是永久 product_family ID，global 的 id 固定 null。`Partition = bootstrap/detail/history`。唯一 fragment 身分是 `(table_name,owner.kind,owner.id,bucket,partition)`，同一 manifest 的所有 files 合計不得重複；檔案如何將 fragments 裝在一起不影響這個身分。每表 fragments 依上述身分排序。

role=bootstrap 只能裝 bootstrap fragments，role=text 只裝 detail/history；role=images 的三個影像集合只用 detail 且無欄位分割。display_ref 外的 face_revision（含其餘候選）才用 history，其他集合內嵌的歷史沿各自完整列出貨。role 不代替 partition，text_all 仍保留原 File 分組。

`base` 在 printing/detail 與 現行／暫顯 face_revision/detail 必填為 `{file:FileRef,table:Code,owner:Owner,bucket:UInt,partition:"bootstrap"}`；其他 fragment 一律 null。base 指精確檔案與 fragment，不允許指自己或跨 manifest。其 FileRef 必列在該 File.dependencies，owner、bucket、table 與 detail 相同。base 存在 payload 裡，故 bootstrap bytes/hash 變更必使相關 detail bytes/hash 變更，即使 row_index 數值恰好不變。

### 4.1 完整欄序

一般集合在其允許的 partition 使用 snapshot-format §2 表列的全部欄位，完全照表列順序；省略建置欄位而非任意省略公開欄位。§3.1 中 text_unit/translation 的分割是按列分割，兩側各保留完整 columns。display_ref 外的 face_revision 在 history 保留完整邏輯列。只有以下欄位分割使用特別欄序：

| fragment／巢狀型別 | columns（依序） |
| --- | --- |
| printing/bootstrap | id, card_id, region, card_no, card_no_state, catalog_state, listing_confidence, review_level, reference_urls, variant_key, rarity_code, rarity_raw, premium, serial_total, int_id, decklog_available, decklog_verification, decklog_source_url, decklog_checked_on, faces |
| PrintingFaceBootstrap | face_id, art_id, frame_code, signed, embellishment_state, stamps |
| printing/detail | row_index, faces |
| PrintingFaceDetail | face_ordinal, printed_name_unit_id, printed_effect_unit_id, flavor_unit_id, printed_text_state, observations, sections, translations, corrections |
| face_revision/bootstrap | id, face_id, region, name_unit_id, class_code, type_code, cost, attack, defense, traits, titles, special_kinds, translations |
| face_revision/detail | row_index, revision, effective_from, effective_until, temporal_status, change_kind, effect_unit_id, sections, translations, corrections |

printing/bootstrap.faces 使用 PrintingFaceBootstrap，detail.faces 使用 PrintingFaceDetail；依 face.ordinal 排序，face_ordinal 取永久 face.ordinal，不是 arbitrary array index。只有所屬 card 的實際面可出現。face_revision 兩邊 translations 分別只收 field=name 與非 name；其他所有值的型別與邏輯欄位相同。

row_index 為從 0 起的 UInt，指 base fragment 已按 PK 排序的 rows。detail 按 row_index 排序，每個 base row 必須且只能有一列 detail，包含文字未知／空陣列的卡，不能藉缺列改變 unknown 語意。printing detail 每列須恰有與 bootstrap 相同的 faces 集合，從 base 的 face_id 連回 face.ordinal 來定位。join 以 base 還原 id／face_id，不輸出 row_index／face_ordinal；translations 按 `(field,ordinal,target_lang)` 合併並拒絕同鍵重複，合併後依該鍵排序（null ordinal 在數字前）。

`current_ref` 只是從所有 face.current.revision_id 推導的去重集合；`display_ref` 依 snapshot-format §2.3 為 current_ref 加 pending wording.display 非 null revision ID 的聯集，不加入其餘 candidates。兩者都不是 manifest 欄位、檔案或額外公開集合。display_ref 內只用 bootstrap/detail，每個 pending face-region 最多一筆暫顯的輕量欄位、名稱與可用名稱翻譯閉包進 bootstrap；集合外（含其餘候選）用 history 完整列，role=text、base=null，按需載入。history 不表示年代，其餘候選引用不形成 bootstrap 對 history 的強制下載依賴。候選與現行／暫顯共用 revision 時只沿用其唯一儲存；同一 revision 不可同時兩邊出現。current 或 display 改變時重算分割，永久 ID 不變；所有分割 join 後必須恰等於公開邏輯投影，無遺失、無重複欄、無額外列。須量測日版／英版各自的 manifest、config 與首屏實際必載 bootstrap（含暫顯輕量投影、名稱／可用名稱翻譯閉包及稀疏 wording 引用），共用／混區檔整檔計入；Brotli 約 1 MiB 為目標、2 MiB 可接受，更大停下交維護者決定，詳 [size-budget.md](size-budget.md)。另列其餘按需候選片容量；未載候選的索引與搜尋進度依 snapshot-format §2.3 標示。

例如 base 有依 ID 排序的兩列 revision，detail 的 `[1,...]` 只可指第二列。缺 base hash、hash 指舊片、index=2、重複 index=1、漏 index=0，或 printing 同一 face_ordinal 兩次，都必須拒收，不能 fallback 到最新 bootstrap。

### 4.2 text_all 是容器聯集

`TextAll` 是 Blob 五欄加 `contains:[FileRef]`。contains 精確列出 files 中所有 role=bootstrap/text/config 的鍵與 hash，按 key 排序；不含 images、programs 或自身。分片依賴閉包不能指向此集合外的檔案，圖片／AST 的公開 reference 由 producer 整體驗證，不是文字離線下載的強制依賴。

完整文字包 payload 是 `{format_version,members:[{key,sha256,payload}]}`，按 key 排序。每個 payload 就是該 File 的 JSON 值（config 亦然），重新 canonical 序列化後 bytes/hash 須精確等於原 File；外層 Blob.sha256 驗整個聯集容器。members 與 contains 一對一，不能將 fragments 合併成全欄 rows、重新分配 row_index 或另放重複 config。reader 可從聯集還原與個別下載相同的驗證檔案，再走同一 join。

text_all 是替代表示，不列入 files，不在容量合計重算。文字離線備妥要求整個 contains（未提供 text_all 時按相同規則從 files 推導）及其依賴均驗過；只完成 bootstrap 不算。容量另計 manifest、config、changes 及其 QA／errata 摘要，不因 metadata 移出文字容器而免計；啟動量依所選日版／英版的實際必載閉包各自以 Brotli 計算；若 reader 仍載全部 bootstrap，兩區均完整計入，raw／gzip 另報，不用日英合計作啟動門檻。

### 4.3 檔案描述與容器對照例

以下是手寫合成片段，使用 2.0 固定 N=64；不是完整可發布卡表。未顯示的 manifest/config／引用者需另行補齊。此 text_unit 屬名稱閉包，容器如下（排版不計入 canonical bytes）：

```json
{
  "format_version": "2.0.0",
  "types": {},
  "tables": {
    "text_unit": [
      {
        "owner": {
          "kind": "global",
          "id": null
        },
        "bucket": 23,
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
  "key": "bootstrap/bootstrap/global/global/band/2",
  "path": "snapshots/blobs/099e7804e341d8ad5cff67c3addc5f90ee5843fb7174b02c7718156193d288e4.json",
  "sha256": "sha256:099e7804e341d8ad5cff67c3addc5f90ee5843fb7174b02c7718156193d288e4",
  "bytes": 234,
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
      "bucket": 23,
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

bucket 使用 `sha256-mod-v1`：分片鍵一律為 JSON 陣列，再依 §1 引用的 canonical-json-v1 取得 bytes。card 系列的鍵為 `[card_id]`，printing 系列為 `[printing_id]`，art 系列為 `[art_id]`；image_asset／image_variant 使用 `[id]`／`[image_id]` 共用實體 bucket；其他集合使用依公開 PK 欄序排列的完整主鍵值陣列，單欄 PK 也保留陣列外層。對這組 bytes 算 SHA-256，全 256 bit 視為無號 big-endian 整數，對 bucket_count 取餘數。所有同主實體欄位分割／所有 revision 共用此鍵；ID 保持字串型別，hash 不截斷。bucket 範圍 `[0,bucket_count)`，空 bucket 不必出檔。owner 分組與 bucket 一起定位，不由檔案下載順序決定。

可核算向量：card ID `c:example` 的鍵是 `["c:example"]`；canonical bytes 長度 13，hex 為 `5b22633a6578616d706c65225d`，SHA-256 為 `6ff93079f7688d35b704b55a2eea460f7d087079d15d238d8980a5d4b0eaea9f`。`bucket_count=4` 時餘數為 **3**（末 byte `0x9f` 對 4 取餘數亦為 3）。producer／reader 的共用 golden 須固定此向量，確保字串鍵的引號與陣列括號都參與 hash。

正式配置凍結前，用候選 N（正整數，依次 1、2、4、8…）量實際 JP 資料，再量 EN 與三語閉包；依 [size-budget.md](size-budget.md) 驗總量、最大分片、bootstrap 大小。選擇通過單片預檢的最小 N，若完整文字超標，回到投影與裝檔調整；啟動量依各版本 Brotli 目標與維護者決定停點處理，不能只增加 N 冒稱通過。量測須含 row_index 依賴造成的重建片數與一次增量更新大小。

每一候選配置也須有明示的 format 版本，且 Schema／golden／reader 支援表釘住對應 N；manifest 不得自選同版本的另一個 N。正式發布前以雙區實測（未收錄區域則如實列明）凍結配置；增加資料後若需改 N，升 format minor 並同步契約與 reader，保留窗口內舊快照仍按原配置解讀，不承諾永久重新下載。此規則不預先宣稱某個未量測數目足以承載全庫。

### 5.1 format 2.0.0 固定配置

2.0 固定 N=64、min_reader_version=2.0.0，完整能力集合依 §5.4。
producer／reader 須明示支援此配置，不放寬 bucket 範圍，也不保留 1.x accessor。

image_asset 使用 `[id]`，image_variant 使用 `[image_id]`，兩者仍是 global owner、detail partition，同 image 的所有 variant 同 bucket。printing_image 沿永久 printing.home_set_id 與 `[printing_id]`；不依卡號或 card.home_set_id 猜歸屬。rules_name／face_rules_name 的完整列唯一存於 global detail，其他集合仍用 §5 原本的主實體／完整 PK 鍵與欄位分割。

實際裝檔按 `(role,partition,owner.kind,owner.id,band)`，`band = bucket // width`。同一固定群組的非空 fragments 合為一個 File，不跨 owner／partition 填裝，不按每版資料大小重新 greedy packing。File key 為 `<role>/<partition>/<owner.kind>/<encoded-owner-or-global>/band/<band>`，owner ID 按 UTF-8 percent encode（僅保留 RFC 3986 unreserved 字元），global 使用字面 `global`；key 不等於下載 path。下列 widths 屬格式配置，不是呼叫端參數：

| role／partition／owner | width |
| --- | ---: |
| images／detail／global | 1 |
| images／detail／home_set | 32 |
| bootstrap／bootstrap／global | 8 |
| bootstrap／bootstrap／home_set | 64；BP01、CP04 固定為 32 |
| text／detail／global | 2 |
| text／detail／home_set | 32 |
| text／history／global 或 home_set | 32 |

config 與 programs 各一檔，key 分別為 `config`、`programs`。無列 bucket 可不出 fragment／檔案；相同 fragment 身分仍全庫唯一。每個資料 File 的 canonical raw（含 types 閉包與所有 fragments）≤512 KiB；manifest 與 text_all 另計。新增內容超標時停止驗收，另提至少 minor 的固定配置修訂，不能在同版自動加例外、搬 owner、縮 width 或丟列。BP01／CP04 是釘住的永久 family ID，不是從卡號前綴作判斷。

先排定每個邏輯 fragment 的 PK 與 rows，裝 bootstrap 並取得實際 File hash，再重建 printing／display revision 的 row_index、base FileRef 與精確 dependencies；一個 detail File 可以依賴多個 bootstrap File，但每個 base 仍精確指相同 table／owner／bucket。types 只收必要且完整的引用閉包。完整文字包的 contains／members 仍是 config、bootstrap、text 的原 File 聯集，不產生另一份全欄卡表。單張 image metadata 更新只改其 global bucket 與 manifest；若 binding 未變，其他 image bucket 不因 hash 排序或插入重排。

### 5.2 影像 metadata 的背景預取與逐頁解析

2.0 reader 依 §5.4 只取可見面的卡包 media，圖片 blob 按可見面下載；全域來源詳情按需另取。必列實際 session／離線的 raw／br／gzip、檔數與 cache footprint。任何 metadata 若實際阻擋首屏，均加回 startup_by_region；未下載完整集合時不宣稱已完整離線。

先驗 File bytes／hash，再按 manifest hash 與內容 hash 隔離保存 CacheStorage bytes；按頁解析仍驗 canonical／Schema／語意，不把已驗 bytes 當已解析資料。當頁取 printing_image 的狀態、尺寸與版本即可組 URL，不必另取 global asset／variant。同 File 的多 locator 去重，當頁優先於背景，整體最多 4 個 in-flight。切快照取消舊工作，舊回應不寫入新狀態；失敗／重試與尚未完整的進度明示，不標離線備妥。

解析片的調度上限為 12 MiB raw 對應量與 64 檔，當頁 pin 到畫面移除；解析時只留下當頁所需 image 列，立即丟掉其他列與整片解碼物件，不建立全庫資產／variant Map。上一頁解除 pin 後即釋放不再使用的 image 列；列快取也須有界，連同 view／Map／暫存與 pinned 工作集量 JS heap；raw 調度量不能代替 48／80 MiB 手機驗收。已驗 bytes 可留在持久快取，換頁後重解析不需外部重抓；CacheStorage 不可用／quota 失敗／被清除時明示退化及實際成本。

冷頁成本以前置啟動包已驗證並快取、尚無 images metadata 的狀態計算，config 依賴已在啟動量計入。每次配置定版須用至多 24 張可見面圖的頁面量冷頁 P50／P95／max：同 printing owner ≤25 個 metadata File、raw≤9 MiB、br≤2 MiB；混 owner ≤48 檔、raw≤12 MiB、br≤2.5 MiB。雙面若同時展示兩圖就算兩張圖，超過 24 圖的頁面另外量當頁 pin，不沿用此上界。同頁重繪、P1→P2→P1 及解析 LRU 淘汰後回頁，在已驗 byte cache 未被清除時，metadata 外部請求與傳輸 bytes 為 0，另報 cache 讀取／重解析成本。假 fetch／CacheStorage 接線與真實 heap 仍須獨立測，不用數值模擬冒稱瀏覽器驗收。

### 5.4 format 2.0.0 卡包 media 與 ID 圖片

本節記錄切換前 2.0 配置；3.0 以 §8 覆寫變更欄序與版本。format_version、min_reader_version 均為 `2.0.0`。2.0 公開欄序依既有套件 Schema，canonical、owner、分片鍵及文字裝檔依本文件前述配置。translation 固定七欄的最後一欄為必填 Bool `low_confidence`，origin 為 `official/project/machine`；bootstrap／detail 使用相同完整七欄，FieldTranslation.basis 同步接受 `shared_jp_unchecked`。reader 依固定 Schema 拒絕舊 status 字串、舊 origin、缺欄與非 Bool，不兼容兩種七欄形狀。

2.0 尚未正式首發，本候選契約同步 producer、Python／TS reader、共用 golden 與 Web 合成快照，不增加能力旗標；正式發布後同類破壞性欄型更動須升 major。required_capabilities 恰為排序的：

```json
[
  "column-partition-v1",
  "digital-same-name-links-v1",
  "fragment-container-v1",
  "image-entity-buckets-v1",
  "image-id-url-v1",
  "rules-name-on-demand-v1"
]
```

所有容器、config、programs、text_all、changes 的 format_version 都必須一致；空圖片／same_name 也不省能力。
索引以獨立 `index_format:2` 協商，形狀依 [snapshot-format §4.1](snapshot-format.md#41-發布窗口圖片新鮮度與回收)，不混同於 reader 契約版號。
producer 與 reader 須各自驗 same_name 的卡層 null、effect_similarity=null、review_level=unreviewed
及完整公開引用閉包，不只驗 enum 合法。config.digital_endpoints 依 game 排序，
2.0固定 sv1／svwb 兩列；模板、language_map 沿 §3，
未做連線健康檢查時 status=unknown，refresh_policy 分別為 frozen／on_sve_release。
每個公開 digital_card.game 都須有可用來解析卡片頁 URL 的 endpoint；
缺 UI 語言對照不能猜 provider_lang，狀態 unknown 不得顯示為已確認可用。

N 固定為 64，沿用 sha256-mod-v1。printing_image 使用永久 printing.home_set_id、鍵 `[printing_id]`，
role=images、partition=detail、base=null，columns 為 snapshot-format §2.1 的完整 2.0 欄序。
此卡包投影稱 media，**不是新增 role 或 partition enum**；home_set 的 images band width 固定 32。
image_asset／image_variant 仍在 global detail、width=1，以 `[id]`／`[image_id]` 共置；其餘 widths 沿 §5.1。
File key 仍用 §5.1 配方，不新增另一套手寫 URL 對照庫。

media File.dependencies 恰為 config 加上該檔各 printing_id／face_id 所需的 printing、face bootstrap FileRef 聯集，按 key 排序去重；
不依賴 global images 詳情，不用 row_index。全域影像詳情依賴 config；所有公開 FK 仍由 producer 整體驗證。
printing.home_set_id 是建置所有權，不出貨新 printing 欄位；reader 由已驗 printing bootstrap fragment 的 owner 定位相同 home_set media。
圖片狀態只放在 media 列；局部列的尺寸是明示顯示投影，producer 驗其與 image_variant 一致，不新增第二資料來源。
`ImageDisplayVariant` descriptor 固定為 `[Code,UInt,UInt]`，各 size_key 限 config 五檔且尺寸 >0；
version 為 1..2^53−1 或 null，狀態／空陣列約束依 snapshot-format §2.1。

首屏只取可見面的 media，拿到該列即由 int_id／face.ordinal／size／version 組 URL；
來源詳情按使用者需要再載，不全量預取 global images，亦不建立全庫影像 Map。
最多四個 metadata in-flight，有界 byte／row LRU；沿 §5.2 的工作集上限與手機量測要求。
前一頁解除 pin 即釋放列，切新版取消舊工作；src、srcset、SW key 必須包含新 v，失敗不能 fallback 舊圖。
按實際可見面 eager、其餘 lazy，不能把固定 100 張 eager 當規格。

本配置仍須用 2.0 雙區輸出核對 ≤512 KiB 單片、manifest、首屏 24 面同包／混包及單面成本、增量重建與真實 heap；
不能拿其他配置的數字當通過。未驗收不得發布，超標依 §5 的候選／升版流程處理，不在同格式自動改 width。

## 6. 覆蓋與 QA／errata 摘要

`Coverage = {reviews:[ReviewCoverage],translations:[TranslationCoverage],mechanics:[MechanicCoverage]}`；陣列可空，空不等於已完整查核。

| 型別 | 完整欄位與計數 |
| --- | --- |
| ReviewCoverage | `{entity:printing/art,region:Region,total:UInt,unreviewed:UInt,model_reviewed:UInt,sampled:UInt,confirmed:UInt}`；按 entity/region 排序，四種狀態數合計 total，按出貨永久 ID 去重；art 可在兩個 region 各計一次 |
| TranslationCoverage | `{region:Region,target_lang:Lang,total_fields:UInt,translated_fields:UInt,missing_fields:UInt}`；按 region/target_lang 排序，translated+missing=total；計 current face_revision 的 name/effect 及每個 section 的使用位置，不計歷史或 printing 倍數；target_lang 等於原文語言的組合不列；實際可選 FieldTranslation 才算 translated；low_confidence=true 仍計 translated，沒有有效選用才計 missing，不另推導人工審核狀態 |
| MechanicCoverage | `{region:Region,scope:Code,total_cards:UInt,any_annotated_cards:UInt,fully_annotated_cards:UInt,unknown_cards:UInt,eligibility:"all_non_retired_cards_in_region",by_keyword:[{keyword_id:ID,complete_cards:UInt}]}`；按 region/scope 排序、keyword 按 ID 排序；沿 snapshot-format §8 的 fresh／EN block／分母規則 |
| SourceWindow | `{kind:errata/qa/cardlist/cr,region:Region,scope_key:Text,from_date:Date,until_date:Date?,as_of:Date,state:complete/partial,source_url:URL}`；scope_key 限 `region:*` 或 `product:<id>` |
| RestrictionCoverage | `{profile_id:ID,from_date:Date,until_date:Date?,state:complete/partial,source_url:URL}`；source_id 投影為公開來源 URL，不出建置 FK |

窗口是 `[from_date,until_date)`；until=null 只表示未另記有限終點，不保證未來來源完整。SourceWindow 查詢還須 date≤as_of；RestrictionCoverage 無獨立 as_of 時最多證明到 manifest.published_at 的 UTC 日期。重疊窗口有 partial／矛盾時不得挑 complete 宣稱完整；producer 應合併無矛盾窗口或隔離衝突。缺窗口、日期／region／scope 不涵蓋皆 unknown。

`qa_card_ids` 從本次出貨的 qa.current_version_id 對應 qa_version.cards 聯集推導；歷史版本的已移除關聯不算現行 QA。`errata_card_ids` 從本次 errata.versions.changes.face_id 及 printings.printing_id 對應的 card 聯集推導；包含已公布但未生效的公告，UI 另讀日期。兩者先驗引用閉包、再排序去重；有 ID 表示 present。沒 ID 只有在相應 kind／region／收錄 scope 的完整來源窗口涵蓋查詢日期時才為 absent，其餘 unknown。摘要不表示裁定適用已人工確認，也不能由一區的 absent 推另一區。

## 7. changes 元素

changes 是一般物件 `{format_version,from_data_version,to_data_version,added,modified,retired,errata,new_qa_versions,identity_changes,coverage_changes,support_changes}`。from_data_version 為 Text?（首版才可 null），to 必須等於 manifest；正式續版指精確前一發布版。所有陣列存在，即使無變更也填 []。不放原始官方卡文或私有稽核資料。

| 欄位 | 元素完整形狀 |
| --- | --- |
| added, modified, retired | `{entity:Code,key:{欄名:主鍵值},changed_fields:[Text],reason:Text}`；entity 限該 format 的公開集合（2.0 為 40＋3，3.0 為 43＋3），key 恰含其 PK 欄且型別相同；changed_fields 是排序唯一的公開頂層欄名，modified 非空，added/retired 為 [] |
| errata | `{errata_id:ID,version_id:ID,card_ids:[ID],reason:Text}`；新公布／改版的 ErrataVersion，不把所有舊公告重列 |
| new_qa_versions | `{qa_id:ID,version_id:ID,card_ids:[ID]}`；包含新增題與同題新 revision |
| identity_changes | `{identity_change_id:ID}`；引用本次新增公開 identity_change，包含撤回事件；原事件不重寫，reverts_id 沿 snapshot-format 的新格式白名單 |
| coverage_changes | `{section:reviews/translations/mechanics/source_windows/restriction_coverage,before:JSON?,after:JSON?,reason:Text}`；JSON 精確為 §6 該 section 的一列，至少一側非 null；兩側都有值時鍵相同，改鍵表示刪舊＋增新 |
| support_changes | `{card_id:ID,region:Region,before_status:Code?,after_status:Code?,before_reasons:[Code],after_reasons:[Code],reason:Text}`；status 限既有五態，null 表示該區卡尚未／不再出貨；reasons 排序去重，缺側為 [] |

added/modified 的 key 必存在新快照；retired 指前版存在而新版不存在的公開列，不代表撤銷永久身分或刪掉永久 registry。陣列按 entity＋canonical key／各元素 ID 排序，coverage_changes 按 section＋其列鍵排序，support_changes 按 card_id/region 排序；不得重複。before／after 的來源版本要與 from／to 一致。QA／errata card_ids 排序去重且須與對應新版本的關聯一致。

support_changes 比較套用 override/block 後的有效狀態；同狀態但 reasons 或 manifest engine_support_target 改變也列出受影響卡區，reason 明示目標變動。changes_ref 為 null 不宣稱「無變更」。ETag／抓取時間改變但公開投影不變不列 modified；coverage 的查核日期變動屬公開投影變動。完整發布閘門仍驗兩版實際差異，摘要不能代替資料閉包驗證。

2.0.0 的 printing_image changed_fields 包含 publication_state、availability、card_version、art_version、variants；image_variant 不再接受 path。key 仍依各自 PK，不能把 URL 當 ID。changes 是摘要，不是 row delta；previous 引用的 changes blob 保留，但其 from_data_version 不構成對更早快照的遞迴保留依賴。落後多版或無法讀取跨格式摘要時全量取得 current，仍須通過格式准入；詳見 [snapshot-format §4.1](snapshot-format.md#41-發布窗口圖片新鮮度與回收)。

## 數位同名規則的版本准入

[數位名字政策](../domains/digital-name-policy.md) 的 same_name 枚舉能力使用 §5.4 的 2.0 配置。未支持 digital-same-name-links-v1 或 min_reader 不足的 reader
拒絕該快照，不把規則 unreviewed 誤看成裸候選或真人確認。
政策與收據不出貨，不追加公開 tuple 欄位；枚舉新增 minor、既有欄序／語意更換 major，沿既有快照准入與完整引用閉包。

## 8. format 3.0.0 的 annotation 變更

[公開 annotation 契約](public-annotation.md)及其[元件 Schema](public-annotation.schema.json)釘住新版：
format_version／min_reader_version=3.0.0，原六個 capability 加 jp-source-translation-v1 與 public-annotation-v1，
完整集合依該契約 §1，所有封套及 index entry 一致；index_format=2 不變。
原有 canonical、N=64、band widths、media、base／row_index 與同名規則沿用；整體容量仍須重新量測。

欄序以公開 annotation §2.1 覆寫 2.0 的同名列：translation 加必填可 null 的 annotation_set_id，FieldTranslation 加
source／counterpart 並改 basis；cr_clause 加 translations；新增 annotation_set／field_annotation／annotation_concept。
Translation 在 bootstrap／detail 都用完整八欄。所有原本內嵌 FieldTranslation 的位置都同步使用七格，
不能只改 face_revision 而漏掉 printing／QA／keyword／vocabulary／商品。新增 PublicTextPointer／Annotation／AnnotationRange
的 descriptor 與三個有限 JSON reference 定義必隨使用處完整附入 types。

新增集合的 PK／欄序、欄位分割及 bucket 依公開 annotation §2／§5；field_annotation 的 history
只用於非 display revision 的原文用途，base=null，完整 tuple，不套 face_revision 的 row_index。
其他集合仍沿前述欄位分割，printing.faces 的 face_ordinal 與 translation 子陣列 join 鍵不變。
text_all 包含全部 43 文字集合的原始 File 聯集；changes 支援新表 PK／欄位白名單。
annotation_set／field_annotation 只產非空集合及用途列；row_counts 計實際列數，不為空集合建立 fragment。
translation.annotation_set_id=null 表示空集合；缺 field_annotation 只有在對應分片完整驗畢後才可當空，
非 null 引用缺目標仍拒絕。完整性由 producer 比對投影前 occurrence 保證，不能靠 reader 重造空 set ID。
改 annotation／bold 可能改 set ID 及用途列，不能因 text bytes 未變省掉相關 changes／依賴更新。

機器 Schema、reader 支援表與共用完整 golden 已在 #498 的 N0 同步替換；正式 producer／reader 為 3.0.0，不沿用舊 2.0 標頭，
也不得只升 data_version。固定案例依 [public-annotation-cases](public-annotation-cases.md)，
正式容量依 [size-budget](size-budget.md#30-annotation-與-jp-來源的計帳)，N0 已接線並完成量測；未達預算的項目沿用維護者豁免，由 #506 最佳化，手機實測由 #53 承接。
