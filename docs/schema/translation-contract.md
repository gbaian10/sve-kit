# 翻譯、模板與當前資料契約

本契約保留清冊、句型比對、固定字與參數定義、譯文、術語、卡名、風味、加粗、新卡自動套用及未匹配清單。
2026-10-04 起，翻譯資料採可直接修訂的當前值；Git 保存修改歷史，退回使用 git revert。
不再用決定封套、收據、成員雜湊、只增不改、歷史重播、首輪抽查、雙模型或手動合併前檢查作載入門檻。
本文件定義目標格式；程式、資料、建置投影與 reader 必須配套切換，不表示舊 loader 已能讀新格式。

## 1. 來源與顯示原則

繁中以 JP 原文為主；日英已確認同卡且適用時共用 JP 譯文，已知差異及英文獨有欄位才用 EN 來源。
跨區卡片 ID 對應仍須人工確認，不能靠同名或移除卡號後綴推算。
EN 身分已確認而文字尚未核對時，沿 `shared_jp_unchecked` 顯示並標示；核對完成用 `shared_jp`，已知 divergence 不適用。
卡名來源順序及資格見[數位名字規則](digital-name-policy.md)，風味見[風味契約](flavor-translation.md)。

每筆資料只用 `origin: official|project|machine` 與 `low_confidence: Bool` 描述來源類別及信心，
不保存誰點哪個按鈕、模型互審結果或私人證據 hash。machine 經審閱仍是 machine。
結構及適用性通過的低信心譯文直接顯示「待校對」，可切回原文；旗標不是禁止入庫或發布的人工關卡。
缺譯、無法匹配或缺必要參數者回原文並列清單，不把半段原文混入完整繁中譯文。
來源損壞、錯 owner、錯型別及衝突引用屬錯誤，不能用低信心旗標掩蓋。

## 2. 當前資料入口

沿用 `authored/translations/`，分片按卡包或共用類別歸檔，單檔小於 1 MiB；序號只是檔名，不是修訂鏈。
沿 authored-layout §1 的嚴格 YAML、未知欄位拒絕、禁止 symlink／跳脫路徑及全入口完整索引；
filing_key 為 `[A-Za-z0-9_-]+`，sequence 為三位以上十進位字串，記錄按 record_key 排序。
空集合明示 includes/inventories 空映射，缺 index 不當成空資料。

| 檔案 | 完整頂層欄位 |
| --- | --- |
| `translations/index.yaml` | `translation_authored_format: 2, kind: translation_index, includes, inventories` |
| `translations/{glossary,templates,overrides}/<filing_key>/<sequence>.yaml` | `translation_authored_format: 2, kind: translation_shard, records` |
| `translations/template-sources/<sequence>.yaml` | 清冊 format 3，依[當前清冊契約](template-source-replay.md) |

includes／inventories 均是 authored 相對路徑到解析後 canonical JSON SHA-256 的映射，由工具更新。
它們只檢查檔案完整性，不是核可證明；index 不釘自身或同 PR 未來的 commit。
新 index 過渡期間可同時索引舊、新分片，各檔版本明示；兩個 reader 都須能辨識整個入口的版本與檔案閉包。
舊分片只供轉換與過渡，不得把「舊資料曾被修改」當作拒讀新格式的理由。

record 完整欄位為 `{record_key,kind,data,origin,low_confidence,note}`。
record_key 是下表選擇鍵前加 kind 的 canonical JSON 陣列字串；全入口唯一，不再包含 adoption_no 或 revision。
note 可空，僅寫簡短資料理由，不參與 ID 或決定狀態。來源、owner、參數等結構欄位不是審查欄位。
沒有 decisions、default_decision_id、members、sample_ids、delegation、adoption_review 或 predecessor。
同一選擇鍵直接修改當前資料；null 的含義按各 kind 明定，不能以最後讀入的重複鍵覆蓋。

