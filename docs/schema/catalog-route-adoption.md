# 詞彙與展示的當前資料

詞彙、語言、搜尋別名、記號、特殊構築名稱與展示覆寫共用 current 格式。
資料可直接修訂，由 Git 保存變更；不建立採納號、前驅或決定封套。
永久 code、來源引用、各地區 exact 原值映射、三語值與退回規則保持。

## 1. 既定邊界

- 詞彙 code 是穩定的小寫英文 `[a-z][a-z0-9_-]*`，譯名改字不重配 code；官方原值另留 raw。
- `printing.rarity_code` 與 `premium` 已分欄；單獨的「プレミアム」為 rarity=null、premium=true。
  未知 premium/frame/signed 是 null，缺 stamp 列不自動證明無標誌。本文件不重開分欄決定。
- 卡號與區域構築名稱保留 exact 原值；跨區同名不能混群。構築名稱不是搜尋別名，也不合併 card 身分。
- 一般路由與預設版次由確定性規則推導；同號 variant 與人工 default 才採納覆寫。
  改卡片預設不改版次 URL；永久入口、改號與身分修復仍依 [identity-repair](identity-repair.md)。
- 繁中來源與翻譯採納一律依 [translation-contract](translation-contract.md)。翻譯詞的來源與低信心沿新格式，
  code 的存在不授予官方翻譯權威。日／英 UI 不回退繁中，UI fallback 不套用卡文。
- 卡文記號只做文字與圖示呈現，不產生自身 has、遊戲規則或引擎支援；Q 保留 literal，變數白名單仍只有 X。
- 本契約不收官方卡文全文；官方字句以凍結來源引用重建。例子與驗收鍵均為合成資料。

## 2. 入口、分片與當前值

所有路徑相對 authored 根。只讀當前工作樹的固定資料 area，不需要 checksum index。

| 入口／分片 | 完整頂層欄位 |
| --- | --- |
| `catalog/adoptions/<area>/<sequence>.yaml` | `format: 2, kind: catalog_adoption_shard, records` |
| `catalog/overrides/<area>/<sequence>.yaml` | `format: 2, kind: display_override_shard, records` |

