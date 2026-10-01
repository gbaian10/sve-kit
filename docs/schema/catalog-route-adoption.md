# 詞彙、記號與路由採納契約提案

本文件細化 [build-db §2／§15](build-db.md#15-網址搜尋預設版次與記號) 的 authored 輸入。
**新增格式與政策均為提案，待使用者決定**；不是已核可資料、loader 實作或發布驗收。
§1 摘錄既定語意；§2–§7 是可供審核的技術格式；§8 集中列出政策選項。
格式核可後，仍須 loader 通過 §9，且每筆真實資料有適用採納，才能供正式建置。
本契約不以任意 confirmed 決定或 caller 提供的布林值代替採納。

## 1. 既定邊界

- 詞彙 code 是穩定的小寫英文 `[a-z][a-z0-9_-]*`，譯名改字不重配 code；官方原值另留 raw。
- `printing.rarity_code` 與 `premium` 已分欄；單獨的「プレミアム」為 rarity=null、premium=true。
  未知 premium/frame/signed 是 null，缺 stamp 列不自動證明無標誌。本文件不重開分欄決定。
- 卡號與區域構築名稱保留 exact 原值；跨區同名不能混群。構築名稱不是搜尋別名，也不合併 card 身分。
- 一般路由與預設版次由確定性規則推導；同號 variant 與人工 default 才採納覆寫。
  改卡片預設不改版次 URL；永久入口、改號與身分修復仍依 [identity-repair](identity-repair.md)。
- 繁中來源與翻譯採納一律依 [translation-contract](translation-contract.md)。術語的逐條確認仍在該流程，
  不因本封套 confirmed 而把未採納譯詞升級。日／英 UI 不回退繁中，UI fallback 不套用卡文。
- 卡文記號只做文字與圖示呈現，不產生自身 has、遊戲規則或引擎支援；Q 保留 literal，變數白名單仍只有 X。
- 本契約不收官方卡文全文；官方字句以凍結來源引用重建。例子與驗收鍵均為合成資料。

## 2. 入口、分片與封套

採兩個獨立入口，共用下面的決定與續版規則，不擴充 registry／ids／products 的 kind 白名單。
所有路徑相對 authored 根。

| 入口／分片 | 完整頂層欄位 |
| --- | --- |
| `catalog-adoptions/index.yaml` | `catalog_adoption_format: 1, kind: catalog_adoption_index, includes` |
| `catalog-adoptions/<area>/<filing_key>/<sequence>.yaml` | `catalog_adoption_format: 1, kind: catalog_adoption_shard, default_decision_id, records, decisions` |
| `display-overrides/index.yaml` | `display_override_format: 1, kind: display_override_index, includes` |
| `display-overrides/<area>/<filing_key>/<sequence>.yaml` | `display_override_format: 1, kind: display_override_shard, default_decision_id, records, decisions` |

catalog 的 area 為 `vocabulary/symbols/aliases/rules-names/languages`；display 的 area 為 `routes/defaults`。
filing_key 為 `[A-Za-z0-9_-]+`，只作歸檔、不產生商品或地區真值；卡片相關可沿既有 owner，
共用資料可用 `shared`。sequence 為每個 area/filing_key 從 001 起連續只增的三位以上序號。
同片一種 record.kind、一個決定，records 非空並按 record_key 排序，decisions 恰含 default_decision_id 所指決定。

YAML 1.2 邊界、單檔嚴格小於 1 MiB／512 KiB 目標、canonical recipe 沿
[authored-layout §1／§2](authored-layout.md#2-分片批次決定與來源)。所有欄位必填，可空者明示 null；未知欄位拒絕。
includes 映射上述各自分片路徑到**完整解析內容**的 canonical hash；禁止絕對路徑、`..`、symlink、重複鍵、
缺檔、未索引分片、跨入口偷載與 hash 不符。先驗全入口、全區、全部歷史，再投影本次範圍。
歷史分片與 index 舊 entries 不改；新分片驗妥後原子追加 index。啟用入口時必有 index，空集合明示 includes={}。

record 恰為 `{record_key,kind,filing_key,data,evidence}`。data 恰為
`{subject,adoption_no,predecessor,value,review_context,dependencies,reason}`：

| 欄位 | 定義 |
| --- | --- |
| subject | §4／§5 的完整選擇鍵，不含版號；不可於續版換鍵 |
| adoption_no | 同 kind/subject 從 1 起連續只增正整數 |
| predecessor | 首筆 null；其後 `{record_key,record_hash,decision_id}`，恰指此鏈緊接前件 |
| value | 該 kind 的完整替代值；null 為明示停止選用，首次不得 null；不是局部 patch |
| review_context | 沿 authored-layout §9.2 的 `{context,source_batches}`，釘核對時程式、依賴、設定、來源及已存在 authored 入口 |
| dependencies | 排序唯一的 `{record_key,record_hash,decision_id}` 陣列，指全部人工採納依賴；無依賴為 [] |
| reason | 非空的人寫理由，不含官方卡文、私人路徑或秘密 |

record_key 是 `[kind,subject,adoption_no]` 的 canonical JSON **字串**，全域唯一。
subject 是物件，鍵排序依同一 canonical recipe，不使用有分隔符碰撞風險的字串拼接。
review_context 不引用尚未寫出的自身分片；建置再另釘新入口。依賴圖不得循環或自我引用，
歷史依賴須可重現；當前選用須匹配有效依賴。reason 與 evidence 都參與 record hash。

### 2.1 決定、成員與 hash

decision 恰有 `id,state,scope,category,policy_id,membership_hash,members,sample_ids,authored_by,authored_at,reviewed_by,reviewed_at,reviewed_precision,note`。
state 固定 confirmed、scope 固定 batch；一筆也是一成員 batch。其他 state 不得進入入口，候選留 authored 外。
reviewed_by／reviewed_at 須記使用者實際確認者與時間；日精度沿 authored-layout 的 day 編碼，否則 instant。
authored_by／authored_at 記真正製作封套者／時間，不冒充核對者。note 可為空字串。

| record.kind（area） | category | policy_id |
| --- | --- | --- |
| vocabulary_adoption（vocabulary） | vocabulary_adoption | catalog-vocabulary-v1 |
| text_symbol_adoption（symbols） | text_symbol_adoption | catalog-symbol-v1 |
| search_alias_adoption（aliases） | search_alias_adoption | catalog-alias-v1 |
| rules_name_adoption（rules-names） | rules_name_adoption | catalog-rules-name-v1 |
| language_adoption（languages） | language_adoption | catalog-language-v1 |
| route_override_adoption（routes） | route_override_adoption | display-route-v1 |
| default_printing_adoption（defaults） | default_printing_adoption | display-default-v1 |

令 H 為 authored-layout §2 的完整 SHA-256 canonical JSON recipe：UTF-8、object keys 排序、無空白／尾換行、
不做 Unicode 正規化。`record_hash=H(完整 record)`，members 恰為本片所有 `[record_key,record_hash]` 按 key 排序，
`membership_hash=H(members)`，`decision.id="d:"+membership_hash 的 64 hex`；Hash 值帶 `sha256:`。
sample_ids 恰為排序且唯一的全部 checked record_key；缺一、多一、重複、只列代表項均拒絕。
核對包含 value、全部受影響目標、來源與依賴，不因外層只有一成員而縮成只看一張卡。

loader 必須重算三層 hash、驗 category/policy/area/kind 與精確成員、checked、人名、時間及實際核對收據。
本封套就是人工全筆採納收據，不能由工具自行填入人名；規格文件、候選頻率、來源頁或模型審核均不等於使用者確認。
本版不沿用表記 approved_rules 或模板長尾 approved_policy 例外。
身分／商品／其他詞彙／舊修訂的 confirmed 決定，即使有 authored source，均不能代簽本筆內容。

### 2.2 來源與 freshness

evidence 沿 [translation-contract §2](translation-contract.md#2-人工採納入口) 的排序去重
`{source_ref,role}` 陣列；source_ref 的 parser/locator/text_hash 指向凍結投影的 exact 字串。
圖片人工核對可改用 `{image_ref,role}`，image_ref 恰為
`{store_id,batch_id,source_version_id,raw_hash,printing_id,face_id}`，須驗圖像 descriptor/raw hash 及版次面關聯。
兩種 ref 恰擇一；role 非空，不將圖片 hash 當作文字 hash。所有批次列入 review_context.source_batches。
純自撰的 code、別名或介面配置可 evidence=[]，但依賴、核對收據不可缺；聲稱官方原值／名稱／記號者必有來源。

每個實際使用的 source version 都驗 batch/descriptor/receipt/raw 閉包、parser 程式與設定 pin、locator 和 exact bytes。
人工依賴驗完整 decision/member/hash 與有效鏈；版次依賴驗當前 registry 身分、區域、面、卡號與觀測。
核對時的 review_context 可重播不代表適用新輸入：本次 source_ref、映射原值、依賴版本或 §5 候選集合改變時，
舊採納不自動 fresh。新 raw/parser 若 exact 結果相同，只有下述原值映射可重用；其他逐來源採納依各自 freshness 規則。
無論是否重用，都須可驗原 recipe 與原來源；不能只換 recipe 名。

詞彙原值映射只授權已核對的 `(kind,region,lang,raw exact bytes)`，可用於本次其他相同原值的觀測，
但每次都要從當次凍結觀測驗明 exact 相同；不能延伸到新 spelling、被切半的 trait 或新類別。
來源頁不再存在於本次批次不等於概念被刪除；若該概念與所用映射仍可從釘住證據驗回，穩定 code 可保留。
rules-name／route/default 等逐目標決定則依各自完整目標集合重驗，不能套用這個原值重用規則。

結構／hash／來源損壞是整筆建置交易失敗；來源完好但本次適用性失效列 stale，停止該選用、阻止受影響正式發布，
不得默默挑回舊採納。preview 可依既有排除閉包規則隔離並如實報告；純推導且無 stale 覆寫者仍可正常推導。

## 3. 續版、修正與投影

更正只追加下一 adoption_no 的完整 value、精確 predecessor 與新決定，歷史 bytes 不動。
拒絕分叉、缺號、倒指、錯前件 hash／decision，不能按檔名、日期、最大 decision ID 或最後讀入者選勝者。
null 停用也是鏈的一版；恢復須追加非 null，前件仍指停用版，不重開 adoption_no=1。
原採納者誤選時，亦以新決定指向正確完整內容；不自動恢復更早版本。

vocabulary 的 `(kind,code)`、symbol 的 id/code 配對不可重配；停用仍保留歷史鍵，不給其他概念重用。
同字不同概念要新 code；錯把兩概念合併時停用錯映射、另採納正確映射與依賴，不藉改 label 偷換概念。
變動 keyword、參數或拼法須新 symbol 採納版並重驗依賴，不因 id 相同沿用舊 token／翻譯。

建置先驗全部歷史封套，按 predecessor 建有效投影、驗本次 freshness，再同交易寫 DB。
`vocabulary`／`language` 無 decision_id，不新增隱形 subject 真值表：以完整 authored source_record、decision_source 與 F1
保存採納追溯；其餘具 decision_id 的表填**實際有效版**的決定。vocabulary 停用投影 active=false，
其餘 null 停止對應覆寫／人工列；仍被使用的語言或詞彙不得因此製造懸空 FK。
route 撤回若將改已公開入口必走 §5 的永久路由限制，不得因 null 自動刪入口。

F1 釘兩個入口與所有分片的 exact bytes／canonical hash、完整 authored commit、來源／parser／配置、有效依賴與選用結果。
重建只投影當次有效值，完整歷史留 authored 與舊快照。覆寫不得跨元件修改 registry、配號、翻譯或 public schema。
未接 loader 的 typed API 只能做合成驗證，不能作真實採納旁門。

## 4. 詞彙、語言、別名與特殊名稱

本節 value 的欄位均完整列出；Text 不做隱式 trim/fold，Code 必須直接符合格式。
`TextValue` 恰為 `{kind:authored,lang,text}` 或 `{kind:source,source_ref}`：前者限自撰文字，
後者沿來源 recipe 重建非空原字串及其語言，且 ref 必出現在 evidence。翻譯不放此欄。

| kind | subject | 非 null value 的完整欄位 |
| --- | --- | --- |
| vocabulary_adoption | `{kind,code}` | `{label,raw_mappings,active}`；label 為 TextValue，active 為 Bool |
| language_adoption | `{code}` | `{display_name,fallback_order}`；非空 Text、排序有意義且無重複的 Lang 陣列 |
| search_alias_adoption | `{kind,code,lang,text}` | `{normalized,normalizer}`；normalized 為非空 Text，normalizer 見下文 |
| rules_name_adoption | `{face_id,region,role}` | `{name,identity_ref,observations}`；name 為來源型 TextValue，另兩欄見下文 |

### 4.1 詞彙與語言

本版 vocabulary.kind 白名單恰為 class/type/rarity/trait/title/frame/stamp_series。
其餘既有固定 enum 須先依 P8 明列來源表／欄、專用 kind 與完整 code 對照，更新受版控白名單後才能載入；
不能由 caller 在 configuration 填任意 kind 就擴張。keyword/stamp/product_family/card 保留給各自目標表，不能冒充詞彙。
raw_mappings 是排序唯一的 `{region,lang,raw,source_ref}` 陣列；region 為 jp/en，lang 須與來源一致。
有效且 active 的映射中，相同 kind/region/lang/raw 同時映射兩個 code 即失敗；
停用的歷史映射不參與選用，但不可重用其 code 給另一概念。空陣列允許純介面 enum，但不能假稱已涵蓋官方原值。
來源的未知符號（含 `-`）如何投影 null 須有欄位 recipe，不把它自動採為職業或稀有度 code。
稀有度／premium 的拆解是釘住 recipe 的來源投影，不能讓 raw_mappings 改寫 premium；未知組合不猜。

label 只保存一個基底原文／自撰標籤；其他語言由 translation-contract 的 vocabulary_choice 連同
`(vocabulary_kind,vocabulary_code,lang)` 採納，不能在本檔再放一份三語翻譯表。
若 trait 同時是 glossary 概念，只有明示同概念關係才可使用既有選詞，不能因字串相同合併概念。
已核可譯名的數位／社群來源與 machine 標示沿翻譯契約，不重新要求逐卡確認。

language.code 沿 Lang；fallback_order 不含自身、未知語言或重複項；全體 fallback 圖不得成環。
它是完整依序嘗試清單，不遞迴串接其他語言的清單。ja/en 清單不得含 zh-Hant；
最終基底回退亦不得繞過這項限制，基底為繁中時改用穩定 code。
原文／固定 code 的最終呈現與缺譯標示見 §8；語言配置修改也需續版，不改卡面地區與卡文來源。

### 4.2 搜尋別名

kind→目標表的白名單沿 build-db §15；code 必存在且有效，keyword/stamp 尚未啟用時不得用同名 vocabulary 冒充。
text 非空、lang 已登錄，同 subject 只有一條鏈；不同目標可有同一別名，多義回全體並讓使用者選，不任取第一。
canonical code 仍先 exact 查找，正規化只作用別名查找，不修改卡號、構築名稱、存檔原文或 canonical code。

normalizer 恰為 `{version,program_revision,code_path,code_hash,config,config_hash}`，釘可重現的版控實作與 canonical 配置。
loader 重算 normalized，不能信 caller 傳字串；同次查找索引只接受建置配置指定的同一 normalizer pin。
改版本需對全部有效 aliases 重算並逐項續版／採納，不能把舊 normalized 配上新版本。
自動由官方原名推導的 alias 可依既有 DB 契約無 decision，但必須有獨立釘住的推導 recipe 與來源；
本入口所有人工別名都必須 confirmed，不能省略決定冒充自動推導。本版不新增搜尋 grammar 或 query_alias。

### 4.3 特殊構築名稱

role 限 collab/treated_as；一般 primary 仍從同區官方 current 名稱推導，無 current 但所有觀測名稱 exact 相同亦可推導，
有不同名稱則留未定，不藉特殊名稱封套偷選 primary。name 的語言必與 region 相符（jp→ja，en→en）。

identity_ref 是 `{face_id,card_id,registry_record_key,record_hash,decision_id}`，引用有效面身分；observations 是
排序唯一的 `{printing_id,face_id,source_ref}` 陣列，完整列該 face/region 在核對範圍的全部名稱來源，
printing 必同 card、同區且包含此面。新增／移走版次、面重配或任何名稱來源變更均重驗並需新採納。
規則依據另列 evidence，不因名稱共現就認定 treated_as；沒有同區版次不能造該區名稱關聯。

name 重建後沿既有 `(region,official_name exact)` 產 rules_name，特殊關聯 decision 指本決定。
本版每個 face/region/role 只採一個特殊名稱；多名稱同 role 是另案擴充，不能塞字串列表或冒用第二個 role。
撤回移除該人工關聯，若同名群組仍被其他面／推導 primary 引用則保留，不刪共享物件。
不剝括號、不跨區合群、不逐面多算牌組張數。

## 5. 同號路由與預設版次覆寫

| kind | subject | 非 null value 的完整欄位 |
| --- | --- | --- |
| route_override_adoption | `{region,route_key}` | `{printing_id,candidates,candidates_hash}` |
| default_printing_adoption | `{card_id,region}` | `{printing_id,candidates,candidates_hash,selection_basis}` |

route_key 為原始 official 卡號，不是 URL encoded 字串；route 只解決**同區同 exact 卡號**的多個 variant。
candidates 必完整列同區該 exact 卡號的全部 official printing，至少兩個；選中 printing_id 必為成員。
不允許 provisional override、跨區 exact 撞號、保留路徑、不存在／錯號目標，亦不修改 card_no 或 variant_key。
跨 JP/EN exact 撞號仍是待決 URL 設計，不能把 region 加入此 authored subject 就假裝公開 URL 已隔離。

兩種 candidates 都是按 printing_id 排序唯一的
`{printing_id,card_id,region,card_no,card_no_state,variant_key,identity_ref}`；identity_ref 恰為
`{record_key,record_hash,decision_id}`，指有效 printing 身分記錄。candidates_hash=H(完整 candidates)。
核對範圍由 review_context 的完整 registry 與凍結來源重建，不接受 caller 自選清單；新增 variant 亦使舊決定 stale。
即使選中者未變，省略競爭者也不能沿用舊採納。

預設覆寫的 candidates 完整列同卡同區、在 selection_basis 指定的展示集合內的版次。
selection_basis 恰為 `{scope_id,scope_hash,selector_version,policy_hash}`；scope_id 是建置配置的具名展示範圍，
scope_hash 釘該範圍內排序唯一的完整 printing_id 集合；selector 與政策 bytes 由 F1 驗回。
不是只 hash 當前卡片所挑的一筆。覆寫目標須同卡、同區、可展示；Decklog 不可用不等於不可展示。
本版採保守 freshness：集合或上述 pins 改變即需重新採納，不把特定 preview 的選擇自動帶到正式全量輸出。
人工 override 可以選特殊版，不必偽造「一般」證據；投影 method=override，不標 earliest_general。

一般唯一 official 路由仍自動推導，不能為每卡造冗餘 route 決定。
同號 route 首次採納後，若續版／撤回將改已發布 route 的 printing 目標，必須在
identity-repair 的完整路由交易中提供前後件與永久入口比較；本入口不能單獨通過發布。
未發布候選也要驗原 route 不被另一 active key／alias 劫持。
舊 alias 的展平、merge/split/reassign、改號與 provisional corrected 不走本入口。
預設覆寫撤回則回現行機械 selector，只改 card 區域預設，不更新 printing URL 或 owner。

## 6. 卡文記號與三語文案

text_symbol_adoption.subject 為 `{id}`；value 恰為
`{code,parameter_schema,keyword_id,spellings,source_localization,localization_refs}`。
id 為人工首次指定的永久非空 ID，code 符合 Code；二者全域一對一且續版不改。
parameter_schema／spellings 沿 [snapshot-transport §3.2](snapshot-transport.md#32-公開參數宣告)
與 [snapshot-format 的 Spelling](snapshot-format.md#2-公開表完整欄位與玩家用途)，不另定 regex 或參數值域。
spellings 非空、排序唯一，語言已登錄；literal 拼接 prefix/suffix 必非空且 parameter_name=null。
uint/variable 必引用啟用的參數。必須消耗完整 token，保留 raw 及前導零；多義或未知回原記號，不任選 symbol。
keyword_id 可 null；非 null 須引用已採納 keyword，不能因拼法相似自動建立機制關係。

source_localization 恰為 `{lang,name,tooltip,copy_pattern}`，後三項為 §4 的 TextValue，語言須都等於 lang；
它是單一基底文案（官方來源引用或自撰說明），不是另一份三語翻譯。
localization_refs 是按 lang 排序唯一的 `{lang,record_key,record_hash,decision_id}`，引用下述翻譯選詞，
不得與基底 lang 重複，且全部 refs 都列入 data.dependencies。所有文案與翻譯的 placeholder 必依 parameter_schema 驗證，不能引入新參數或藏執行語言。
三語缺項如實缺譯，不能先填未採納文字湊滿三語；32 是候選觀測數，非格式上限或已核可名單。

**技術擴充提案（P7）**：在 translations/glossary 入口增加 `symbol_localization_choice`，
record_key 為 `["symbol_localization_choice",symbol_id,lang,adoption_no]` 的 canonical JSON 字串，
data 恰為 `{symbol_id,lang,symbol_basis,value,origin,concept_evidence,adoption_no,predecessor}`。
symbol_basis 釘 `{code,parameter_schema_hash,source_localization_hash}`，均可從上述基底重算；不引用將引用它的 symbol 採納決定，避免循環。
value 為 null（撤回），或 `{name,tooltip,copy_pattern}`，各值**直接沿用** translation-contract §5 的 value union，
採納門檻／來源／續版全部沿該翻譯契約，無模板長尾例外。
origin／concept_evidence 一律各為以 name/tooltip/copy_pattern 為鍵的完整映射，逐欄沿該契約的 origin enum／證據陣列驗證；
value=null 時兩者亦為 null。category 固定 symbol_localization_choice、policy_id 固定 translation-symbol-choice-v1，
其餘決定與 members/hash 規則沿 translation-contract §2；新增 kind 仍須單獨核可。
不把 tooltip 或 copy_pattern 偽裝 glossary_term，也不新增公開 translation owner／FieldTranslation enum。

symbol 封套只核對這些已採納翻譯引用及參數相容，不重簽翻譯語義；loader 解析 refs 後產公開 Localization 純字串。
某 choice 續版或撤回後，要新 symbol 採納版更新精確 refs；舊 ref 不可默默套到新基底／schema。
P7 未核可／未實作前，此新增 translation kind 不合法，不能用本提案繞過既有 loader。
固定三語完整度或缺文案策略見 §8，已核可的術語來源優先順序不在此重定。

## 7. 預設版次與一般版分類的政策提案

build-db §15 已有順序：同卡同區 → confirmed override → card.home_set 內的一般候選 →
最早可信 inclusion 日期 → 穩定 ID；無一般候選則從可展示版次以已知日期優先、固定 ID 選 fallback。
本節提案只細化尚缺的分類證據、完整性與展示集合，不自行改既有先後。

**建議方案（P1、P2，待使用者決定）**：採明示 rarity code／普通 frame code 白名單，
每個版次必有 premium=false、全部面 signed=false、普通 frame、已採納的無 stamp 證據，才算已證實一般。
任何一面已知特殊即排除一般；有 null／unreviewed 或缺面則為未知，不當作 false。
稀有度白名單先考慮 br/sr/gr/lg，但這只是候選，須對 JP／EN 實際完整 code 清單逐項確認；
PR、純 premium、未知稀有度或新框型不能自動歸普通。普通框的 code 本文件不預配。

無 stamp 不能由 printing_stamp 空表推導。建議分類事實交後續加工採納契約，至少釘
`printing_id,face_id,frame_code,signed,stamp_state,stamp_ids,image_ref,decision_ref`，
stamp_state 為 none/present/unknown，none 需該面已完整檢查且 stamp_ids=[]，present 需完整有效 stamp_ids。
premium 的來源／解析 recipe 亦必可驗；各面與版次完整受審才可供 classifier。
這是**所需證據的提案，不是新增可寫入 authored 的第三入口**；未完成其正式 loader 前保持未知，不能用裸 GeneralEvidence 布林假冒採納。
不降低其他加工功能既有 sampled 門檻；是否 sampled 足以宣稱「最早一般版」另見 P2。

日期只取同區 printing_product 的最早可信 day 級 first_available_on；月／年／unknown 不補成某日，
product.released_on、抓取日、owner 或卡號順序不能代替 inclusion 日期。
固定 ID tie-break 採 printing.id 原字串升序，不以建置插入次序排序。

| method | 提案中的精確條件 |
| --- | --- |
| override | fresh confirmed 覆寫，目標在本次展示集合 |
| earliest_general | 存在 home_set 一般候選，所有可能競爭的一般候選分類已知且可信日期完整；依最早日期、固定 ID 選出 |
| candidate_general | home_set 尚有未排除的一般候選，但分類或日期不齊；已證實一般者優先，再按已知日期、固定 ID；不稱「最早」 |
| fallback | home_set 無未排除的一般候選，從本次可展示集合按已知日期、固定 ID 選取；全無版次則不造 default |

未知分類者即使暫未選中，仍會阻止 earliest_general；不能只檢查勝出的版次。
若只有未知加工的 home_set 版次，建議列 candidate_general 並附缺項，這是 P1 待決細節，
不是已實作 API 行為的背書。政策 pins 與所缺證據須進建置報告；後補證據可改卡片預設，永久 URL 不變。

**展示集合提案（P6）**：selector 接收由既有公開投影規則產生並釘住的完整同區 printing 閉包，
包括 unlisted/provisional 與 Decklog unavailable，只要原有發布／投影閘門允許展示；不能由 selector 另作資格黑名單。
缺卡圖、缺翻譯、表記未定不單獨縮集合；依各既定降級規則顯示。非法／缺引用輸入仍依投影閘門排除或失敗，
不是本提案授權全部 registry 無條件公開。`/sets` 的已核可區域／歸檔過濾仍在查詢端，不改全站 card 區域預設。

## 8. 提案，待使用者決定

以下選項尚未授權採納真實資料。政策核可記錄與逐筆資料收據分開；不能只引用此提案當核可證據。

| 編號 | 選項 | 建議及影響 |
| --- | --- | --- |
| P0 封套與修正 | A：採 §2–§6 的獨立入口、confirmed 全筆、精確成員與只增續版；B：先修訂格式再採用 | 建議 A。已確認資料才能進正式檔；來源變動要重驗，不借身分決定代簽 |
| P1 一般版及完整性 | A：明示 rarity/frame 白名單＋逐面三態證據，未知可作 candidate；B：未知一律只走 fallback；C：只靠 rarity 推定普通（改變既定未知政策） | 建議 A，白名單逐項確認；C 不建議，會把特殊加工當普通。既定 override/home_set/日期/ID 順序不重問 |
| P2 無 stamp 與審查程度 | A：逐面 confirmed 的完整圖像核對才能宣稱無 stamp／一般；B：允許已有 sampled 加工組且如實標抽查 | 建議 A 作 earliest_general 的證據門檻；B 可減少人工，但需另定抽查範圍與如何呈現，不把空表當無標誌 |
| P3 稀有度顯示 | A：基礎 rarity 與 premium 分開篩選，可組合顯示；B：另採複合顯示別名 | 建議 A；DB 已分欄，不重問要不要拆。B 也不得另造重複稀有度真值；完整 code/raw 對照另逐條確認 |
| P4 介面缺字 | A：zh-Hant→ja→en，ja→en，en→ja；B：zh-Hant→en→ja，ja→en，en→ja | 建議 A，符合日文來源方向。末端回基底原文或穩定 code 並標缺譯，不空白；ja/en 仍不得回繁中。卡文沿翻譯契約，不套此順序 |
| P5 別名比對 | A：NFKC＋casefold，不 trim／合併空白、不折假名、不去標點；B：只 exact；C：再加空白／假名折疊 | 建議 A 並版本化；只改搜尋比對索引，多義保留。C 需另列轉換順序與碰撞清單後核可，不自動擴充 |
| P6 展示集合 | A：使用既有投影允許的完整同區集合；B：另外選較窄的展示政策 | 建議 A，Decklog 資格與查卡分開；若選 B 須列明排除條件，不讓 caller 任意挑集合冒稱全量 |
| P7 記號三語文案 | A：§6 的翻譯入口擴充與精確 refs，允許缺譯回原記號；B：相同入口但每個記號三語齊全才啟用 | 建議 A；沿既有翻譯採納，不複製第二份三語表。name/tooltip/copy_pattern 仍需逐項採納；32 個候選不自動全收 |
| P8 固定 enum 顯示 | A：所有對使用者展示的既有 enum 明列專用 vocabulary kind/code；B：首批只採查卡需要者 | 建議 B，其餘保持未啟用且不假裝已翻譯；同名 kind 不跨表混用，不新增 enum 值 |
| P9 特殊名稱多值 | A：首版每面每區每 role 一個特殊名稱；B：允許同 role 多名稱並改為完整集合續版 | 建議 A，遇真實多名稱需求再擴充；不得為繞限制把名稱誤填另一 role |

固定英文 code、官方原值映射、各個記號拼法與文案、實際搜尋別名與特殊構築關係仍需資料採納。
候選的頻次不是核可；萃取修正後須重產 trait 清單，不能採用被切成半截的複合特性。
已進行的術語逐條確認、繁中來源、數位優先與社群參考不在此重開政策問題。

## 9. 獨立反例與定向突變驗收

這是未來 loader 的驗收規格，**不是已執行測試或突變數量**。
每列先有最小成功基例，僅改單一條件；斜線列出的條件各建獨立例，再刪除相應 guard 做定向突變。
正常 baseline 須通過、反例須以該原因失敗，才算 killed；不把 unrelated FK 失敗或語法錯當攔到約束。
production 採納／觀測數、合成案例、實跑 mutants 分開報，未知未測不填零通過。

| 編號 | 單一反例／定向突變 | 必要結果 |
| --- | --- | --- |
| C01 | 缺 index／缺分片／未索引／symlink／跨入口／未知欄／重複 YAML key；各移除一個 guard | 各自拒絕，啟用空集合只認明示空 index |
| C02 | 一筆 value 改一字／新增成員／改 evidence，留舊 hash | 三層重算檢出，不能沿用舊決定 |
| C03 | confirmed 的 category／policy 換成合法但不適用的其他類別 | 拒絕；即使有 authored source 也不放行 |
| C04 | 借同 kind 別筆的 decision／漏一成員／多一成員／重複 member | 精確集合驗證拒絕 |
| C05 | proposed／sampled／空核對者／空時間／checked 少一筆／假 approved_policy | 各自拒絕；本入口 confirmed 全筆 |
| C06 | 刪 raw／改 parser pin／改 locator／改 text_hash／錯 image face | 來源驗證失敗，不回讀 latest |
| C07 | 前件 hash 錯／decision 錯／分叉／缺號／停用後重開第 1 版 | 各自拒絕；合法停用與恢復新版本通過 |
| C08 | 有效依賴續版，選用仍指舊版／依賴自循環 | stale 或結構失敗，不挑舊 approved 值 |
| C09 | 相同 raw 跨 kind 借映射／新 spelling 沿舊決定／完整 trait 的 ref 卻填半截 raw | 不匹配；同 kind/region/lang exact 原值重用基例可通過 |
| C10 | 同一原值兩 code／改 symbol code／譯名改字後重配 code | 衝突或穩定身分檢查拒絕 |
| C11 | alias 假 keyword 父列／錯語言／未存在目標 | 各自拒絕，不能用 vocabulary 冒充 |
| C12 | normalized 不可重算／混 normalizer pins／略過 canonical 優先／多義任選 | 前兩拒絕，後兩檢出錯誤解析 |
| C13 | language 自回退／未知目標／循環／ja 或 en 回 zh-Hant | 各自拒絕，UI 配置不改卡文 |
| C14 | 特殊名稱跨區／無同區 printing／借舊名稱觀測／剝括號 | 各自拒絕，不合併 card 或 primary |
| C15 | symbol 多餘尾字／非 ASCII 數字／越界 uint／x 當 X／Q 擴成變數 | 各自不匹配或拒絕，raw 可 roundtrip |
| C16 | localization 未採納／錯 symbol_basis／新參數／缺譯硬填／舊 ref 假 fresh | 各自拒絕；正常缺譯走核可降級 |
| C17 | route 候選漏 variant／新增競爭者／target 錯號／provisional／跨區撞號 | 拒絕或 stale，不看 UI 語言選勝者 |
| C18 | default 跨 card／跨 region／目標不在展示集合／scope_hash 錯 | 各自拒絕；Decklog unavailable 可展示基例不誤擋 |
| C19 | route 續版改已公開目標卻無 repair／撤回刪舊入口 | 發布拒絕；default 改選不能改 URL |
| C20 | 未知 premium／signed／frame／stamp 當 false，或漏背面 | 各自不能 earliest_general，依核可 P1 降級 |
| C21 | 已知 premium／signed／stamp／特殊框／非白名單 rarity 算一般 | 各自排除一般候選，不受日期早晚影響 |
| C22 | month 補 day／用 product 日代 inclusion／用跨區日期／同日反向 ID | 各自檢出日期或確定性規則違反 |
| C23 | 只驗勝出者，忽略未知競爭者／忽略 home_set／忽略 override | 各自檢出 method 或選擇錯誤 |
| C24 | 先濾 JP 再驗 EN 壞分片／半筆失敗仍提交 DB | 全入口失敗且交易回滾 |
| C25 | YAML 只換排版／輸入檔順序改／無關卡變而原值映射不變 | canonical 決定與適用詞彙結果不變，不亂失效 |

驗收須另覆蓋合法的新採納、完整續版、撤回／恢復、literal/uint/variable、雙面與多區互不污染。
選定 P1–P9 後把相應預期固化，再以 production 凍結來源驗證覆蓋；候選清單、合成成功與格式核可都不等於正式資料可發布。