| kind | 選擇鍵 | data 完整欄位 |
| --- | --- | --- |
| glossary_term | id | `id,category,concept_key,source_ref,source_span,authored_source_ja,missing_source_reason` |
| glossary_choice | term_id,lang | `term_id,lang,value,concept_evidence,source_claim` |
| vocabulary_choice | vocabulary_kind,vocabulary_code,lang | `vocabulary_kind,vocabulary_code,lang,value,concept_evidence,source_claim` |
| glossary_emphasis_choice | term_id | `term_id,value` |
| symbol_localization_choice | symbol_id,lang | `symbol_id,lang,symbol_basis,value,concept_evidence`，完整值依[記號文案契約](catalog-route-adoption.md#6-卡文記號與三語文案) |
| sentence_template | id | `id,inventory_id,source_span,source_lang,normalizer_version,semantic_variant,parameter_schema,content_hash,supersedes_id` |
| template_translation | template_id,lang | `template_id,lang,text` |
| template_translation_candidate | source_kind,candidate_id,lang | `source_kind,candidate_id,lang,text,inventory_ids,reasons`，未啟用原稿依 §2.1 |
| template_translation_variant | template_id,lang,variant_key | `template_id,lang,variant_key,text` |
| glossary_choice_variant | term_id,lang,variant_key | `term_id,lang,variant_key,value,concept_evidence,source_claim` |
| context_assignment | owner,field,ordinal | `owner,field,ordinal,source_hash,variant,concept_key,reason` |
| card_name_concept | subject | `subject,term_id,source_ref,reason`；subject 恰為 `{card_id,face_id,source_lang,source_hash}`，不帶歷史 identity_basis |
| template_match | context_key | `context_key,source_hash,matches` |
| translation_override | context_key,lang | `context_key,lang,action,templates,terms,reason`，action=suppress/pin/default |
| source_exception | card_id,region,scope | `card_id,region,scope,basis,reason`；region=en、scope=rules/name/all、basis=divergence/default_jp |

glossary 欄位依[術語契約](glossary-adoption.md)。context_key 恰為 `{source_unit_id,variant}`；
matches 為 null（回自動比對）或 `{template_id,source_span,params}` 陣列，須完整且無歧義。
context_assignment 的 concept_key 可 null；非 default variant 須非空理由。card_name_concept 的 term_id 可 null，表示撤回指派。
owner／field／ordinal 的組合及來源雜湊由本次有效身分與原文自動核對，不再要求歷史背景在 base main 的祖先。
同名不代表同概念。region-reviews 中的 region_text_review／region_divergence 同時控制 DSL／機制，
保留原格式與 build-db §5 的決定及 freshness 檢查，不轉成本節 record；index 過渡讀取不得忽略它們。
counterpart 仍須兩側精確原文及有效同卡／面關係。

translation_override 的 pin 以 templates=`[{template_id,lang,variant_key}]`、
terms=`[{term_id,lang,variant_key}]` 指明可重用的當前譯文／選詞；variant_key=default 指一般 record，
其餘指對應的 template_translation_variant／glossary_choice_variant。suppress/default 時兩陣列為空。
模板／語言／term 必須與該 context 的實際依賴一致，不允許加入與來源無關的選詞或逐卡自由全文。

variant_key 是穩定具名 Code（`[a-z][a-z0-9_-]*`），候選不能命名 default；兩種候選各沿原 kind 的
來源、參數、語言、origin／low_confidence 檢查，glossary 候選 value 不得 null。
它只是可修改的替代文字，不是 semantic_variant、修訂序號或核可狀態；普通渲染永遠不任取候選。
可直接修字；移除仍被引用的候選失敗。pin 是明示選詞例外，不從不同譯字推論不同概念或重配模板 ID。
舊 pin 選用的文字即使不是最新值，轉換時也只保留仍被引用的具名候選及實際來源，維持相同渲染結果；
不保留全部歷史譯本或採納鏈。無法無損對應者列清單，不把 pin 悄悄改成 default。

source_exception 的 divergence 只在本次存在該卡／區／scope 的 fresh、未解 confirmed divergence 時有效；
default_jp 撤回例外並按預設重算，不造不存在的 JP 來源。英文獨有仍由 fresh confirmed_none 與 as_of 推導，
不另寫 en_only、不藉翻譯欄位授予跨區資格。

source_ref 保留既有 `{store_id,batch_id,source_version_id,parser,locator,text_hash}`，只定位來源資料；
store_id 是可攜邏輯識別，實體路徑由執行端設定。parser 是本次支援的解析器，locator 為 JSON Pointer，
text_hash 對定位到的完整 UTF-8 字串計算；span 使用 Unicode code point 半開區間。
不把本機檔名、對話紀錄、點擊時間或私人網頁 hash 塞進來源欄位。

### 2.1 無法綁定 slot 的候選原稿

`template_translation_candidate` 保存尚無完整參數定義或無法無損綁定的專案譯文草稿。
沿用 format 2 的六欄 record，origin 限 project/machine，low_confidence 保留原值；
它始終未啟用，不因低信心旗標為 false 而成為可渲染譯文，也不能用 template_translation_variant 代替。

路徑固定為 `authored/translations/templates/template_translation_candidate/<sequence>.yaml`，
沿共用 includes 索引、可修改分片及單檔小於 1 MiB 的限制；索引 hash 仍由工具更新。
record_key 為 `["template_translation_candidate",source_kind,candidate_id,lang]` 的 canonical JSON 字串。
text、reasons、note 不參與身分；同鍵原稿有不同文字時拒絕，不按檔序選取。

| data 欄位 | 型別與含義 |
| --- | --- |
| source_kind | effect/flavor |
| candidate_id | 非空的原稿穩定識別，例如效果舊 T ID 或風味 exact ID；不是已驗證的模板 ID，不要求存在正式 definition |
| lang | 沿既有 Lang 型別的目標語言 |
| text | 非空 UTF-8 譯文草稿字串，保留最終原稿字元；不解析匿名 N/X 或套用可渲染譯文的 slot 語法 |
| inventory_ids | 按 ID 排序、唯一的當前清冊 entry ID 陣列；來源確實缺失時可空，並在 reasons 記原因，不偽造定位 |
| reasons | 排序、唯一且非空的原因代號陣列，每項符合 ASCII `[a-z][a-z0-9_]*` |

data 恰含上表六欄；record 及分片沿既有封閉結構、鍵唯一、安全路徑與引用檢查。
所列 inventory_ids 必須存在；完整建置仍驗當前清冊的來源閉包，不能因資料是候選就跳過壞來源。
原稿只存我們撰寫／生成的譯文，不保存官方原文、JP normalized、完整來源欄位、私人路徑、核可 hash 或事件。
來源文字由清冊的 source_ref／locator 取得，不另抄進 text 或 note。

候選不加入 active targets、bindings、renderer、pin 選用或公開翻譯投影，也不計為機械有效／已翻譯覆蓋。
統一處理清單保留 candidate ID、各原因及受影響卡片數；來源未知時影響數記未知，不當成零。
日後能綁定時，以普通資料 PR 改為有效定義與 template_translation，通過原有參數／來源驗證後才可渲染。
不新增核可、收據、修訂鏈或不可變機制。

## 3. 清冊與定義

清冊是所有來源欄位及句型位置的完整盤點，保留 body、reminder、token_header、layout、name、label 與 flavor 角色。
缺來源、unknown、空字串與空白不能混為不存在；每個來源片段要有去向。
固定文字和參數 schema 共同描述模板；辨識不能只把所有數字叫 N、所有引號內容叫 X 後當成同一句。

保留既有模板 ID 與內容指紋；semantic payload 為 level/source_lang/normalized_text/normalizer_version/semantic_variant/parameter_schema。
content_hash 由工具對實際 payload 計算，用來抓錯配與碰撞，不含 note、來源收據或翻譯審查。
純譯文、說明修正不換模板 ID；固定字、參數型別或語義分支真的改變時，形成另一個模板定義，並重算其使用位置。
舊 T/C＋normalized UTF-8 SHA-256 前 10 hex 的 ID 不重配；短指紋撞異內容就失敗。
新 ID 為 T（sentence）／C（clause）＋六欄 payload hash 前 16 hex；新鍵碰撞逐次加 2 碼至 64，
保留已配結果。完整 hash 相同仍比完整 payload，撞異 bytes 就失敗；來源換頁而 payload 相同可重用。
這是語義身分規則，不要求保存全部舊清冊／採納分片或遍歷 Git 祖先；未使用的舊定義可從當前檔移除。

同一位置至多匹配一個有效定義；多個候選列歧義，不依檔名或 ID 大小任選。
supersedes_id 只記定義替代關係，不需收據；不得形成環，未載入的 legacy 父 ID 不建立假的 DB 外鍵。
所有句型包括只出現一次者仍用模板；子句組合無環，不另造每張卡自由翻譯覆寫。

## 4. 參數、譯文與自動套用

### 4.1 位置與完整覆蓋

binding 由每次建置產生。source_span 固定 `{role,segments,anchor}`，segments 是排序的 `{start,end}` 非空陣列，按**原文 Unicode code point** 半開區間；role 如 §3，anchor 為所屬 body binding ordinal 或 null。一個 body 可以含多段，不必用一個連續 span 包住句中提示。binding.ordinal 按第一個 segment.start 由 0 連續排序；body/layout/header 的 anchor=null，reminder 只有句中者指向 body。渲染輸出 anchored reminder 後跳過其獨立位置，避免重複；layout 依來源順序保留。每個 context 的所有頂層 segments 恰分割完整原文，不重疊、不漏字；exact 空字串用零 binding，unknown 不轉空字串。

| 原文情況 | 分段與渲染規則 | 獨立反例 |
| --- | --- | --- |
| 換行、空白、空行 | 先保留 CRLF/LF 與行首尾空白成 layout；layout 使用固定新模板、literal 參數只允許該來源的空白序列，原樣輸出 | trim 後遺漏空行、CRLF 只覆蓋 LF 均失敗 |
| 只有提示文的行 | 分類確認後整段（含括號）使用新 reminder 模板；沒有 body 仍必有 binding | 舊分類未給 T ID 就丟掉該行失敗 |
| 句中提示 | body.segments 排除提示，提示另綁 reminder 並 anchor 到 body；譯文先輸出完整 body，再按原文順序附提示，原文欄保持原位置 | 用單一 body span 包住提示再重疊 reminder 失敗 |
| 同模板有／無提示 | 同一 body payload，提示屬來源 binding，不灌進 body 模板；未知括號仍作 body 規則或報匹配失敗 | 把條件括號一律當提示，或因有提示覆寫 body payload 失敗 |
| token 定義標頭 | 保留標頭完整 span，新 token_header 模板釘名稱、職業／特性／種類、可選費用／攻防的 slot；不同形狀用不同 schema | 只翻後續能力而略掉標頭、未知形狀硬套固定費用失敗 |
| NFKC 改變字元 | trace 保存每一正規化片段對應的 raw 區間與前後 bytes；normalizer 僅做明定轉換，不做逐字可逆的假設 | 把全形標點正規化後位置當原文 offset 失敗 |
| 裸 N／『X』與原文字面字母 | 舊 normalized 原樣留；schema 以 normalized 的位置指明「哪個 N/X 是哪個 slot」，其餘為 literal | 全域 replace N、把字面 N 當數字 slot 失敗 |

layout 不含待翻語義，可機械生成固定模板；reminder/token_header 是獨立句型，須像 body 一樣提供其譯本。版次欄位與 section 的完整覆蓋各自核對，不跨欄偷接；提示分類有疑義則保留原文／失敗清單，不擅自取語義等義。

上表的一般卡文分段不套到 flavor。其非空整欄恰一個 flavor span，包含換行／空白／括號，不再分 layout 或 reminder；專屬 exact recipe 與零參數新 ID 依 [風味文字契約](flavor-translation.md#1-來源與清冊)。其餘欄位的完整覆蓋與分段規則不變。

### 4.2 參數 schema 與驗回來源

當前解析結果逐位置保留角色、引用身分、raw 拼寫、span 與 pending；
逐位置 schema／raw roundtrip 及定義六欄 payload 都要驗，同 hash 仍比完整 bytes。

parameter_schema 固定 `{format:1,slots:[...]}`；每個 slot 恰為 `{name,type,occurrences,reference_kind,min,max}`。name 為 `[a-z][a-z0-9_]*`，唯一；type=uint/literal/reference，reference_kind 為 card/term/vocabulary 或 null，uint 的 min/max 為安全非負整數，其餘為 null。occurrences 是 normalized_text 的不重疊 `{start,end}` 陣列；同 slot 多次出現值須一致。slot 陣列按首次位置排序。舊 `『X』` 的 slot 只覆蓋中間 X，左右引號仍是 literal；params 的 uint/literal 是整數／字串，reference 為 `{kind:card,id}`、`{kind:term,id}` 或 `{kind:vocabulary,vocabulary_kind,vocabulary_code}`，須符合宣告種類與 FK。數字正規化前的 raw 字串與數值分開保存；reference 綁永久概念 ID，不能靠顯示名猜同卡。literal 僅給明定的格式片段（例如 layout），不得包住整句外文冒充翻譯。

術語引用 slot 可覆蓋 normalized_text 中原樣保留的名稱，不必換成佔位符；該模板的名稱固定、大括號仍 literal，舊指紋不變。只以唯一 exact 當前概念及 category 綁定；詞庫改動由當前清冊重產檢查歧義。四條能力門檻規則（combo／lesson／necrocharge／spell_chain）只將數字作 uint slot，已採納名稱只供 context 檢查、仍為 literal；不借此核可中文譯名或新增名稱引用。

數量／增減幅度的 schema 界值為 0..9007199254740991，序數為 1..9007199254740991；此為安全整數技術界值。原樣十進位／safe unsigned／序數非零等匹配條件仍須驗證；正負號留 literal，只以非負幅度綁 slot。完整欄位依賴（例如選項引導與全部標號）按各當前來源核對，不從同一舊 ID 的其他成員借證據。

例（全為自撰）：原文 `N測試２` 重建 normalized=`N測試N`。唯一 uint slot `count` 的 occurrences=[{start:3,end:4}]、min=0、max=9007199254740991；第一個 N 是 literal。若另一成員在位置 0 也是數字，兩者不能共用此 schema，分新模板；不能依候選分組的字串相同合併。

「驗回來源」是兩個獨立檢查，不能只把譯文重跑同一個 normalizer：

1. trace 由原文直接取每個 segment 的 raw bytes，連同 layout/reminder/header 按來源位置重組，須逐 byte 等於來源 UTF-8（不依翻譯結果猜回原文）。trace 的 NFKC 對照可多對多；prefix/字數變化不移動原文座標。
2. 以當前分類／normalizer 重算選中 body 等角色，對 literal 段與 slot 的位置、型別、原值、規則分類逐項匹配，產生 normalized 須等於 模板 payload。額外／遺漏參數、未分類片段、無法解析的引用都失敗。第一項防漏原文，第二項防「任意片段都包成 literal」假通過。

### 4.3 當前譯文與自動套用

template_translation.data 恰為 `{template_id,lang,text}`，origin／low_confidence 在 record 外層。
text 使用 `{{slot_name}}`，literal 的反斜線與左右大括號以反斜線跳脫；禁止未知 slot、
未閉合括號與未使用的必要 slot，不支援運算式。slot 可重排，重複使用依 schema，
不以裸 N 作替換語法。缺必要目標語詞庫時整個 context 回原文，列 missing_term_translation。

首版在 sentence 層翻譯，包括只出現一次者；C ID 保留盤點，template_component 暫不啟用拼接。
已有 component 仍驗無環及父子來源一致性；日後啟用拼接須先補參數映射契約，不能將未實作能力當成可用。

新增卡包由工具產清冊、套用當前模板及詞庫、渲染譯文。只需一個資料 PR 和一張
低信心／新句型／歧義／未匹配／缺譯清單，不需先完成固定數量人工樣本。
低信心由輸入旗標及實際使用的譯文／詞庫依賴作 OR 傳播；機器來源不自動等於低信心。
加粗由概念引用及位置產生，不能事後搜尋同中文字串猜位置。

## 5. 概念選詞與數位證據

glossary 保留 `term:<concept_key>` 永久概念，category=keyword/ability/trait/rule_term/card_name。
選詞以 `(term_id,lang)` 為唯一當前值；vocabulary 以 `(kind,code,lang)` 為唯一值。
同名、同譯不合併概念；職業／卡種代碼與泛稱、資源名依[術語契約](glossary-adoption.md)分開。
加粗可直接改值，讓所有依賴者重新產生；不需改核可 hash。

官方來源可使用 source_ref 重建文字，專案／機器詞用自寫 text；official 不能單憑相似字串判定。
來源類別 official 與公開 authority 分開：數位官方名稱仍是 digital_official，整段效果機器翻譯仍 unofficial。
既有 source_claim 只保留可公開的作品／URL／簡短未驗理由，不記本機證據；來源不明不得硬升 official。

## 6. 建置、穩定 ID 與失效

### 6.1 當前資料的建置

一般 reader 只驗版本、型別、唯一鍵、索引及引用。建置使用本次來源與有效資料，
產生 context、use、binding、translation、selection；完整當前清冊重產一次並比對檔案，
不執行舊 producer、不重算舊採納歷史，也不保存新的核可證明。
每次建置的同一組來源可共用解析結果；缺資料不得借最新官網或另一台機器的私人檔補洞。
來源歸檔完整性及本次 build inputs 的追溯仍依[來源歸檔](source-archive.md)，不能代入假 decision。

context_assignment／卡名指派只對自己的 owner 與 exact 原文有效；來源改變時不搬到新字串。
模板／術語修改後重算所有相依位置，受影響清單由工具列出；有效原文沒有變而只改 note 不造成語義變更。
同字串出現在 name、effect、flavor 時，use 仍驗自己的欄位資格，不能只憑共用 context 借用譯文。

### 6.2 穩定 ID

H 是 build-db §14 canonical-json-v1 的完整 SHA-256（64 小寫 hex）。命中仍比完整輸入；
完整 hash 撞異 payload 即停止，不換舊鍵。排除時間、私人路徑、核可者與收據。

| 物件 | ID 與 hash 輸入 |
| --- | --- |
| context | `ctx:`＋H(`{recipe:context-v1,source_unit_id,semantic_variant}`) |
| use | `use:`＋H(`{recipe:use-v1,owner,field,ordinal,context_id}`) |
| binding | `bind:`＋H(`{recipe:binding-v1,context_id,ordinal,template_id,params,source_span}`) |
| face_semantics | `sem:`＋H(`{recipe:semantics-v1,face_id,region,rule_text,rule_sections,normalizer_version}`)；exact 內容及有序 sections，語義引用另進 rule bundle |

owner 為具名單一物件，恰有 kind 與 §6.3 該種類列出的鍵；不以未命名陣列或另一組別名表示。
模板語義 ID 依 §3，term ID 依 §5。
翻譯推導改用 render-v2：對 `{recipe,context_id,target_lang,dependency_key,text,origin,authority,low_confidence}`
的 canonical JSON 取完整 SHA-256，ID 為 `tr:`＋64 hex；recipe 恰為 `render-v2`。
dependency_key 是按種類及鍵排序的本次模板定義、譯文、詞庫／加粗與 binding 的語義值，
包含選中的 variant_key 及其實際文字／來源類別，不含 note、adoption_no 或核可 hash。revision 取完整 hash 前 13 hex 作 52-bit 非負整數；同鍵撞不同完整 hash 必拒絕。
這是產物的更新／去重鍵，不是人工資料的不可變版本機制；人工一般譯文只有當前 `(template_id,lang)`，具名候選是 §2 的明示可修改替代值。

### 6.3 來源 owner

use 恰有一個有效 owner，context 原文等於該 owner 欄位；section/action_label 才有非 null ordinal。
face_revision 用 name/effect/section；printing_face 另可用 flavor，且只用自己已知的 printed 文字。
qa_version 用 question/answer，cr_clause 用 effect，vocabulary／商品用 label；keyword 可用 label/effect/action_label。

| owner.kind／鍵 | 合法 field |
| --- | --- |
| face_revision／revision_id | name、effect、section |
| printing_face／printing_id,face_id | name、effect、flavor、section（只用該版已知 printed 字串） |
| qa_version／qa_version_id；cr_clause／cr_clause_id | 前者 question/answer；後者 effect（條文） |
| vocabulary／vocabulary_kind,vocabulary_code；product_family／product_family_id；product／product_id | label |
| keyword／keyword_id | label、effect（definition）、action_label |

未實作的 owner 不因有 FK 就宣稱支援。相同模板與參數預設同翻法；§2 的 pin 明示選用可重用替代譯詞，真正歧義才另用語義分支。

### 6.4 更新與失效

本次清冊、來源原文、參數及引用檢查通過才可輸出；壞結構／引用使建置失敗，未匹配／缺譯則回原文列清單。
舊的輸出不能冒充新來源翻譯。每次建立新 DB，不原地修改已輸出快照；公開保留窗口仍為 latest＋previous。
舊資料及舊程式留 Git 歷史，不承諾新 reader 能重播所有舊環境。

## 7. 跨區與官方對照

### 7.1 非翻譯區域核對保留原入口

region_text_review／region_divergence 會控制 DSL／機制，保留 region-reviews 的 format 1，
不使用 §2 新 record。分片為 `{translation_authored_format:1,kind:translation_shard,default_decision_id,records,decisions}`；
record 為 `{record_key,kind,filing_key,data,evidence}`，同片單 kind／單決定，decisions 恰含 default 所指 batch。
evidence 為排序唯一 `{source_ref,role}`；decision 沿 authored-layout §2 的完整封套與精確成員 hash。
record_key 是 `[kind,...選擇鍵,adoption_no]` 的 canonical JSON 字串，review 選擇鍵為
`card_id,region,jp_hash,region_hash`，divergence 為 `card_id,region,field_scope`。
adoption_no 從 1 連續只增，predecessor 首筆 null，其後為上一筆 `{record_key,record_hash,decision_id}`；
拒絕分叉、缺號、錯前件，舊分片及索引 hash 不改。本段只保留原門檻，不擴及翻譯 format 2。

region_text_review.data 恰為 `{card_id,region,jp_hash,region_hash,hash_recipe,faces,display_checks,state,checked_at,adoption_no,predecessor}`。region=en，hash_recipe=rule-bundle-v2；faces 每項 `{face_id,jp_revision_id,region_revision_id,jp_semantic_id,region_semantic_id}`，涵蓋全部面、按 face_id 排序。jp_hash/region_hash 投影 source_jp_hash/source_region_hash；跨語 hash 不要求相等，state=aligned/divergent（pending 留候選）。aligned 需 sampled/confirmed 語義對照決定，兩端身分仍另須 confirmed。

display_checks 每項 `{face_id,field,jp_ordinal,region_ordinal,jp_ref,region_ref,state,reason,counterpart}`，field=name/effect/section/flavor，ref 為 §2 source_ref。預設同面同欄對照由工具列出，採納以雙端精確 hash 的 batch 決定釘住；只有非同欄／跨 ordinal 等例外逐項人工指定。state=aligned/divergent，aligned 的 reason 可空，其他必填；counterpart 為布林，true 僅在 aligned 且該對照已採納時允許。這項決定是 counterpart 原文配對的人工採納，不是把每個推導選用寫進 git。

region_divergence.data 為 `{card_id,region,field_scope,reason,effect,override_dsl_id,resolved,jp_ref,region_ref,adoption_no,predecessor}`，沿 build-db 的 rules/name/all、manual/override_dsl enum；override_dsl 時 ID 必填且可驗。confirmed 決定釘兩端來源與差異，resolved=true 也須新 confirmed 決定與解除證據。投影當前 region_divergence，舊版本留封套。

### 7.2 逐 owner 的顯示選用

共用 JP 的 EN 顯示沿 §1，JP owner 的 use 不能硬綁成 EN 原文。
官方 counterpart 依有效 display_checks 逐 owner 產生 translation，由 FieldTranslation 直接引用，
不占共用 selection；origin=official、authority=sve_official、bindings/terms 空。
其 context/source_hash 是顯示來源 owner 的原文，text 是已核對的 counterpart；
render-v2 依賴包含兩端 owner/ref 及有效核對，不能讓同 context 的第三張卡借到官英資格。

shared_jp_unchecked 只限 target_lang=zh-Hant、EN 接收端、已確認同卡／面、JP 譯文 fresh、
兩端來源可驗且無相關已知 divergence。它不建立 aligned review，不放行 DSL／機制。
日英段落數不同時，全部 section ordinal 及包含它們的 effect 全文均不得提前共用；
回 EN 原文或有效 own_source，不依 ordinal 猜配。name 等獨立合法欄不受牽連。
完成 display_checks 且適用後才轉 shared_jp；basis 改變不改共用譯文的內容／origin。

已知 divergence 按 scope 隔離；不知道不等於英文獨有或已核對一致。
來源／身分變動時重驗兩端及該 owner；counterpart 失效回有效本站詞或原文，不影響無關 owner。
語義 bundle 的 face_semantics／revision_semantics／semantic_reference、表記核可與來源更正仍依
[ADR-0013](../adr/0013-rule-bundle-migration.md)及 build-db §14；exact 字句對照不能由規則等義代替。

## 8. 公開投影與自動檢查

公開仍只輸出選中的文字、來源類別／authority、適用基礎與必要呈現資料，不輸出內部清冊或核可紀錄。
新格式的 low_confidence 必須從 producer 傳到 reader，UI 顯示「待校對」且提供原文；
啟用時同步 snapshot-format、機器 Schema、tuple、Python／TS reader 及能力版本，不能只改資料版號或冒用 reviewed 表示人工看過。
加粗／原文位置的公開承載依[術語契約 §6](glossary-adoption.md#6-公開快照影響與最小擴充提案)，內部支援不代表舊公開 tuple 已有該欄。

CI 自動驗格式、key／ID 唯一、參數對齊、引用與 owner、來源覆蓋，並用固定輸入重新產生當前清冊比檔案。
官方輸入存既有永久私有 testdata repo，公開 repo 只留 commit 與檔案 hash，依既有 deploy-key 流程取得；
可信任 CI 缺資料失敗，fork 明示只跑合成檢查，不能宣稱完整資料驗收。
CI 不讀個人檔案、不即時爬站；報告列 ID／原因，不在 log、cache 或 artifact 放官方全文。
保留能抓錯的行為反例；資料 PR 不要求每個 guard 都做一次定向突變，不另加人工合併前命令。

## 9. 舊格式轉換

format 1 的 decisions、membership、sampled/confirmed、policy／approval／review queue、adoption_no／predecessor
及 inventory 1/2 的 producer／expected／replay_context 僅供讀出舊有效值；新資料不再寫入這些機制。
轉換保留模板、參數、有效選詞、加粗、來源類別、排除及撤回的語義；不能用舊核可紀錄造新核可事件。
官方數位卡名繼續由當前規則與來源產生。機器草稿通過格式／引用檢查即可入庫，原低信心或仍有語意疑義者標旗標。
匿名參數無法無損對齊者依 §2.1 保留候選原稿並列清單；不將錯誤正文混入可渲染集合。
共同 index 切換先讓兩個 reader 認新格式，再分批換 glossary／模板資料；各批可獨立驗證，不改寫 Git 歷史。