catalog 的 area 為 vocabulary／languages／aliases／symbols／rules-names；display 為 routes／defaults。
sequence 為三位以上十進位序號，允許缺號；area 下直接放分片。records 非空，每片的 kind 須符合 area。
YAML、單檔小於 1 MiB、路徑安全與固定目錄掃描依 [authored-layout](authored-layout.md#2-分片與來源)。
拒絕未知欄位、重複 YAML 鍵、symlink 及跨入口資料；載入後排序，不要求作者先排序。

record 必填 `{kind,data}`；origin／low_confidence／note 可省略。品質預設 project／false，
顯式預設值可讀，寫出端省略；非預設品質值與 `value: null` 保留。
record_key 載入時計算，存檔欄位拒絕；品質欄位沿翻譯契約。
推導的 record_key 是 `[kind,subject]` 的 canonical JSON 字串，同入口全域唯一，不含修訂號。
data 為 `{subject,value,evidence?}`；subject/value 見 §4–§6；value=null 表示停止使用當前值。
evidence 預設空陣列，只保存值本身沒有引用的額外文字或圖片證據。

### 2.1 來源與 exact 欄位

label 的 source_ref 與每筆 raw_mapping 的 source_ref 自動收集一次，作者不在 evidence 重填。
保留每個實際引用的 batch_id、source_version_id、parser、locator、text_hash。
同一來源同一 locator 在本次建置只解析一次，仍驗凍結 batch、descriptor、raw hash、parser 與 exact UTF-8 文字 hash。
來源 raw 與歷史歸檔不得因 authored 去重而刪除。

額外 evidence 可為 `{source_ref,role}` 或 `{image_ref,role}`；role 非空，陣列不得重複。
image_ref 為 `{batch_id,source_version_id,raw_hash,printing_id,face_id}`，圖像 hash 與文字 hash 分開驗證。
純自撰 code、別名或介面配置可省略 evidence；官方原值與名稱仍須實際來源引用。
來源損壞使建置失敗；未知 raw 列缺項，不從翻譯、拼字或卡號推算 code。
詞彙映射只適用已驗證的 `(kind,region,lang,raw exact bytes)`，不授權新拼法或被切半的 trait。

## 3. 修正與建置投影

直接修改當前 value；停用與恢復都修改同一選擇鍵，不另外建立歷史資料鏈。
vocabulary 的 `(kind,code)` 與 symbol 的 id/code 配對是永久概念身分，不能把同鍵配給其他概念。
停用值不參與選用；仍被使用的語言與詞彙不可造成懸空引用。

建置驗語言 fallback、raw mapping、特殊標記、來源欄位及唯一映射，再於同交易寫 DB。
vocabulary／language 的品質欄位與 authored_source_id 指向本次讀入的 authored 檔案，不造 confirmed decision。
DB 的 current DDL 是唯一建置 schema，FK、STRICT、來源與參數檢查保持。

offline composer 接受 vocabulary／language；其餘五種可編輯值保留 current typed loader 與各自既有功能，
尚未接進 offline 的值必須明確拒絕，不能載入後默默略過。展示永久路由仍受 §5 限制，不能藉停用覆寫改 URL。

## 4. 詞彙、語言、別名與特殊名稱

本節 value 的欄位均完整列出；Text 不做隱式 trim/fold，Code 必須直接符合格式。
`TextValue` 恰為 `{kind:authored,lang,text}` 或 `{kind:source,source_ref}`：前者限自撰文字，
後者沿來源 recipe 重建非空原字串及其語言；引用由值本身取得，翻譯不放此欄。

| kind | subject | 非 null value 的完整欄位 |
| --- | --- | --- |
| vocabulary_adoption | `{kind,code}` | `{label,raw_mappings,active,translations?}`；label 為 TextValue，active 為 Bool |
| language_adoption | `{code}` | `{display_name,fallback_order}`；非空 Text、排序有意義且無重複的 Lang 陣列 |
| search_alias_adoption | `{kind,code,lang,text}` | `{normalized,normalizer}`；normalized 為非空 Text，normalizer 見下文 |
| rules_name_adoption | `{face_id,region,role}` | `{names,identity_ref,observations,name_basis_hash}`；names 為非空來源型 TextValue 陣列，其餘見下文 |

### 4.1 詞彙與語言

本版 vocabulary.kind 白名單恰為 class/type/special_kind/rarity/trait/title/frame/stamp_series。
class/type 的首批 code 清單與 preview 對照沿[正式 catalog 輸入](catalog-inputs.md)；
special_kind 此次限 evolve/advance/token，用於基本卡種的明示特殊標記，不擴充 label 翻譯的 kind。
先做查卡所需的固定 enum；新增對照須明列來源表／欄、專用 kind 與完整 code 對照，更新受版控白名單後才能載入；
不能由 caller 在 configuration 填任意 kind 就擴張。keyword/stamp/product_family/card 保留給各自目標表，不能冒充詞彙。
raw_mappings 是排序唯一的 `{region,lang,raw,source_ref,special_kinds}` 陣列；region 為 jp/en，lang 須與來源一致。
special_kinds 為必填、按 code 排序唯一的陣列；非 type 必為 []。type 的每個完整 raw 映射到 subject.code
這個基本卡種及明示標記組合，沒有標記亦明填 []；不把進化／進階／衍生物配成 type。
標記只允許 evolve/advance/token，每個都須引用有效且 active 的 vocabulary(kind=special_kind,code)，
直接引用由工具自動收集，不能借 glossary、inactive／撤回列或任意同名 code。
首批 special_kind 是自撰標記定義，raw_mappings=[]；定義標籤不冒稱官方來源，不假造一條 raw binding 來通過驗證。

special_kinds 必填；欄位缺漏是格式錯誤。

每個 raw 保留原欄位的 exact source_ref，type 的 locator 仍指完整 card_type／info/Card Type，不用切片字串假造來源。
基本卡種＋標記的解讀由完整 value 的適用採納核對，exact 來源證據本身不等於分類核可。
有效且 active 的映射中，相同 kind/region/lang/raw 必只有一組 `(code,special_kinds)`，
同 code 卻標記不同、重複同一原值但借不同 ref，亦拒絕；來源直接保留在 mapping，不建立多條衝突映射。
已驗相同原值的當次觀測只重用這一組，不通用 split、trim 或猜新拼法；未知原值拒絕並列缺項，不產生暫碼。
建置／更新工具列出未知原值與候選，修正當前映射後自動重驗，不以點擊收據作門檻。

停用映射不參與選用，但不可重用其 code 給另一概念。空陣列允許純介面 enum，但不能假稱已涵蓋官方原值。
來源的未知符號（含 `-`）如何投影 null 須有欄位 recipe，不把它自動採為職業或稀有度 code。
稀有度／premium 的拆解是釘住 recipe 的來源投影，不能讓 raw_mappings 改寫 premium；未知組合不猜。
技術預設 P3 分開篩選基礎 rarity 與 premium，顯示可組合；複合顯示別名不建立第二份稀有度真值。

label 只保存一個基底原文／自撰標籤；其他語言的選詞放在同一記錄的 `value.translations`（`lang,text,origin,low_confidence`，不得為 ja），建置時寫成 vocabulary 的 label 翻譯；此處 origin／low_confidence 也可省略為 project／false，寫出端省略預設值。
若 trait 同時是 glossary 概念，只有明示同概念關係才可使用既有選詞，不能因字串相同合併概念。
已核可譯名的數位／社群來源與 machine 標示沿翻譯契約，不重新要求逐卡確認。

language.code 沿 Lang；fallback_order 不含自身、未知語言或重複項。
它是完整依序嘗試清單，不遞迴串接其他語言的清單；ja→en 與 en→ja 可並存，不因互相備援誤判為循環。ja/en 清單不得含 zh-Hant；
最終基底回退亦不得繞過這項限制，基底為繁中時改用穩定 code。
使用者 2026-10-01 核可繁中介面依序回退日文、英文，完整 fallback_order 與末端呈現見 §8.1；
語言配置可直接修改當前值，不改卡面地區與卡文來源。

### 4.2 搜尋別名

kind→目標表的白名單沿 build-db §15；code 必存在且有效，keyword/stamp 尚未啟用時不得用同名 vocabulary 冒充。
text 非空、lang 已登錄，同 subject 只有一個當前值；不同目標可有同一別名，多義回全體並讓使用者選，不任取第一。
canonical code 仍先 exact 查找，正規化只作用別名查找，不修改卡號、構築名稱、存檔原文或 canonical code。

技術預設 P5 的算法為先 NFKC、再 casefold；不 trim／合併空白、不折假名、不去標點。
Unicode 資料版本隨 normalizer 實作／配置釘住，不使用系統未明示的版本。

normalizer 恰為 `{version,config}`，由目前實作套用明示配置。
loader 重算 normalized，不能信 caller 傳字串；同次查找索引只接受建置配置指定的同一 normalizer pin。
改版本需重算全部有效 aliases，不能把舊 normalized 配上新版本。
人工別名直接存當前值；自動由官方原名推導的 alias 須保留推導 recipe 與來源。本版不新增搜尋 grammar 或 query_alias。

### 4.3 特殊構築名稱

role 限 collab/treated_as；一般 primary 仍從同區官方 current 名稱推導，無 current 但所有觀測名稱 exact 相同亦可推導，
有不同名稱則留未定，不藉特殊名稱封套偷選 primary。names 每項語言必與 region 相符（jp→ja，en→en）。
陣列按 canonical bytes 排序，重建後的 exact 名稱不得重複；它是此 face/region/role 的完整特殊名稱集合。

identity_ref 是 `{face_id,card_id}`，引用有效面身分的穩定鍵，本次建置驗面／卡片的實際對應。
observations 是排序唯一的 `{printing_id,face_id,source_ref}` 陣列，完整列該 face/region 的名稱來源；
它保存核對證據，不是永久固定的版次成員集合。printing 必同 card、同區且包含此面。
name_basis_hash=H(全部 observations 重建的 `{lang,text_hash}` 排序唯一集合)，text_hash 驗 exact UTF-8；
同名再錄不重複計入，hash 相同仍比 exact bytes，不把碰撞當同名。

每次建置機械重建該面該區目前全部可用名稱觀測，完整驗來源及面對應，再與受審 exact 名稱集合比較。
新增／移走同名版次、換來源版本但名稱不變，無須改特殊名稱值。
來源名稱集合改變、面／card／region 對應改變，或已採納特殊關係的相關規則依據不再適用才使關係 stale；
來源缺漏則列無法驗證，不能把缺資料當名稱沒變。沒有同區版次也不能保留該區的有效人工關聯。
規則依據另列 evidence，不因名稱共現就認定 treated_as。

各 name 重建後沿既有 `(region,official_name exact)` 產 rules_name；新增／移除名稱時直接修訂完整 names 集合。
撤回移除該人工關聯，若同名群組仍被其他面／推導 primary 引用則保留，不刪共享物件。
不剝括號、不跨區合群、不逐面多算牌組張數。

## 5. 同號路由與預設版次覆寫

| kind | subject | 非 null value 的完整欄位 |
| --- | --- | --- |
| route_override_adoption | `{region,route_key}` | `{printing_id,candidates,candidates_hash}` |
| default_printing_adoption | `{card_id,region}` | `{printing_id,candidates,candidates_hash}` |

route_key 為原始 official 卡號，不是 URL encoded 字串；route 只解決**同區同 exact 卡號**的多個 variant。
candidates 必完整列同區該 exact 卡號的全部 official printing，至少兩個；選中 printing_id 必為成員。
不允許 provisional override、跨區 exact 撞號、保留路徑、不存在／錯號目標，亦不修改 card_no 或 variant_key。
該同號入口只指選中的 variant；其他 variant 保留版次資料但不因此各有一條 official route，
不得擅加 slug／假卡號／provisional 入口替它們補 URL，新增 URL 能力須另定路由契約。
跨 JP/EN exact 撞號仍是待決 URL 設計，不能把 region 加入此 authored subject 就假裝公開 URL 已隔離。

route 的 candidates 是按 printing_id 排序唯一的
`{printing_id,card_id,region,card_no,card_no_state,variant_key}`；由本次 registry 驗 printing 身分。
candidates_hash=H(完整 candidates)，H 為 [authored-layout](authored-layout.md#2-分片與來源) 的 canonical SHA-256。
候選範圍由完整 registry 與凍結來源重建，不接受 caller 自選清單；新增 variant 須更新完整候選集合。

預設覆寫的 candidates 是本次展示範圍內**同卡同區全部 printing_id** 的排序唯一陣列，
candidates_hash=H(candidates)。每次重建完整集合並驗各成員的當前歸屬；選中者必仍同卡、同區、可展示。
集合增減、目標被移到別卡／別區或不再可展示才需重新採納；不能省略競爭者或以 caller 自選清單驗過。
Decklog 不可用不等於不可展示；同一版次的無關來源修訂不改候選集合，也不使覆寫失效。

不保存 selection_basis 或重複的 scope_hash，不將 selector_version、一般版 policy_hash 或程式版號當 freshness 條件。
人工選擇是「此卡此區預設這一版」，與機械 selector 怎麼算分開。
一般版清單核可、selector 修正、無關卡包新增都不要求重簽。preview 轉正式時重驗同卡同區集合與目標可展示性，
集合相同即可沿用；若本卡新增候選則 stale，不只因展示範圍的名稱改變就失效。
人工 override 可以選特殊版，不必偽造「一般」證據；投影 method=override，不標 earliest_general。

一般唯一 official 路由仍自動推導，不能為每卡造冗餘 route 決定。
同號 route 發布後，若修訂／撤回將改 route 的 printing 目標，必須在
identity-repair 的完整路由交易中提供前後件與永久入口比較；本入口不能單獨通過發布。
未發布候選也要驗原 route 不被另一 active key／alias 劫持。
舊 alias 的展平、merge/split/reassign、改號與 provisional corrected 不走本入口。
預設覆寫撤回則回現行機械 selector，只改 card 區域預設，不更新 printing URL 或 owner。

## 6. 卡文記號與三語文案

text_symbol_adoption.subject 為 `{id}`；value 恰為
`{code,parameter_schema,keyword_id,spellings,source_localization}`。
id 為人工首次指定的永久非空 ID，code 符合 Code；二者全域一對一且不可重配。
parameter_schema／spellings 沿 [snapshot-transport §3.2](snapshot-transport.md#32-公開參數宣告)
與 [snapshot-format 的 Spelling](snapshot-format.md#2-公開表完整欄位與玩家用途)，不另定 regex 或參數值域。
spellings 非空、排序唯一，語言已登錄；literal 拼接 prefix/suffix 必非空且 parameter_name=null。
uint/variable 必引用啟用的參數。必須消耗完整 token，保留 raw 及前導零；多義或未知回原記號，不任選 symbol。
keyword_id 可 null；非 null 須引用已採納 keyword，不能因拼法相似自動建立機制關係。

source_localization 恰為 `{lang,name,tooltip,copy_pattern}`，後三項為 §4 的 TextValue，語言須都等於 lang；
它是單一基底文案（官方來源引用或自撰說明），不是另一份三語翻譯。
記號值不存 localization_refs，也不釘翻譯選詞版本。
所有文案與翻譯的 placeholder 必依 parameter_schema 驗證，不能引入新參數或藏執行語言。
三語缺項如實缺譯，不能先填未採納文字湊滿三語；32 是候選觀測數，非格式上限或已核可名單。

**記號譯文**使用 translation format 2 的 symbol_localization_choice：
data 恰為 `{symbol_id,lang,symbol_basis,value,concept_evidence}`，品質欄位在 record。
symbol_basis 為 `{code,parameter_schema_hash,source_localization_hash}`，由當前基底自動計算防錯配；
value=null 為撤回，否則為 `{name,tooltip,copy_pattern}`，各值沿 glossary 的 value union。
concept_evidence 以 name/tooltip/copy_pattern 為鍵；origin 為整筆來源類別，混合機器內容時為 machine，
只有三欄皆為有效官方來源才可 official。此彙整不抹去各 source_ref 的 provider。
選擇鍵為 `(symbol_id,lang)`，直接修改；不存成員核可或歷史封套。
同語言不另以 choice 覆蓋基底 source_localization；base 改變須重驗 params 與 basis，不把舊文字套新記號。
不增 glossary_term、公開 owner 或 FieldTranslation enum；缺譯回原記號，低信心沿翻譯呈現，
來源損壞仍失敗。locale 文案改字不需新增 symbol 定義的核可；記號定義直接使用同一個 current 入口。

## 7. 預設版次與一般版分類

build-db §15 已有順序：同卡同區 → 當前有效 override → card.home_set 內的一般候選 →
最早可信 inclusion 日期 → 穩定 ID；無一般候選則從可展示版次以已知日期優先、固定 ID 選 fallback。
下述日期、展示集合與完整性是既有語意的技術細化；稀有度分類已核可，不改選取先後或宣稱加工證據已齊。

**使用者 2026-10-01 核可（P1：一般版稀有度白名單）**：一般稀有度限 BR、SR、GR、LG，
英版原標籤 Bronze、Silver、Gold、Legendary 分別採相同分類。以下完整列出盤點的 27 個 exact 原始標籤之分類，
不是已配發的 vocabulary code，也不是日英卡片身分對應。

| 分類 | JP 原始標籤 | EN 原始標籤 | 一般版候選 |
| --- | --- | --- | --- |
| 一般稀有度 | BR、SR、GR、LG | Bronze、Silver、Gold、Legendary | 可進候選池，仍須同卡同區、home_set 及加工條件適用 |
| 含 premium 的複合標籤 | BR・プレミアム、SR・プレミアム、GR・プレミアム | Bronze / Premium、Silver / Premium、Gold / Premium | 排除；即使拆出一般 rarity，premium=true 仍優先排除 |
| 純 premium | プレミアム | Premium | 排除；rarity=null、premium=true，不捏造基礎 rarity |
| 特殊稀有度 | SL、SP、SSP、UR | Super Legendary、Special、Super Special、Ultimate | 排除 |
| PR／Promo | PR | Promo | 不列入一般稀有度白名單 |
| 未標稀有度 | - | - | 不列入白名單；token、領袖等不因「-」自動成為一般版 |

所有含 プレミアム／Premium 的版次均排除一般候選，不能只保留拆出的 BR／SR／GR 來繞過 premium。
只有 PR／Promo／「-」等非白名單版次的卡仍有預設版次，method=fallback；不因此隱藏卡片，
也不宣稱 PR／Promo 在實物上全部有特殊加工。raw 原樣保留，穩定 code 仍按詞彙採納契約登錄，
新標籤／未核對的映射保持未知，不從卡號推分類。

**卡框與標誌不在本次核可範圍**，依既有技術設計處理：要證實一般版，仍須 premium=false、
全部面 signed=false、已採納普通 frame 及無 stamp 的證據。任一面已知特殊即排除一般候選；
null／unreviewed／缺面維持未知，不以 variant_key=standard 代替普通框證據，也不預配 frame/stamp code。
已知一般 rarity、尚無已知特殊加工但 frame/stamp 等缺證據者，可留候選池並標 candidate_general，
不能因這次核可直接升成 earliest_general；沒有一般 rarity 候選才走 fallback。

無 stamp 不能由 printing_stamp 空表推導。建議分類事實交後續加工採納契約，至少釘
`printing_id,face_id,frame_code,signed,stamp_state,stamp_ids,image_ref,decision_ref`，
stamp_state 為 none/present/unknown，none 需該面已完整檢查且 stamp_ids=[]，present 需完整有效 stamp_ids。
premium 的來源／解析 recipe 亦必可驗；各面與版次完整受審才可供 classifier。
這是**所需證據的提案，不是新增可寫入 authored 的第三入口**；未完成其正式 loader 前保持未知，不能用裸 GeneralEvidence 布林假冒採納。
加工審查沿 build-db §3.3 的 sampled/confirmed 門檻並保留抽查標示，不自行提高為全筆 confirmed。
分類規則的核可與各面加工事實的採納分開；未完成事實採納時維持未知。

日期依 [build-db §3.2](build-db.md#32-商品與發行)：每筆同區 printing_product 的有效日期，
在 first_available_precision=null（沒有覆寫）時沿該 product 的 released_on/date_precision；
有覆寫則以 first_available_on/first_available_precision 為準。只有有效精度為 day 才有完整日期。
覆寫為 month/year/unknown 時不得回退商品 day，也不得補成某日；商品自身不是 day 時同樣未知。
必須先有可驗的同區收錄關係才能沿商品日期，不從 owner、抓取日或卡號順序猜收錄。
printing 取其全部可信 inclusion 的最早已知有效日；任何可能更早的收錄日期未知，仍不能聲稱最早已證實。
固定 ID tie-break 採 printing.id 原字串升序，不以建置插入次序排序。

| method | 既有規則的技術細化 |
| --- | --- |
| override | 有效的當前覆寫，目標在本次展示集合 |
| earliest_general | 存在 home_set 一般候選，全部可能競爭者的分類與有效收錄日期完整；依最早日期、固定 ID 選出 |
| candidate_general | home_set 有已知一般 rarity 且未有證據排除的一般候選，但加工或有效日期不齊；仍依已知日期、固定 ID 選，不稱「最早」 |
| fallback | home_set 無上述一般候選，從本次可展示集合按已知日期、固定 ID 選取；全無版次則不造 default |

未知分類者即使暫未選中，仍會阻止 earliest_general；不能只檢查勝出的版次。
只有未知 rarity 而沒有可辨識的一般候選時使用 fallback；已知一般 rarity 但加工未知可為 candidate_general。
不新增「加工更完整者優先於較早日期者」的排序。政策 pins 與所缺證據須進建置報告；後補證據可改卡片預設，永久 URL 不變。

**展示集合技術契約**：selector 接收由既有公開投影規則產生並釘住的完整同區 printing 閉包，
包括 unlisted/provisional 與 Decklog unavailable，只要原有發布／投影閘門允許展示；不能由 selector 另作資格黑名單。
缺卡圖、缺翻譯、表記未定不單獨縮集合；依各既定降級規則顯示。非法／缺引用輸入仍依投影閘門排除或失敗，
本契約不授權全部 registry 無條件公開。`/sets` 的已核可區域／歸檔過濾仍在查詢端，不改全站 card 區域預設。

## 8. 技術預設與核可政策

協調者裁定的技術預設如下；格式與預設定案不表示真實資料或使用者偏好已採納。

| 編號 | 定案的技術設計 |
| --- | --- |
| P0 | 採獨立 current 入口，依 §2–§3；穩定鍵與實際來源引用保留，直接修改當前值 |
| P3 | rarity 與 premium 分開篩選、可組合顯示，不另立複合稀有度真值 |
| P5 | NFKC 後 casefold；不 trim／合併空白、不折假名、不去標點，版本化且多義不任選 |
| P6 | 使用既有投影允許的完整同區展示集合；人工 default 只比同卡同區 candidates_hash 與目標有效性 |
| P7 | 記號三語不必齊全，缺譯回原記號；建置自動選有效 choice，不二次簽核記號 |
| P8 | 首批只做查卡需要的固定 enum 對照，其餘未啟用；不得由 caller 新增任意 kind 或 enum 值 |

P2 的無標誌證據形式與標示門檻留待加工採納入口一起定，不要求使用者現在回答；
目前沿 §7 已有的未知處理及 sampled/confirmed 邊界，不把缺資料當無加工。

### 8.1 使用者 2026-10-01 核可

P1 稀有度白名單採 §7 的 JP／EN 原始標籤分類；卡框／stamp 的個別判斷與採納證據不在本次核可範圍。
P4 繁中介面缺翻譯時先日文、再英文，只影響介面詞彙標籤，不影響卡文；日／英不回繁中沿既有規則。

| 介面語言 | fallback_order（不含自身） |
| --- | --- |
| zh-Hant | `["ja","en"]` |
| ja | `["en"]` |
| en | `["ja"]` |

先找目前介面語言，再依其完整清單嘗試，不遞迴。全部缺譯時用允許的基底原文或穩定 code 並標缺譯，
不顯示空白；日／英末端也不得回退繁中，不改卡面地區或套用卡文翻譯的來源／選用。
政策核可與逐筆資料修訂分開；這兩項已無待決問題，但核可不等於 loader 已實作或正式資料已全部採納。

固定英文 code、官方原值映射、各個記號拼法與文案、實際搜尋別名與特殊構築關係仍需資料採納。
候選的頻次不是核可；萃取修正後須重產 trait 清單，不能採用被切成半截的複合特性。
已進行的術語逐條確認、繁中來源、數位優先與社群參考不在此重開政策問題。

## 9. 功能反例

正常基例須通過，反例須因對應約束失敗；不把無關 FK 或語法錯誤當成功攔截。
驗收包括以下功能，不要求資料作者重填來源或建立核可封套。

| 功能 | 必要結果 |
| --- | --- |
| 路徑、YAML 與選擇鍵 | symlink、未知欄位、重複鍵與同 subject 重複選擇均拒絕；允許缺號與未排序檔案 |
| 直接修訂、停用與恢復 | 同鍵修改當前值，不需要前驅；null 不產生新的有效值 |
| label／raw mapping 的來源 | 省略 evidence 仍驗實際引用；錯 locator、text_hash、來源 raw 或 parser 均失敗 |
| 額外圖片證據 | 驗 image descriptor 與 raw hash，不與字串 hash 混用 |
| 官網分類映射 | 相同 exact raw 不得指向兩個 code；不得跨 kind 或地區借映射，完整 trait 不得截半 |
| type 特殊標記 | 標記須已定義且 active；拒絕未知、缺漏、重複、非 type 標記與同 raw 的衝突組合 |
| 三語與品質 | 基準 label 不被 ja 譯名覆蓋；選定譯名的 origin／low_confidence 投影到譯文 |
| 介面 fallback | 拒絕自身、未知語言、重複與錯順序；ja/en 不回繁中，缺譯回允許的原文或 code |
| 搜尋與記號 | 保留 canonical 優先、多義結果與原記號退回；有限參數、placeholder、完整 token 與 uint 範圍檢查 |
| 構築名稱 | 保留 exact 名稱、地區與面身分；同名群組不合併 card，未知名稱不能自行猜定 |
| route／default | 保留完整同卡同區候選與永久 URL 邊界；錯候選、目標或已發布路由改動不得繞過身分修復 |
| 一般版 selector | premium、signed、frame、stamp、日期與競爭者未知時按 §7 降級，不冒稱 earliest_general |
| 建置交易 | 任一來源或映射驗證失敗整批回滾；未接 offline 的 kind 明確拒絕 |

正式凍結來源匯出與合成測試分開報告；合成成功不等於來源與授權已由維護者確認。
