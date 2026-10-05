# 詞彙當前資料與其他採納入口

2026-10-04 起，vocabulary／language 使用可直接修訂的 format 2，取消核可封套、點擊紀錄及採納鏈。
職業、卡種、特性、稱號與介面語言仍保留永久 code、exact 原值映射及引用檢查。
本次不放寬 display-overrides、身分、勘誤、構築名稱、搜尋別名、記號定義等非翻譯採納。
下文 format 1 的收據／續版規則對 vocabulary／language **僅供 legacy 轉換**，對其餘入口仍沿既有契約。

## 0. 詞彙與語言 format 2

catalog-adoptions/index.yaml 為 `{catalog_adoption_format:2,kind:catalog_adoption_index,includes}`；
vocabulary／languages 分片為 `{catalog_adoption_format:2,kind:catalog_adoption_shard,records}`。
includes 的 canonical hash 只驗檔案完整；過渡期可索引其他 area 的 format 1 分片，按各檔版本分派。
不再掃全部歷史，當前目錄仍須完整索引、安全路徑、唯一鍵、無缺檔／多檔且每檔小於 1 MiB。

record 欄位為 `{record_key,kind,data,origin,low_confidence}`，note 可省略；品質欄位沿翻譯契約。
kind 為 vocabulary_adoption/language_adoption；record_key 是 `[kind,subject]` canonical JSON 字串。
data 恰為 `{subject,value,evidence}`，subject/value 沿 §4 表格，evidence 為 §2.2 的來源引用陣列，
不含私人證據或人員事件。null value 表示明示停用；code 不因此供另一概念重用。
外鍵依賴從 value 自動取得，去掉手寫 dependencies、review_context、adoption_no、predecessor 及 reason 雜湊鏈；理由只在 note。

直接修訂 value 與拼法，新增 commit 保存歷史。一般 reader 驗格式／引用；建置自動驗當前來源、
唯一映射、特殊標記及語言 fallback。來源損壞失敗，未知 raw 列缺項，不產暫碼或靠翻譯猜代碼。
DB 由當前 record 投影並指回 authored_source_id，不能建立假 confirmed decision；其餘入口的 decision 約束不變。

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

## 2. 入口、分片與封套

採兩個獨立入口，共用下面的決定與續版規則，不擴充 registry／ids／products 的 kind 白名單。
所有路徑相對 authored 根。

| 入口／分片 | 完整頂層欄位 |
| --- | --- |
| `catalog-adoptions/index.yaml` | `catalog_adoption_format: 1, kind: catalog_adoption_index, includes` |
| `catalog-adoptions/<area>/<filing_key>/<sequence>.yaml` | `catalog_adoption_format: 1, kind: catalog_adoption_shard, review_context, default_decision_id, records, decisions` |
| `display-overrides/index.yaml` | `display_override_format: 1, kind: display_override_index, includes` |
| `display-overrides/<area>/<filing_key>/<sequence>.yaml` | `display_override_format: 1, kind: display_override_shard, review_context, default_decision_id, records, decisions` |

catalog 的 area 為 `vocabulary/symbols/aliases/rules-names/languages`；display 的 area 為 `routes/defaults`。
filing_key 為 `[A-Za-z0-9_-]+`，只作歸檔、不產生商品或地區真值；卡片相關可沿既有 owner，
共用資料可用 `shared`。sequence 為每個 area/filing_key 從 001 起連續只增的三位以上序號。
同片一種 record.kind、一個決定，records 非空並按 record_key 排序，decisions 恰含 default_decision_id 所指決定。

YAML 1.2 邊界、單檔嚴格小於 1 MiB／512 KiB 目標、canonical recipe 沿
[authored-layout §1／§2](authored-layout.md#2-分片與來源)。所有欄位必填，可空者明示 null；未知欄位拒絕。
includes 映射上述各自分片路徑到**完整解析內容**的 canonical hash；禁止絕對路徑、`..`、symlink、重複鍵、
缺檔、未索引分片、跨入口偷載與 hash 不符。先驗全入口、全區、全部歷史，再投影本次範圍。
歷史分片與 index 舊 entries 不改；新分片驗妥後原子追加 index。啟用入口時必有 index，空集合明示 includes={}。

每片一份 review_context，釘核對時程式、依賴、設定、
來源及已存在 authored 入口；同片 records 共用，核對背景不同時分片，不在每筆複製完整輸入。

record 恰為 `{record_key,kind,filing_key,data,evidence}`。data 恰為
`{subject,adoption_no,predecessor,value,review_context_hash,dependencies,reason}`：

| 欄位 | 定義 |
| --- | --- |
| subject | §4–§6 的完整選擇鍵，不含版號；不可於續版換鍵 |
| adoption_no | 同 kind/subject 從 1 起連續只增正整數 |
| predecessor | 首筆 null；其後 `{record_key,record_hash,decision_id}`，恰指此鏈緊接前件 |
| value | 該 kind 的完整替代值；null 為明示停止選用，首次不得 null；不是局部 patch |
| review_context_hash | H(本片 review_context)，每筆必須相同；以完整 SHA-256 將共享核對背景綁入成員決定，避免換背景卻沿用舊批准 |
| dependencies | 排序唯一的 `{table,key}` 陣列，指穩定目標與其有效採納；見下文，不釘整筆依賴的修訂 hash；無依賴為 [] |
| reason | 非空的人寫理由，不含官方卡文、私人路徑或秘密 |

record_key 是 `[kind,subject,adoption_no]` 的 canonical JSON **字串**，全域唯一。
subject 是物件，鍵排序依同一 canonical recipe，不使用有分隔符碰撞風險的字串拼接。
review_context 不引用尚未寫出的自身分片；建置再另釘新入口。review_context_hash 只 hash 共享物件，不 hash 封套，
沒有自我引用。它保存核對時背景，本次建置背景不同本身不使資料 stale；各 kind 的相關內容另依 §2.2 重驗。
依賴圖不得循環或自我引用；reason、evidence 與 review_context_hash 都參與 record hash。

dependencies.table 白名單為 vocabulary/language/text_symbol/keyword/stamp/product_family/card/face/printing/rules_name，
key 是該表完整主鍵的物件，欄位恰依 build-db（如 vocabulary 為 `{kind,code}`，language 為 `{code}`，card 為 `{id}`）。
loader 必須依有效投影解析穩定鍵，驗目標存在、未停用／未退役、具所屬契約要求的有效採納與來源；
不能只驗 SQL FK 或借任意 confirmed 決定。沒有 active 欄的表，依其正式採納鏈／投影狀態判斷，不自造欄位。
各 kind 的直接有效實體引用須完整列入 dependencies，重複引用只列一次；review_context、evidence 與
observations 中僅作歷史核對的引用依當時背景驗回，不要求其來源版次永遠是現行成員。詞彙原值增加、label／譯名修訂而
目標仍為同概念同鍵時，引用者不需續版。採納概念不可用相同永久鍵偷換，見 §3。

只有真正依賴內容的條件才比相關內容：source_ref 驗 exact 字串，symbol_basis 驗基底與參數，
candidates_hash 驗競爭集合，特殊名稱的 name_basis_hash 驗名稱集合；不一律比整筆 record_hash。
predecessor 仍須精確 hash，因其用途是驗續版鏈。歷史解析依核對時 context 可重播；
本次解析到的完整 record_hash／decision／來源都由工具寫入 F1，不回填 authored 要人重簽。

### 2.1 決定、成員與 hash

decision 恰有 `id,state,scope,category,policy_id,membership_hash,members,sample_ids,note`。
state 固定 confirmed、scope 固定 batch；一筆也是一成員 batch。其他 state 不得進入入口，候選留 authored 外。
決定不保存製作者／核對者姓名、時間或精度；loader 不驗帳號名單。note 可省略或為空字串。

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
本封套是維護者全筆採納收據，loader 驗確認者在上述名單內，不能只驗名字非空；
工具不得捏造確認事件，規格文件、候選頻率、來源頁或模型審核均不等於維護者確認。
此檢查只屬本契約兩個入口，不修改商品等其他區域共用的決定模型，不設委託模式。
本版不沿用表記 approved_rules 或模板長尾 approved_policy 例外。
身分／商品／其他詞彙／舊修訂的 confirmed 決定，即使有 authored source，均不能代簽本筆內容。

### 2.2 來源與 freshness

evidence 為排序去重的 `{source_ref,role}` 陣列；source_ref 沿
[translation-contract §2](translation-contract.md#2-當前資料入口) 六欄，parser/locator/text_hash 指向凍結投影的 exact 字串。
圖片人工核對可改用 `{image_ref,role}`，image_ref 恰為
`{batch_id,source_version_id,raw_hash,printing_id,face_id}`，須驗圖像 descriptor/raw hash 及版次面關聯。
兩種 ref 恰擇一；role 非空，不將圖片 hash 當作文字 hash。format 1 的批次列入 review_context.source_batches；format 2 由本次建置來源集合提供。
純自撰的 code、別名或介面配置可 evidence=[]；format 1 仍須依賴與核對收據，format 2 不含這些欄位；聲稱官方原值／名稱／記號者必有來源。

每個實際使用的 source version 都驗 batch/descriptor/receipt/raw 閉包、parser 程式與設定 pin、locator 和 exact bytes。
人工依賴解析到的有效版本須驗完整 decision/member/hash 與來源，但 hash 改變本身不使引用者 stale。
版次依賴重驗當前 registry 的相關身分、區域、面與卡號。核對時 context 可重播不代表新輸入必然適用；
本次相關原值、symbol_basis、name_basis_hash 或 §5 候選集合改變時，按各 kind 的條件停止選用／重新採納。
新 raw/parser 只要能從當次凍結輸入驗回相同的相關 exact 內容，詞彙映射與 §4.3 名稱可機械重驗，不要求重簽；
不能據此忽略真正相關的規則依據、身分或來源變化。
無論是否重用，都須可驗原 recipe 與原來源；不能只換 recipe 名。

詞彙原值映射只授權已核對的 `(kind,region,lang,raw exact bytes)`，可用於本次其他相同原值的觀測，
但每次都要從當次凍結觀測驗明 exact 相同；不能延伸到新 spelling、被切半的 trait 或新類別。
來源頁不再存在於本次批次不等於概念被刪除；若該概念與所用映射仍可從釘住證據驗回，穩定 code 可保留。
route/default 仍依各自完整候選集合重驗；rules-name 依 §4.3 的名稱內容集合與面身分重驗，不把版次數量當名稱變更。

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
重建只投影當次有效值，採納歷史留 authored；公開快照僅 current＋previous，不承諾 CDN 歷史下載。覆寫不得跨元件修改 registry、配號、翻譯或 public schema。
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
format 2 的直接引用由工具自動收集，不能借 glossary、inactive／撤回列或任意同名 code。
首批 special_kind 是自撰標記定義，raw_mappings=[]；定義標籤不冒稱官方來源，不假造一條 raw binding 來通過驗證。

format 2 的 special_kinds 仍必填；欄位缺漏是格式錯誤，不由 loader 默補舊資料。

每個 raw 保留原欄位的 exact source_ref，type 的 locator 仍指完整 card_type／info/Card Type，不用切片字串假造來源。
基本卡種＋標記的解讀由完整 value 的適用採納核對，exact 來源證據本身不等於分類核可。
有效且 active 的映射中，相同 kind/region/lang/raw 必只有一組 `(code,special_kinds)`，
同 code 卻標記不同、重複同一原值但借不同 ref，亦拒絕；來源證據可另放 evidence，不建立多條衝突映射。
已驗相同原值的當次觀測只重用這一組，不通用 split、trim 或猜新拼法；未知原值拒絕並列缺項，不產生暫碼。
建置／更新工具列出未知原值與候選，修正當前映射後自動重驗，不以點擊收據作門檻。

停用的歷史映射不參與選用，但不可重用其 code 給另一概念。空陣列允許純介面 enum，但不能假稱已涵蓋官方原值。
來源的未知符號（含 `-`）如何投影 null 須有欄位 recipe，不把它自動採為職業或稀有度 code。
稀有度／premium 的拆解是釘住 recipe 的來源投影，不能讓 raw_mappings 改寫 premium；未知組合不猜。
技術預設 P3 分開篩選基礎 rarity 與 premium，顯示可組合；複合顯示別名不建立第二份稀有度真值。

label 只保存一個基底原文／自撰標籤；其他語言的選詞放在同一記錄的 `value.translations`（`lang,text,origin,low_confidence`，不得為 ja），建置時寫成 vocabulary 的 label 翻譯。
若 trait 同時是 glossary 概念，只有明示同概念關係才可使用既有選詞，不能因字串相同合併概念。
已核可譯名的數位／社群來源與 machine 標示沿翻譯契約，不重新要求逐卡確認。

language.code 沿 Lang；fallback_order 不含自身、未知語言或重複項。
它是完整依序嘗試清單，不遞迴串接其他語言的清單；ja→en 與 en→ja 可並存，不因互相備援誤判為循環。ja/en 清單不得含 zh-Hant；
最終基底回退亦不得繞過這項限制，基底為繁中時改用穩定 code。
使用者 2026-10-01 核可繁中介面依序回退日文、英文，完整 fallback_order 與末端呈現見 §8.1；
語言配置可直接修改當前值，不改卡面地區與卡文來源。

### 4.2 搜尋別名

kind→目標表的白名單沿 build-db §15；code 必存在且有效，keyword/stamp 尚未啟用時不得用同名 vocabulary 冒充。
text 非空、lang 已登錄，同 subject 只有一條鏈；不同目標可有同一別名，多義回全體並讓使用者選，不任取第一。
canonical code 仍先 exact 查找，正規化只作用別名查找，不修改卡號、構築名稱、存檔原文或 canonical code。

技術預設 P5 的算法為先 NFKC、再 casefold；不 trim／合併空白、不折假名、不去標點。
Unicode 資料版本隨 normalizer 實作／配置釘住，不使用系統未明示的版本。

normalizer 恰為 `{version,program_revision,code_path,code_hash,config,config_hash}`，釘可重現的版控實作與 canonical 配置。
loader 重算 normalized，不能信 caller 傳字串；同次查找索引只接受建置配置指定的同一 normalizer pin。
改版本需對全部有效 aliases 重算並逐項續版／採納，不能把舊 normalized 配上新版本。
自動由官方原名推導的 alias 可依既有 DB 契約無 decision，但必須有獨立釘住的推導 recipe 與來源；
本入口所有人工別名都必須 confirmed，不能省略決定冒充自動推導。本版不新增搜尋 grammar 或 query_alias。

### 4.3 特殊構築名稱

role 限 collab/treated_as；一般 primary 仍從同區官方 current 名稱推導，無 current 但所有觀測名稱 exact 相同亦可推導，
有不同名稱則留未定，不藉特殊名稱封套偷選 primary。names 每項語言必與 region 相符（jp→ja，en→en）。
陣列按 canonical bytes 排序，重建後的 exact 名稱不得重複；它是此 face/region/role 的完整特殊名稱集合。

identity_ref 是 `{face_id,card_id}`，引用有效面身分的穩定鍵，實際版本與決定由工具解析並記 F1。
observations 是排序唯一的 `{printing_id,face_id,source_ref}` 陣列，完整列**採納當時**該 face/region 的名稱來源；
它保存核對證據，不是永久固定的版次成員集合。printing 必同 card、同區且包含此面。
name_basis_hash=H(全部 observations 重建的 `{lang,text_hash}` 排序唯一集合)，text_hash 驗 exact UTF-8；
同名再錄不重複計入，hash 相同仍比 exact bytes，不把碰撞當同名。

每次建置機械重建該面該區目前全部可用名稱觀測，完整驗來源及面對應，再與受審 exact 名稱集合比較。
新增／移走同名版次、換來源版本但名稱不變，不需重新採納；不改寫舊 observations，當次證據寫 F1。
來源名稱集合改變、面／card／region 對應改變，或已採納特殊關係的相關規則依據不再適用才使關係 stale；
來源缺漏則列無法驗證，不能把缺資料當名稱沒變。沒有同區版次也不能保留該區的有效人工關聯。
規則依據另列 evidence，不因名稱共現就認定 treated_as。

各 name 重建後沿既有 `(region,official_name exact)` 產 rules_name，全部特殊關聯 decision 指本決定。
新增／移除其中一名也要對完整 names 集合續版，不借另一 role 繞過完整集合核對。
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
`{printing_id,card_id,region,card_no,card_no_state,variant_key,identity_ref}`；identity_ref 恰為
`{record_key,record_hash,decision_id}`，指有效 printing 身分記錄。candidates_hash=H(完整 candidates)。
核對範圍由 review_context 的完整 registry 與凍結來源重建，不接受 caller 自選清單；新增 variant 亦使舊決定 stale。
即使選中者未變，省略競爭者也不能沿用舊採納。

預設覆寫的 candidates 是本次展示範圍內**同卡同區全部 printing_id** 的排序唯一陣列，
candidates_hash=H(candidates)。每次重建完整集合並驗各成員的當前歸屬；選中者必仍同卡、同區、可展示。
集合增減、目標被移到別卡／別區或不再可展示才需重新採納；不能省略競爭者或以 caller 自選清單驗過。
Decklog 不可用不等於不可展示；同一版次的無關來源修訂不改候選集合，也不使覆寫失效。

不保存 selection_basis 或重複的 scope_hash，不將 selector_version、一般版 policy_hash 或程式版號當 freshness 條件。
它們只留於核對背景與本次 F1 作可重建追溯；人工選擇是「此卡此區預設這一版」，與機械 selector 怎麼算分開。
一般版清單核可、selector 修正、無關卡包新增都不要求重簽。preview 轉正式時重驗同卡同區集合與目標可展示性，
集合相同即可沿用；若本卡新增候選則 stale，不只因展示範圍的名稱改變就失效。
人工 override 可以選特殊版，不必偽造「一般」證據；投影 method=override，不標 earliest_general。

一般唯一 official 路由仍自動推導，不能為每卡造冗餘 route 決定。
同號 route 首次採納後，若續版／撤回將改已發布 route 的 printing 目標，必須在
identity-repair 的完整路由交易中提供前後件與永久入口比較；本入口不能單獨通過發布。
未發布候選也要驗原 route 不被另一 active key／alias 劫持。
舊 alias 的展平、merge/split/reassign、改號與 provisional corrected 不走本入口。
預設覆寫撤回則回現行機械 selector，只改 card 區域預設，不更新 printing URL 或 owner。

## 6. 卡文記號與三語文案

text_symbol_adoption.subject 為 `{id}`；value 恰為
`{code,parameter_schema,keyword_id,spellings,source_localization}`。
id 為人工首次指定的永久非空 ID，code 符合 Code；二者全域一對一且續版不改。
parameter_schema／spellings 沿 [snapshot-transport §3.2](snapshot-transport.md#32-公開參數宣告)
與 [snapshot-format 的 Spelling](snapshot-format.md#2-公開表完整欄位與玩家用途)，不另定 regex 或參數值域。
spellings 非空、排序唯一，語言已登錄；literal 拼接 prefix/suffix 必非空且 parameter_name=null。
uint/variable 必引用啟用的參數。必須消耗完整 token，保留 raw 及前導零；多義或未知回原記號，不任選 symbol。
keyword_id 可 null；非 null 須引用已採納 keyword，不能因拼法相似自動建立機制關係。

source_localization 恰為 `{lang,name,tooltip,copy_pattern}`，後三項為 §4 的 TextValue，語言須都等於 lang；
它是單一基底文案（官方來源引用或自撰說明），不是另一份三語翻譯。
記號封套不存 localization_refs，也不在 dependencies 釘翻譯選詞版本。
所有文案與翻譯的 placeholder 必依 parameter_schema 驗證，不能引入新參數或藏執行語言。
三語缺項如實缺譯，不能先填未採納文字湊滿三語；32 是候選觀測數，非格式上限或已核可名單。

**記號譯文**使用 translation format 2 的 symbol_localization_choice：
data 恰為 `{symbol_id,lang,symbol_basis,value,concept_evidence}`，品質欄位在 record。
symbol_basis 為 `{code,parameter_schema_hash,source_localization_hash}`，由當前基底自動計算防錯配；
value=null 為撤回，否則為 `{name,tooltip,copy_pattern}`，各值沿 glossary 的 value union。
concept_evidence 以 name/tooltip/copy_pattern 為鍵；origin 為整筆來源類別，混合機器內容時為 machine，
只有三欄皆為有效官方來源才可 official。此彙整不抹去各 source_ref 的 provider。
選擇鍵為 `(symbol_id,lang)`，直接修改；不存成員 hash／approval／adoption_no。
同語言不另以 choice 覆蓋基底 source_localization；base 改變須重驗 params 與 basis，不把舊文字套新記號。
不增 glossary_term、公開 owner 或 FieldTranslation enum；缺譯回原記號，低信心沿翻譯呈現，
來源損壞仍失敗。locale 文案改字不需新增 symbol 定義的核可；記號定義本身仍走原入口。

## 7. 預設版次與一般版分類

build-db §15 已有順序：同卡同區 → confirmed override → card.home_set 內的一般候選 →
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
| override | fresh confirmed 覆寫，目標在本次展示集合 |
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
| P0 | 採獨立入口、精確成員與只增續版，依 §2–§3；共享核對背景與穩定依賴不造成無關續版的連鎖重簽 |
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
政策核可與逐筆資料收據分開；這兩項已無待決問題，但核可不等於 loader 已實作或正式資料已全部採納。

固定英文 code、官方原值映射、各個記號拼法與文案、實際搜尋別名與特殊構築關係仍需資料採納。
候選的頻次不是核可；萃取修正後須重產 trait 清單，不能採用被切成半截的複合特性。
已進行的術語逐條確認、繁中來源、數位優先與社群參考不在此重開政策問題。

## 9. 自動反例與 legacy 驗收

這是未來 loader 的驗收規格，**不是已執行測試或突變數量**。
保留能驗錯配、引用及 fallback 的反例；vocabulary／language format 2 不測已移除的收據鏈，資料 PR 不要求逐 guard 定向突變。
正常 baseline 須通過、反例須以該原因失敗，才算 killed；不把 unrelated FK 失敗或語法錯當攔到約束。
production 採納／觀測數、合成案例、實跑 mutants 分開報，未知未測不填零通過。

| 編號 | 單一反例／定向突變 | 必要結果 |
| --- | --- | --- |
| C01 | 缺 index／缺分片／未索引／symlink／跨入口／未知欄／重複 YAML key；各移除一個 guard | 各自拒絕，啟用空集合只認明示空 index |
| C02 | 一筆 value 改一字／新增成員／改 evidence／換共享 review_context 卻留舊 hash | 三層重算檢出，不能沿用舊決定 |
| C03 | confirmed 的 category／policy 換成合法但不適用的其他類別 | 拒絕；即使有 authored source 也不放行 |
| C04 | 借同 kind 別筆的 decision／漏一成員／多一成員／重複 member | 精確集合驗證拒絕 |
| C05 | proposed／sampled／空核對者／空時間／checked 少一筆／假 approved_policy | 各自拒絕；本入口 confirmed 全筆 |
| C06 | 刪 raw／改 parser pin／改 locator／改 text_hash／錯 image face | 來源驗證失敗，不回讀 latest |
| C07 | 前件 hash 錯／decision 錯／分叉／缺號／停用後重開第 1 版 | 各自拒絕；合法停用與恢復新版本通過 |
| C08 | 詞彙補 raw mapping／label 修訂／目標停用或不存在／依賴自循環 | 前兩者引用別名仍有效、不需重簽，後兩者拒絕；有效版本的採納仍完整驗證 |
| C09 | 相同 raw 跨 kind 借映射／新 spelling 沿舊決定／完整 trait 的 ref 卻填半截 raw | 不匹配；同 kind/region/lang exact 原值重用基例可通過 |
| C10 | 同一原值兩 code／改 symbol code／譯名改字後重配 code | 衝突或穩定身分檢查拒絕 |
| C11 | alias 假 keyword 父列／錯語言／未存在目標 | 各自拒絕，不能用 vocabulary 冒充 |
| C12 | normalized 不可重算／混 normalizer pins／略過 canonical 優先／多義任選 | 前兩拒絕，後兩檢出錯誤解析 |
| C13 | language 自回退／未知目標／重複 fallback／ja 或 en 回 zh-Hant／繁中先英後日；ja→en 與 en→ja 並存 | 前四項拒絕，第五項違反核可順序；互相備援可通過且不遞迴，UI 配置不改卡文 |
| C14 | 同名再錄新增／來源換版但 exact 名稱相同／名稱改字／面重配／跨區／缺來源／剝括號 | 前兩者機械驗過仍有效，其餘拒絕或 stale；不合併 card 或 primary |
| C15 | symbol 多餘尾字／非 ASCII 數字／越界 uint／x 當 X／Q 擴成變數 | 各自不匹配或拒絕，raw 可 roundtrip |
| C16 | choice 改提示文字／撤回／未採納／錯 symbol_basis／新參數／缺譯硬填 | 修訂自動選用、不重簽 symbol；撤回或 basis 不符回原記號且不挑舊 choice；非法採納／參數拒絕 |
| C17 | route 候選漏 variant／新增競爭者／target 錯號／provisional／跨區撞號 | 拒絕或 stale，不看 UI 語言選勝者 |
| C18 | default 跨 card／跨 region／目標不可展示／漏同卡候選／candidates_hash 錯／僅 selector 或 policy hash 變 | 前五項拒絕或 stale，末項覆寫仍有效；Decklog unavailable 可展示基例不誤擋 |
| C19 | route 續版改已公開目標卻無 repair／撤回刪舊入口 | 發布拒絕；default 改選不能改 URL |
| C20 | 未知 premium／signed／frame／stamp 當 false，或漏背面 | 各自不能 earliest_general，依既定未知狀態降級 |
| C21 | 已知 premium／signed／stamp／特殊框／SL/SP/SSP/UR 或英版同類／PR/Promo/「-」算一般；BR/SR/GR/LG 與英版同類、加工未知 | 前六類各自排除一般候選；一般 rarity 加工未知可 candidate_general，不冒稱 earliest_general；僅非白名單者仍有 fallback |
| C22 | 無日期覆寫卻不沿商品 day／覆寫 month、year、unknown 卻回商品 day／商品 month 卻補 day／跨區日期／同日反向 ID | 各自檢出；兩商品日分別 2019、2022、無覆寫且晚者 ID 較小時，仍選 2019 |
| C23 | 只驗勝出者，忽略未知競爭者／忽略 home_set／忽略 override | 各自檢出 method 或選擇錯誤 |
| C24 | 先濾 JP 再驗 EN 壞分片／半筆失敗仍提交 DB | 全入口失敗且交易回滾 |
| C26 | type 標記缺定義／未採納／停用／重複／非 type 帶標記／漏直接依賴／同 raw 同 code 異標記 | 各自拒絕；JP／EN 完整原值及正確基本卡種＋標記通過 |
| C25 | YAML 只換排版／輸入檔順序改／新卡包僅新增無關卡／重建程式或背景更新但相關內容相同 | canonical 決定不變；詞彙與 default override 保持有效，F1 記新實際輸入 |

驗收須另覆蓋合法的新採納、完整續版、撤回／恢復、literal/uint/variable、雙面與多區互不污染。
依 §7／§8.1 已核可政策固化預期，再以 production 凍結來源驗證覆蓋；候選清單、合成成功與政策核可都不等於正式資料可發布。
