# 翻譯、模板與跨區採納契約

本文件細化 [build-db §9／§14](build-db.md#9-翻譯句型與術語) 與 [authored-layout §6](authored-layout.md#6-模板翻譯與語義例外)。人工採納與工具推導分開，binding/use 每次建置重建，不新增永久物件庫。以下為技術契約，尚未實作匯入器；§1 另外記錄使用者已核可的顯示與首輪抽查政策。例子全部自撰，不是真實卡文或採納紀錄。術語的委託採納、未驗來源主張及可修訂加粗由 [術語採納擴充](glossary-adoption.md) 細化；該擴充為 2026-10-02 協調者在維護者委託下的決定，不記為使用者親自核可。

## 1. 政策與狀態

**使用者已核可（2026-10-01）**：繁中翻譯以日文卡文為來源；日英身分已確認同卡一律用日文。只有兩區版本明顯不同、或英文版獨有的卡才用英文。其後同日核可的顯示與抽查方式如下。

**既定限制**：identity confirmed 與 region_text_review aligned 分開；正式 aligned 共用須核對目前兩端、確認面對應且相關 scope 無未解 divergence；下述提前顯示是明示例外，不放行 DSL／機制或官方 counterpart。翻譯最低 sampled/confirmed；機器翻譯審過仍保留 machine。官方數位卡名按獨立核可的名字政策或自己有效真人同卡證據取詞，兩者共用 sv1→svwb；same_character 或同字串仍不足以認定同概念。名字政策及 same_name 瀏覽政策各自成檔核可，依 [數位名字政策](digital-name-policy.md)，不把規則連結當真人同卡證據。本站效果翻譯為 unofficial，SVE 官方 counterpart 則是有證據的官方原文選用。

**技術處理**：缺 JP 來源只列缺來源，不能據此聲稱英文獨有。英文來源例外須有 confirmed_none 的查核範圍/as_of，或 confirmed divergence 所指的受影響欄位；未受影響欄沿 JP。這是核可來源政策的實作判定，不冒稱使用者另行核可了每個判定細節。

**使用者已核可（2026-10-01，追加決定）**：日英確認同卡後、文字核對完成前，EN 卡面先顯示 JP 繁中並加「日英文字尚未核對」；核對完成且適用時標示消失。公開以 `FieldTranslation.basis=shared_jp_unchecked` 表示，與既有已核對 `shared_jp` 分開，詳 §7.1；已知 divergence 不使用提前顯示例外。

首輪依模板出現次數由高到低翻譯，不按卡包排批，長尾後補。頻率在釘住來源清單內按每個版次面欄位的有效觀測計數，相同頁歷史抓取不重複計入；同頻按模板 ID 排序。每個模板由一個模型翻、另一個模型審，保留兩者身分、精確譯本 hash 與分歧；使用者抽查前約 100 個高頻模板，並查看全部模型分歧項。實際抽查集合以使用者看過的 template ID/revision 記錄，不把「約 100」硬寫成已看滿 100 或固定百分比。

使用者抽查範圍是**整體**的高頻前約 100 個模板與全部模型分歧，不要求每個長尾批次另有人類樣本。首輪完成模型互審與實際人工抽查後，以 sampled batch 採納精確成員；後續無分歧的長尾批次可依 §2 的政策採納例外，引用首輪抽查決定與核可政策收據，完整檢查後採納為 confirmed batch。沒有實際抽查的批次不得標 sampled，也不能把裸 model_reviewed 當採納。模型分歧項仍須使用者處理，未處理者留候選。

政策採納的成員一律 origin=machine；使用者親自看過並認可的機器譯文也仍標 machine，這是沿用既有規則，不因審閱改成 project。

風味文字採整段對譯與寬鬆的譯本採納門檻；符合專屬 exact 邊界的模板定義可依有效政策逐筆機械全查後採納，不必每包人工抽查。首輪譯本的實際抽查數量與集合由維護者決定，契約合併不等於已完成抽查或逐筆採納。效果文字維持同語意同譯、單卡自由譯文先不做。專屬來源、整段新模板、定義政策採納及譯本收據見 [風味文字契約](flavor-translation.md)；效果的前約 100 個高頻模板政策不移作風味收據。

## 2. 人工採納入口

只有人工決定及其核可政策下的採納進 authored：模板定義與譯本、概念與譯詞、同字異義指派、模板匹配例外、跨區核對／counterpart 採納、來源例外及撤回。context/use/binding、渲染全文、selection **不進 authored**。模板或術語改字後工具重算，無須逐卡重新簽核；人看的是政策要求的抽查與失敗清單。

| 路徑（相對 authored） | 完整頂層欄位 |
| --- | --- |
| `translations/index.yaml` | `translation_authored_format: 1, kind: translation_index, includes, inventories` |
| `translations/<area>/<filing_key>/<sequence>.yaml` | `translation_authored_format: 1, kind: translation_shard, default_decision_id, records, decisions` |
| `translations/template-sources/<sequence>.yaml` | `template_source_format: 2, kind: template_source_inventory, recipes, replay_context, entries`；完整封閉格式見[歷史清冊重算契約](template-source-replay.md) |
| `template-parameter-rules/<policy_id>.policy.yaml`／`.approval.yaml` | 獨立 `template_parameter_rule_policy`／`template_parameter_rule_approval` 封套；完整欄位見[辨識政策契約](template-parameter-policy.md#1-獨立入口配對與不可變)，不進 translations 的 includes |

area 為 `templates/glossary/overrides/region-reviews`。filing_key 為 `[A-Za-z0-9_-]+`，只歸檔；sequence 為只增三位以上十進位序號。includes/inventories 各映射上述分片／清冊到完整解析內容的 canonical hash。沿 [authored-layout §1／§2](authored-layout.md#2-分片批次決定與來源) 的 YAML 邊界、單檔 <1 MiB／512 KiB 目標、安全路徑及全入口驗證；拒絕缺檔、未索引分片、symlink、hash 不符與未知欄位。新分片驗妥後原子更新 index，舊分片不改。啟用時空集合需明示空映射，缺 index 不是空集合。

下述 translation_shard 的 record 恰為 `{record_key,kind,filing_key,data,evidence}`。同檔一種 kind、一個 default_decision_id，decisions 恰含該決定。候選與模型審查留在 authored 外；入口只接受達該 kind 門檻的 sampled/confirmed 記錄，不讓 proposed 永久占鍵。不可變物件的 record_key 為 `[kind,...主鍵]` 的 canonical JSON 字串，採納選擇另加 adoption_no，見下表。辨識 policy／approval 是獨立入口，不混進 record kind 清單或此分片決定。

| area | kind／主鍵 | 採納內容 |
| --- | --- | --- |
| templates | sentence_template／id | §3 的不可變模板與來源 |
| templates | template_translation／template_id,lang,revision | §4 的模板譯本；reviewed 且 sampled/confirmed |
| glossary | glossary_term／id | §5 的永久概念 |
| glossary | glossary_choice／term_id,lang,adoption_no | 選定譯詞的不可變決定；投影當前 glossary_translation |
| glossary | glossary_emphasis_choice／term_id,adoption_no | rule_term 的可修訂加粗；其餘由型別推導，見術語採納擴充 §5 |
| glossary | vocabulary_choice／vocabulary_kind,vocabulary_code,lang,adoption_no | 介面詞彙標籤的選詞，不假造 glossary_term |
| glossary | symbol_localization_choice／symbol_id,lang,adoption_no | 記號三語文案的選詞，建置自動選有效譯本，缺譯回原記號 |
| overrides | context_assignment／owner,field,ordinal,adoption_no | 同字異義的概念／variant 指派 |
| overrides | card_name_concept／subject,adoption_no | 卡／面與 exact 名稱到已採納 card_name 概念的人工例外關聯；subject 與身分基準依卡名關聯契約 |
| overrides | template_match／context_key,adoption_no | 精確來源下模板拆分或匹配例外；不是渲染全文 |
| overrides | translation_override／context_key,lang,adoption_no | 撤回或指定模板譯本／術語 choice；不得逐卡任意改同語義句 |
| overrides | source_exception／card_id,region,scope,adoption_no | 繁中 EN 來源例外；confirmed |
| region-reviews | region_text_review／card_id,region,jp_hash,region_hash,adoption_no | §7 的跨區規則與顯示核對，含 counterpart 採納 |
| region-reviews | region_divergence／card_id,region,field_scope,adoption_no | §7 的明確差異／解除；confirmed |

translation_shard 的決定封套一律 **scope=batch**，單筆也是一成員 batch，無 record-scope 決定 ID 的另一配法。用 authored-layout §2 的 canonical recipe 計完整 record 的 `record_hash`（含 evidence），members 為排序的 `[record_key,record_hash]`，membership_hash 為該陣列 hash，decision.id=`d:`＋完整 membership hash 的 64 hex。這裡的 record_hash 就是既有文獻的 semantic hash，不另創 recipe。sampled 需真人、時間、非空樣本子集；confirmed 的 checked 集合覆蓋全體，除下述模板政策例外及 [glossary 委託收據](glossary-adoption.md#4-實際人工或委託採納) 外仍須人工核對；glossary 委託決定記實際協調者與事件、不計維護者親自審閱。policy_id 指精確政策，新增／改內容必換決定。

**長尾模板譯本的政策採納例外**：比照 [authored-layout §9.5](authored-layout.md#95-核可規則confirmed-封套與人工確認) 的 approved_rules，僅 template_translation 可用 `adoption_review.mode=approved_policy`。adoption_review 恰為 `{mode,policy,initial_sample_decisions}`，mode=human/approved_policy；human 的 policy=null、initial_sample_decisions=[]。approved_policy 的 policy 為 `{policy_id,authored_revision,path,hash,approval_receipt_hash}`，釘完整 commit、repo 相對路徑、canonical 政策內容 hash 與核可收據 hash；initial_sample_decisions 是非空、排序唯一的 `{decision_id,membership_hash}` 陣列，引用實際完成首輪高頻抽查的 human sampled 決定。政策與收據須能驗明 §1 的抽查集合、無分歧長尾適用範圍與 machine 標示，不以本文件的核可敘述代替真實首輪收據；相關來源、分片及收據 bytes 全部納入 F1。

政策檔、核可收據、authored 內唯一索引與首輪實際抽查的封閉格式依 [模板採納政策契約](translation-policy.md)。政策 ID 在 authored 唯一對到有索引／雜湊的不可變政策與收據，建置設定只作複核，不能取代有效採納。風味定義的政策例外須符合 [風味文字契約](flavor-translation.md)、具備授權定義 kind 的真實政策收據及完整 loader 支援，不由本格式的存在自動授權。

兩種 mode 分檔；approved_policy 同檔使用相同 policy 與首輪決定引用，decision.policy_id 必須等於 policy.policy_id。每個成員都須 origin=machine、不同模型對該 exact text_hash 的互審 result=agreed，且完整通過來源、slot、譯本與政策範圍檢查。決定 state=confirmed，sample_ids 恰為全體 checked record_key，表示政策機械全查，**不是此次逐筆人工審閱**；reviewed_by／reviewed_at／reviewed_precision 沿核可收據的人名與時間，note 明示「政策核可」，authored_by／authored_at 記本次套用工具與時間。缺首輪實際抽查、政策／收據 hash 不符或有模型分歧者不得走此例外；分歧項分到 human 批次、由使用者處理並列入實際 sample_ids。後續新增／改字重做互審與當批決定，可在仍符合政策時引用同一首輪收據，不冒稱使用者看過新譯本。報告分開計 human_sampled_rows、approved_policy_rows 及待人工分歧，不把全體 checked 當真人樣本數。

**flavor 定義的政策採納例外**：僅符合 [風味整段契約 §4](flavor-translation.md#4-譯本政策與歸因) 的 sentence_template 可引用同一份風味政策機械全查後 confirmed；不改九欄 data，decision.policy_id 由 authored 內唯一、已索引且釘 hash 的政策檔與核可收據解析，正式位置依 [模板採納政策契約](translation-policy.md)；author source／decision_source 保留完整 commit、政策索引與檔案 bytes。F1 configuration 的五欄 pin 僅複核，不能取代 authored 的有效採納。這不使用譯本的 adoption_review 欄位，也不授權效果定義；初輪與政策格式／loader 支援要求仍須驗回。其餘 kind 仍按上述門檻。

帶 adoption_no 的 data 另含 `{adoption_no,predecessor}`，首筆為 1/null；後續連續只增、完整替代，predecessor 恰為 `{record_key,record_hash,decision_id}`。每一選擇鍵只有一條已採納鏈，拒絕分叉、缺號與錯前件。kind 自定的 null／撤回值才撤回，舊記錄與證據保留。工具依有效鏈推導結果，不拿檔案順序作優先序。

`evidence` 為 `{source_ref,role}` 去重陣列。source_ref 恰為 `{store_id,batch_id,source_version_id,parser,locator,text_hash}`：parser 為釘住程式與設定的 recipe ID，locator 是該 parser 完整 JSON 投影內的 JSON Pointer，text_hash 驗被定位字串的 exact UTF-8；不是任意可執行查詢。來源歸檔 batch/descriptor/receipt/raw、parser 程式／設定 hash 皆由建置輸入紀錄 F1 驗證。人工純決定可無 raw evidence，但所引用模板、概念、owner 與前件須完整可驗；聲稱官方來源的記錄不可空。author source_record/decision_source 釘完整 commit、index/分片 hash 與原證據，不在 authored 重抄官方原文。

記號的三語 name/tooltip/copy_pattern 沿本入口追加 `symbol_localization_choice`，完整欄位見 [記號文案技術契約](catalog-route-adoption.md#6-卡文記號與三語文案)。其來源、origin 與採納規則沿本契約，詞彙 label 繼續使用既有 vocabulary_choice；新增 kind 須實作完整 loader 驗證後才能載入，不代表三語文案已採納。建置依 symbol_id/lang 取有效 choice、驗 symbol_basis，文案改字不要求記號再簽一次；實際選用的精確 hash／決定留 F1。

## 3. 模板來源清冊與 ID

不新增物件庫。runtime 從正式清冊指向的**已封存卡頁**、釘住的 extractor/normalizer 重建內容，不讀研究草稿或 latest cache。清冊 recipes 每項 `{id,code_revision,code_path,code_hash,config,config_hash}`；code_path 是 repo 相對檔案，code_revision 為完整 commit，config 為 canonical JSON。缺正式實作或不能重現舊結果時停止遷入，不能拿未版控的腳本路徑作 runtime 依賴。

清冊 v2 按 [歷史清冊重算契約](template-source-replay.md) **以凍結語義版本重算並逐項比對輸出 hash**。
recipes 六欄不變，code_revision 是歷史 producer，code_path 指固定計算入口；當次執行版本另記 F1。
replay_context 釘語義 bindings、producer 環境、逐清冊風味背景與不可變六流 expected_outputs。
環境差異只記錄，凍結 bytes／完整輸出全等才能通過，來源／schema／payload／採納仍各自驗。
完整 Git 歷史與全部清冊只增不改，不修改舊 producer／expected 或忽略未被現行定義使用的 entry。
v2 格式支援不能代替 C+hash 能力；首次採納 gate 未完整通過前不寫真實清冊，發現正式 v1 另審遷移。

啟用辨識政策的分類／參數 recipe.config 明示 `recognition_policy=null` 或 `{policy_id,authored_revision,path,hash,approval_receipt_hash}` 五欄 pin，完整納入 config_hash；載體與驗證依[辨識政策契約 §4](template-parameter-policy.md#4-五欄-pin引用與-f1)。政策尚未被完整 loader 支援時只能留候選，不以開關或文字表態清除待審原因。歷史清冊的 config／pin 保留原值，不原地補欄或改標新 matcher commit。

術語辨識的 recipe config 另釘 references.glossary 的 authored_revision、index_hash 與全部分片 exact bytes hashes；升術語 pin 也須完整重播原位置比對，不能把新同名歧義改選另一概念當成功。辨識核可不是清冊、來源覆蓋、模板定義或譯本採納，四者各自驗收。

清冊 entries 每項 `{id,level,source_ref,line_ordinal,role,normalizer_id,normalized_hash,legacy_fingerprint}`。source_ref 定位完整欄位，line_ordinal 自 0，role=body/reminder/token_header/layout/name/label/flavor；各 role 的更細定位由 §4 的確定性分段及模板 source_span 給出。flavor 的行序 0 代表完整段落，限 printing_face.flavor，沿 [風味整段規則](flavor-translation.md#2-整段分段與清冊)。legacy_fingerprint 為舊 normalized 全 SHA-256，新 ID 為 null。完整來源與 recipe 足以重建 exact normalized，normalized_hash 驗證結果。多個來源產相同 ID 仍比完整內容，檢碰撞涵蓋所有歷史清冊，不能只看本批。

舊 `T`（sentence）／`C`（clause）＋SHA-256(normalized UTF-8) 前 10 hex 原樣保留。舊分類 recipe 為切行、trim、移出全形括號片段與 token 標頭，再 NFKC、`『…』`→`『X』`、數字串→`N`；此 recipe 只重現分類指紋，**不授權把所有括號當提醒而刪除**。人採納模板前仍要確認提示／規則區分與完整參數位置。

sentence_template.data 恰為 `{id,inventory_id,source_span,source_lang,normalizer_version,semantic_variant,parameter_schema,content_hash,supersedes_id}`；inventory_id 指清冊 entry 的 id，source_span 沿 §4，source_lang/level 與來源／entry 相符。normalized_text 從來源重建；完整 content_hash 仍沿 build-db §14 的六欄 `{level,source_lang,normalized_text,normalizer_version,semantic_variant,parameter_schema}`。第一次採納凍結全部內容，不能把舊 10 hex 指紋當完整內容 hash。

舊 ID 如能一致重建且具有唯一 slot schema，可直接凍結為可用模板；若同一舊 ID 成員的字面 N／slot 角色不同、語義需拆分，舊 ID 與原 normalized 留清冊，分支各採納新 ID 並 supersedes 舊 ID，不強行給舊分組一個假 schema。supersedes 可指清冊內尚未可翻譯的舊 ID；匯入 sentence_template FK 前須有其已核可完整 payload，未具備時保留清冊中的分叉關係作來源追溯，DB supersedes_id=null，不造假父列。

新模板 ID 用 T/C＋完整 payload hash 前 16 hex；碰撞只將新鍵逐次加 2 碼至 64，永久登錄配發結果，舊 ID 不變。相同 hash 仍比完整 bytes；舊 10 hex 撞異內容或完整 hash 撞異 bytes 均停止匯入。normalizer、參數 schema、語義或 normalized 變更需新 ID；supersedes 禁自指／循環，不自動繼承舊譯本。來源定位換頁但六欄相同可重用模板，追加來源證據不改模板內容。

## 4. 原文分段、參數與譯本

### 4.1 位置與完整覆蓋

binding 由每次建置產生。source_span 固定 `{role,segments,anchor}`，segments 是排序的 `{start,end}` 非空陣列，按**原文 Unicode code point** 半開區間；role 如 §3，anchor 為所屬 body binding ordinal 或 null。一個 body 可以含多段，不必用一個連續 span 包住句中提示。binding.ordinal 按第一個 segment.start 由 0 連續排序；body/layout/header 的 anchor=null，reminder 只有句中者指向 body。渲染輸出 anchored reminder 後跳過其獨立位置，避免重複；layout 依來源順序保留。每個 context 的所有頂層 segments 恰分割完整原文，不重疊、不漏字；exact 空字串用零 binding，unknown 不轉空字串。

| 原文情況 | 分段與渲染規則 | 獨立反例 |
| --- | --- | --- |
| 換行、空白、空行 | 先保留 CRLF/LF 與行首尾空白成 layout；layout 使用固定新模板、literal 參數只允許該來源的空白序列，原樣輸出 | trim 後遺漏空行、CRLF 只覆蓋 LF 均失敗 |
| 只有提示文的行 | 採納分類後整段（含括號）使用新 reminder 模板；沒有 body 仍必有 binding | 舊分類未給 T ID 就丟掉該行失敗 |
| 句中提示 | body.segments 排除提示，提示另綁 reminder 並 anchor 到 body；譯文先輸出完整 body，再按原文順序附提示，原文欄保持原位置 | 用單一 body span 包住提示再重疊 reminder 失敗 |
| 同模板有／無提示 | 同一 body payload，提示屬來源 binding，不灌進 body 模板；未知括號仍作 body 規則或報匹配失敗 | 把條件括號一律當提示，或因有提示覆寫 body payload 失敗 |
| token 定義標頭 | 保留標頭完整 span，新 token_header 模板釘名稱、職業／特性／種類、可選費用／攻防的 slot；不同形狀用不同 schema | 只翻後續能力而略掉標頭、未知形狀硬套固定費用失敗 |
| NFKC 改變字元 | trace 保存每一正規化片段對應的 raw 區間與前後 bytes；normalizer 僅做明定轉換，不做逐字可逆的假設 | 把全形標點正規化後位置當原文 offset 失敗 |
| 裸 N／『X』與原文字面字母 | 舊 normalized 原樣留；schema 以 normalized 的位置指明「哪個 N/X 是哪個 slot」，其餘為 literal | 全域 replace N、把字面 N 當數字 slot 失敗 |

layout 不含待翻語義，可機械生成固定模板；reminder/token_header 是新增句型，須像 body 一樣採納其譯本。版次欄位與 section 的完整覆蓋各自核對，不跨欄偷接；提示分類有疑義則保留原文／失敗清單，不擅自取語義等義。

上表的一般卡文分段不套到 flavor。其非空整欄恰一個 flavor span，包含換行／空白／括號，不再分 layout 或 reminder；專屬 exact recipe 與零參數新 ID 依 [風味文字契約](flavor-translation.md#3-獨立-recipe-與新-id)。其餘欄位的完整覆蓋與分段規則不變。

### 4.2 參數 schema 與驗回來源

清冊 v2 的 members／checkpoint 摘要逐位置保留角色、引用身分、raw 拼寫 hash、span 與 pending，
但不取代本節逐位置 schema／raw roundtrip 或定義六欄 payload 驗證；同 hash 仍比完整 bytes。

parameter_schema 固定 `{format:1,slots:[...]}`；每個 slot 恰為 `{name,type,occurrences,reference_kind,min,max}`。name 為 `[a-z][a-z0-9_]*`，唯一；type=uint/literal/reference，reference_kind 為 card/term/vocabulary 或 null，uint 的 min/max 為安全非負整數，其餘為 null。occurrences 是 normalized_text 的不重疊 `{start,end}` 陣列；同 slot 多次出現值須一致。slot 陣列按首次位置排序。舊 `『X』` 的 slot 只覆蓋中間 X，左右引號仍是 literal；params 的 uint/literal 是整數／字串，reference 為 `{kind:card,id}`、`{kind:term,id}` 或 `{kind:vocabulary,vocabulary_kind,vocabulary_code}`，須符合宣告種類與 FK。數字正規化前的 raw 字串與數值分開保存；reference 綁永久概念 ID，不能靠顯示名猜同卡。literal 僅給已核可的格式片段（例如 layout），不得包住整句外文冒充翻譯。

術語引用 slot 可覆蓋 normalized_text 中原樣保留的名稱，不必換成佔位符；該模板的名稱固定、大括號仍 literal，舊指紋不變。只以唯一 exact 已採納概念／category／record_hash 綁定，完整術語入口 pin 與升版重播依辨識政策契約。四條能力門檻規則（combo／lesson／necrocharge／spell_chain）只將數字作 uint slot，已採納名稱只供 context 檢查、仍為 literal；不借此核可中文譯名或新增名稱引用。

數量／增減幅度的 schema 界值為 0..9007199254740991，序數為 1..9007199254740991；此為協調者依授權採用的技術界值，不寫進辨識政策的語義授權。原樣十進位／safe unsigned／序數非零等匹配條件仍須釘住；正負號留 literal，只以非負幅度綁 slot。完整欄位依賴（例如選項引導與全部標號）按各來源重播，不從同一舊 ID 的其他成員借證據。

例（全為自撰）：原文 `N測試２` 重建 normalized=`N測試N`。唯一 uint slot `count` 的 occurrences=[{start:3,end:4}]、min=0、max=9007199254740991；第一個 N 是 literal。若另一成員在位置 0 也是數字，兩者不能共用此 schema，分新模板；不能依候選分組的字串相同合併。

「驗回來源」是兩個獨立檢查，不能只把譯文重跑同一個 normalizer：

1. trace 由原文直接取每個 segment 的 raw bytes，連同 layout/reminder/header 按來源位置重組，須逐 byte 等於來源 UTF-8（不依翻譯結果猜回原文）。trace 的 NFKC 對照可多對多；prefix/字數變化不移動原文座標。
2. 以釘住的分類／normalizer recipe 重算選中 body 等角色，對 literal 段與 slot 的位置、型別、原值、規則分類逐項匹配，產生 normalized 須等於 immutable payload。額外／遺漏參數、未分類片段、無法解析的引用都失敗。第一項防漏原文，第二項防「任意片段都包成 literal」假通過。

template_translation.data 為 `{template_id,lang,revision,text,origin,model_review,adoption_review}`，adoption_review 見 §2；revision 從 1 只增，origin=project/machine；入口只收已審且 sampled/confirmed 的譯本，投影 status=reviewed。text 使用 `{{slot_name}}`，literal 的反斜線與左右大括號以反斜線跳脫；禁止未知 slot、未閉合括號與未使用的必要 slot，不支援執行運算式。從 slot 的型別與已採納目標語選詞渲染，不用 normalized 中裸 N 當替換語法。

model_review 在 machine 時必填 `{translated_by,reviewed_by,reviewed_at,text_hash,result,resolution}`，記兩個不同模型及版本，text_hash 驗譯本 exact UTF-8；result=agreed/disputed。disputed 須 resolution={reviewed_by,reviewed_at,note} 記使用者處理，且該模板在 batch 的 sample_ids 中；agreed 的 resolution=null。非模型譯本為 null，人工作者仍由 decision 記錄。所有日期是真實事件，譯本改字須新 revision 並重做互審，不能沿用對另一 text_hash 的意見。

首版在 sentence 層翻譯，含只出現一次者；C ID 保留來源盤點，template_component 暫不啟用拼接。若已有 component 資料只作無環與父子來源一致性檢查，不參與渲染，子譯本修訂不影響父句選用。將來要啟用子句拼接，須先補參數映射／子譯本釘版契約；首版可直接採納完整句子譯本，不需等子句拼接實作；尚未翻到的長尾仍按缺譯處理。

## 5. 概念、選詞與數位證據

card／face 身分與 exact 名稱到 card_name term 的自動推導與人工例外關聯、預設選面、printed／current 區分及修復後重驗依 [卡名概念關聯契約](card-name-concepts.md)。該入口重用採納封套與 context_assignment，不改 glossary 的永久 key 或既有 data；官方名稱仍須逐 owner 驗數位同卡／同面證據，不能從共享 context 借資格。

digital_link／coverage 的獨立 authored 入口、續版與逐 owner 凍結名稱重驗另見[數位對應採納契約](digital-link-adoption.md)，尚待審核與實作；不修改本 translations 封套或既有 glossary 必填欄位。

glossary_term.data 為 `{id,category,concept_key,source_ref,source_span,authored_source_ja,missing_source_reason,adoption_review}`；有 frozen 日文欄位時用 source_ref／source_span exact 摘錄重建 source_ja，其餘兩個來源欄為 null；無 raw 的專案概念允許 ref／span 為 null，但名稱與理由必填。互斥模式與委託收據依 [術語採納擴充 §2／§4](glossary-adoption.md#2-專案概念可沒有-raw-locator)，不能用假 locator 或空字串。id=`term:`＋人工首次配發的 concept_key（ASCII `[a-z][a-z0-9_.-]*`）。key 以英文概念命名，如 `action.draw`，不以草稿流水號或原文字串當唯一鍵、不隨譯名重算；同字異義需不同 key。前綴用穩定大類，提案審核後可由獲維護者委託的協調者核可配發；借 EN 名命名不算採納英文。source_ref 不要求數位卡片的 name 欄，故原始詞只出現在效果文時也能登錄。

| 草稿分類 | 正式去向 |
| --- | --- |
| keyword、ability、tribe | glossary category keyword、ability、trait |
| verb、zone、other | glossary category rule_term；other 須逐概念核對，不機械確定語義 |
| class、card_type | 既有實體卡 vocabulary(kind=class/type) 的 label；不存在的職業／卡種與泛稱走 glossary rule_term，不增代碼，見術語採納擴充 §1 |
| 卡名候選 | glossary category card_name，與 SVE／數位同概念決定分開驗 |

glossary_choice.data 為 `{term_id,lang,value,origin,concept_evidence,source_claim,adoption_review,adoption_no,predecessor}`；vocabulary_choice 將 term_id 換成 `{vocabulary_kind,vocabulary_code}`。source_claim／adoption_review 依 [術語採納擴充 §3／§4](glossary-adoption.md#3-選詞的主張來源與-origin)；未驗出處先有效採納 project，另記主張來源，不新增半官方 origin。value 可為 null，或 `{kind:authored,text}` 或 `{kind:source,source_ref,span}`，span 為 `{start,end}` 或 null（取全字串）。parser 必須提供欄位語言，jp_ref 為 ja、target_ref 與 lang 相同；原文整欄 hash 加 exact span 足以驗摘錄，不要求摘錄等於整份 text_unit。origin 沿 build-db 的 official_svwb/official_sv1/project/community/machine。null value 是明示撤回；有效 glossary_choice 投影為每 `(term_id,lang)` 唯一 glossary_translation，保留其決定與來源；舊 choice 留 authored。vocabulary_choice 則推導 label 的翻譯與 FieldTranslation。

concept_evidence 是以下 tagged union 的陣列；官方 origin 至少一項且支持同一概念與 value 的 exact 譯詞，不能只證明兩詞曾在同一頁出現：

| kind | 其餘完整欄位與核對 |
| --- | --- |
| digital_name | `digital_face_id,sve_owner,jp_ref,target_ref,decision_id`；同概念採納連 SVE owner 與數位面，兩 ref 為其語言卡名 |
| effect_term | `jp_ref,jp_span,target_ref,target_span,concept_note`；對齊效果文中同一術語的摘錄，span 精確；concept_note 寫概念理由，不抄官方卡文 |
| dictionary_entry | `dictionary_kind,entry_key,jp_ref,target_ref,concept_note`；dictionary_kind=skill_names/tribe_names，entry_key 釘同一官方字典鍵，核對該 key 真正代表的概念 |

ref 均為 §2 source_ref。effect_term/dictionary_entry 不強制捏造 digital_face/name_unit_id，可承接效果文摘錄與官方詞彙字典的證據。既有草稿摘錄缺 raw 版本或精確位置時不能把草稿當官方證據；可先以 project 有效採納、主張來源另記，補到證據後追加下一個 adoption 升級，見術語採納擴充 §3。

數位 JSON 的**已封存批次是匯入前提**，本文件不宣稱 sv1/svwb 已有批次；實際 batch ID 到位、閉包驗過才可採納。不用 URL／latest API 回應補缺來源。卡名需 digital_card/face/text 與同概念最小閉包，術語字典僅需相應來源與 parser，不為字典建假數位卡。

參數所指卡名／術語沒有已採納目標語譯詞時，整個該 context 為缺譯並回原文，列 `missing_term_translation`；不把日文名悄悄嵌在繁中完整譯文中。無法對到 card 的引用名先作待確認的 term 候選，不能猜 card ID；已有原創譯詞並採納後可正常渲染。

## 6. 推導、穩定 ID 與修訂

### 6.1 每次建置的資料

清冊歷史以自身凍結語義與 inputs 驗回，當次 binding/use 另依有效 owner／來源重建。
其完整來源重讀、六流摘要、單次 F1 與用途映射依[歷史清冊重算契約](template-source-replay.md)，
不能用舊 root 代替當次用途適用性，也不把當次 owner 變動回寫成歷史 context。

工具依凍結原文、有效人工決定與釘住的推導 recipe，順序產生 context→use/binding→translation→selection。translation.source_hash 驗 context 的 exact 原文 UTF-8；translation_binding 完整列出該 context 的 binding 與精確 template_translation 三欄主鍵，template_id、lang、context 均須吻合。translation_term 列全部直接與參數概念，依賴鍵另釘所用 choice record_hash；不能只記 term_id 丟失選詞修訂。同一 exact source/variant 共用 context，同一模板／參數只用一種有效翻法；新語義需人工 context_assignment，不能每卡自由翻。

| 人工例外的 data（另加 adoption_no/predecessor） | 意義 |
| --- | --- |
| context_assignment: `owner,field,ordinal,source_hash,variant,concept_key,identity_basis,reason` | source_hash 精確釘原文；identity_basis 必填，歷史核對與當次適用性依 §6.1.1；variant=default 或已採納 Code，非 default 需同字異義理由；續版可改回 default |
| template_match: `context_key,source_hash,matches` | context_key 同下列 `{source_unit_id,variant}`，source_hash 必與該 unit 相符，避免不同語言同 bytes 撞鍵；matches 為 `{template_id,source_span,params}` 陣列；null 撤回例外回機械匹配，否則完整覆蓋且通過 §4；變更 schema／normalizer 需新 template_id |
| translation_override: `context_key,lang,action,template_revisions,term_choices,reason` | context_key={source_unit_id,variant}；action=suppress/pin/default，後兩陣列在 pin 指定 `{template_id,revision}`／`{term_id,choice_record_key,record_hash}`，其餘為空；只選已採納依賴，不存另寫全文 |

owner 原文變更使舊 context_assignment/source_hash 不匹配時，舊指派列失效、不搬到新字串；新原文無歧義則回 default 自動重建，有歧義才等新指派。無例外時選各模板最高已採納譯本與有效術語 choice；pin 是已採納的明示例外。普通機械匹配只選當前有效且可無歧義匹配的模板，superseded 模板保留來源歷史；新分叉未能唯一匹配即列人工失敗清單，不任取 ID 最大者。

名稱的概念推導以 `(來源語言,完整 exact 名稱)` 唯一對到已採納 card_name 概念；不需逐卡關聯紀錄。直接名字可另依有效 [數位名字政策](digital-name-policy.md) 取詞，無概念不阻擋該名字；概念／效果引用仍依 [卡名概念關聯契約 §3](card-name-concepts.md#3-名稱-owner-的預設綁定) 驗自己的語義。直接名稱依真人實看選詞、政策官名、必要的自己真人同卡官名、其他合法詞、原文的順序；單純譯名字串差異選勝出詞並報差異，不回原文。真正同字異義、錯來源／指派仍消歧或拒絕；sampled 非樣本不算本人實看，保持真實 origin。

**binding 每次建置推導，當次 DB 只放目前一組。** use 同樣重建。模板拆分、normalizer 修正、補登同字異義、商品／標籤原文更正，都在新建置以新依賴重算；舊 DB/快照不原地更新，新的 DB 不帶上一組 binding 的 translation_binding。`UQ(context_id,ordinal)` 與 owner/field/ordinal 唯一約束不變；不需新組序號或後續 DDL 才能改綁。

歷史人工譯本與決定仍保存；歷史**推導結果**由對應 F1 輸入、演算法版本與舊快照重現，不要求把相互衝突的全部歷史 binding 同時塞進單一當前 DB。當快照需歷史 face_revision 時，為該 exact source 各自推導合法 context/binding。未被本次輸出引用的舊生成譯文不載入當次 DB，也不進 git。

### 6.1.1 context_assignment 的不可變身分背景

`context_assignment` 沿 §2 的 overrides 入口、record 五欄、單 kind／單 decision 與實際人工門檻。
data **恰為** `{owner,field,ordinal,source_hash,variant,concept_key,identity_basis,reason,adoption_no,predecessor}`。
record_key 仍為 `["context_assignment",owner,field,ordinal,adoption_no]` 的 canonical JSON 字串；
選擇鍵仍為 `(owner,field,ordinal)`，不把背景版本放進永久鍵。variant／concept_key 與非空 reason 的
既有條件不變，回 default 仍是同一選擇鏈的下一筆，不是刪除歷史。

`identity_basis` **必填且不可為 null**，重用 [卡名概念關聯 §2](card-name-concepts.md#2-card_name_concept-採納格式)
的三欄封閉物件，不新增另一套 registry pin：

| 欄位 | 定義與拒絕條件 |
| --- | --- |
| authored_revision | 核對背景的完整 40 碼 Git commit；必須已存在且可讀。正式採納使用已合併 main 的 commit；讀取端另驗其等於消費端 authored_revision 或是它的祖先。不使用將被 squash 掉的功能分支 commit，也不能引用尚未寫出的自身分片 commit |
| registry_index_hash | 該 revision 的 `authored/ids/index.yaml` 完整解析內容 canonical Hash；不是目前磁碟檔或消費端目前 revision 的 hash |
| transition_index_hash | 同 revision 的 `authored/identity-transitions/index.yaml` canonical Hash；null 只表示該不可變樹沒有此檔，空 index 仍須釘 hash。非空入口須完整重播有效身分及其證據 |

新分片與 translations index 由消費端的 authored_revision 另釘；不把尚未產生的 record／index hash
反塞進 identity_basis，避免自我引用。以下檢查分層執行，不要求 PR 的消費端先合併：

- **讀取端**：每次建置以不可變 Git 歷史驗 basis 等於消費端 authored_revision 或是它的祖先；
  僅能讀到 blob 不算通過，同層分支背景須拒絕、不降成 stale。不讀可變 main ref，
  也不判斷這次是不是正式發布；開發與採納 PR 可以釘功能分支消費端。
- **採納 PR 的 CI／合併前檢查**：每筆新增或續版背景須是該 PR base main commit 的祖先（含本身）。
  base 由 CI 的完整歷史取得或檢查設定明示完整 40 碼 pin，不拿 PR head 或可變 ref 當基準。
  即使背景是消費端祖先，若它只存在功能分支、不在 base main 的歷史，仍拒絕採納，
  避免 squash 後正式建置無法重播。合成採納檢查測試提供自己的明示 base pin。
- **正式發布**：依既有發布流程在 main 上建置，消費端 pin 已合併 main；這是發布閘門，
  不是 loader 的規則。正式讀取端仍只驗上述不可變祖先關係。

每筆續版各保存實際核對時的背景；不得以首筆、末筆或本次
建置背景覆蓋其他歷史成員的值。欄位進完整 record_hash／membership_hash／decision_id，
前件的既有 hash 不改。它不授予新 owner／field、譯詞或官方來源資格。

**先驗全部歷史，再判本次適用。** 兩階段不得用同一個目前 registry 比對代替：

1. **歷史合法性**：從該成員的不可變 authored_revision 讀兩個 index、全部分片與決定，
   驗 canonical／exact bytes、永久身分與有效 transition；缺版本、symlink／非 regular blob、
   hash 不符或未支援的必要重播均建置失敗。已 superseded 或回 default 的指派也須驗。
   來源依 §2 evidence 的 frozen SourceRef 與其釘住的歷史 parser／設定重建，
   不跟目前磁碟上的執行期檔比；只改現在程式註解不能讓舊合法採納失效。
2. **卡面 owner 的歷史證明**：面修訂／印刷面依凍結卡頁、原樣地區／卡號、parser source_index
   及該背景的 source_face_map 共同核對永久 card／face，並驗該成員當時的 owner 與 exact 欄位。
   名字指派的 field=name、ordinal=null；至少一筆 evidence 定位該 owner 的完整名稱，
   text_hash 等於 source_hash，不能拿效果摘錄、另一張卡／另一面或數位卡名代替。
   face_revision 必須由其完整來源與 recipe 重建精確 revision_id；含 source_correction 時也須
   重播完整已採納修正及其證據，未支援時拒絕、不猜 raw 修訂 ID。printing_face 必須屬宣告
   printing／face，且證據是該 printing 自己凍結卡頁的完整名稱，不能借 current。此步不重建
   歷史勘誤覆蓋或 printed_text_state；本次 printed 狀態由第 3 步驗。完整 printing 身分 observation／跨區
   引用及相關 transition 的 frozen 閉包也須驗，不能只驗這個名字或 FK。
3. **當次適用性**：歷史成功後，只對有效鏈的末筆，依本次有效身分、owner 自己實際來源與
   source_hash 判適用；printing_face 另驗本次 printed_text_state 與該版自己的名稱，
   unknown／omitted 不借 current，也不沿歷史成功取得用途。不以歷史 raw observation 必須等於目前 registry observation 作第二道
   採納門檻。單純增加無關身分、更換建置背景或同名印刷頁其他欄更新，不要求重簽。
   若該 owner 的原文換字、revision owner 被新 ID 取代、原 card／face 合法退役或 printing
   已改配父卡／面，舊指派保留為歷史、列本次不適用原因，不把合法的歷史指派報成壞輸入。
   不搬到新 owner、字串、父卡或面；新來源無歧義回 default，有歧義才等新的合法指派。
   當次若仍引用有效舊 owner，仍按該 owner 自己的 exact 來源判適用，不因存在新修訂
   就把全部歷史用途一律停用。
   本次已宣告的 source／身分證據閉包缺失、矛盾或必要重播未支援仍須明示拒絕，不以 stale 掩蓋。

同名但效果改字造成新 face_revision ID 時，永久 card_name_concept 關聯仍依其契約重驗；
原 owner 的 context_assignment 不自動移給新修訂。相同 printing owner 的其他欄變動而名稱、
有效 card／face 及 printed 狀態不變時，既有名字指派仍可用。非卡面 owner 仍依 §6.3 的
自身來源與正式身分契約核對；registry pin 本身不能證明 QA、條文或標籤的 owner。
其他欄位同樣須定位該 owner 的完整欄位及其 ordinal；不能用名字的來源證明 effect／flavor／section。
未具備對應歷史 owner／欄位來源重播的 loader 必須拒絕，不因加了本欄就宣稱已支援那些 owner。

**F1 與審計**：歷史身分 index／全部分片、有效決定、transition 與全部 frozen observation
批次／descriptor／receipt／raw、歷史 parser 程式及設定都須保留可重播 pin，連同本次有效身分
及 owner 的實際 source uses 納入閉包。建置輸入須明示提供所有歷史背景所需的來源批次／recipes，
不能只帶最新 JP／EN 批次、用 live manifest 或 latest cache 補缺。每個 assignment 決定的
name_identity decision_source 只歸屬自己的背景；全部歷史來源仍留 source_record。
最後由 caller 獨立重播 expected uses 並作完整 InputRecord.verify；不能只靠 importer partial verify。

**相容與能力啟用**：本欄是 authored 指派的核對資料，不加 DB 欄或公開 snapshot 欄位，
translation_authored_format=1 的 index／分片封套、既有 glossary／card_name_concept 形狀不變。
context/use/render 的既有 ID recipe 不另加 identity_basis 參數；實際選用的指派 record_hash
仍依原依賴配方驗證，不另定義忽略採納修訂的捷徑。第一筆真實 assignment 之前，loader／
歷史重播、匯入審計及 offline 閉包必須全接入本欄；舊 loader 對新增欄位應嚴格拒絕。
新 loader 不接受缺欄或 null 的指派，也不得從消費端背景補值；只驗 frozen 來源不能代替身分背景。
合成 fixture 須改成明示 pin。目前沒有正式 assignment 需要遷移；若發現舊式正式資料，
先停止新採納、另審有當時核對依據的遷移，不能工具猜背景、改歷史分片或重算舊決定冒充真人確認。

以下均為後續實作的合成驗收，非本文件已執行測試；每次拒絕只改成功基例的一項條件：

| 編號 | 最小反例／變更 | 預期 |
| --- | --- | --- |
| I01 | 移除 identity_basis／改成 null／加未知欄位，各一次 | 分別拒絕，不從 consumer revision 補值 |
| I02 | commit 不存在／指 symlink 分片／registry canonical hash 改／非消費端祖先的同層分支 commit，各一次 | 讀取端歷史核對失敗，不能降成 stale |
| I03 | absent transition 的 null；有空 index 卻仍 null；非空 transition 卻略過重播 | 前者可驗；後兩者拒絕或明示必要重播未支援 |
| I04 | adoption 1、2 釘不同合法背景；只改其中一份舊來源或前件 hash | 各成員驗自己的背景；舊錯誤仍使建置失敗，不用末筆遮過 |
| I05 | 新增無關 registry 記錄／只改目前磁碟 parser 註解 | 指派仍合法；當次 owner 未變時仍適用 |
| I06 | 同 printing／face、名稱與 printed 狀態不變，只更新其他欄的 observation | 歷史按舊 pin 通過；依現在來源重驗後仍適用，不要求舊 observation 等於新 observation |
| I07 | printing 名稱換字；同名但面修訂 ID 更新，各一次 | 舊指派歷史可驗、當次新來源不套用；不搬到新 owner／字串 |
| I08 | 名稱證據是另一張卡／另一面／effect／錯 source_hash，各一次 | 歷史採納證據拒絕，不以目前同名放行 |
| I09 | split／reassign／退役使有效父卡或面不同；printing 名稱 unknown | 舊指派不轉移、不借 current；保留歷史並報當次不適用或缺來源 |
| I10 | 省一個歷史 EN 批次／省歷史 name_identity use／給決定掛別筆背景，各一次 | 完整來源或決定審計拒絕，不以 partial verify 宣稱成功 |
| I11 | 背景是 PR 消費端祖先，但不在明示 base main pin 的歷史 | 採納 PR CI／合併前檢查拒絕；不能借讀取端通過代替採納檢查 |

### 6.2 穩定 ID

以下 H 為 build-db §14 canonical-json-v1 的完整 SHA-256（64 小寫 hex）；kind/recipe 字串參與 hash，命中仍比完整輸入。除模板的既定加長政策外，這些完整 hash 碰撞一律停止，不換舊鍵。所有輸入均排除時間、建置次數、路徑和無關卡；集合排序，順序有意義者保留。

| 物件 | ID 與 hash 輸入 |
| --- | --- |
| context | `ctx:`＋H(`{recipe:context-v1,source_unit_id,semantic_variant}`) |
| use | `use:`＋H(`{recipe:use-v1,owner,field,ordinal,context_id}`) |
| binding | `bind:`＋H(`{recipe:binding-v1,context_id,ordinal,template_id,params,source_span}`) |
| translation | `tr:`＋H(`{recipe:render-v1,context_id,target_lang,dependency_key,text,origin,authority}`)；dependency_key 釘排序的 binding、精確模板譯本／術語 choice record_hash；counterpart 另釘兩端 owner/ref 與核對決定 |
| glossary_term | 人工永久 `term:<concept_key>`，見 §5 |
| face_semantics | `sem:`＋H(`{recipe:semantics-v1,face_id,region,rule_text,rule_sections,normalizer_version}`)；exact 內容與有序 sections，語義引用閉包另進 rule bundle |

推導 translation.revision 為同一完整 translation hash 的前 13 hex 轉非負整數（52 bit，符合 UInt）；它是穩定內容版本鍵，不表示時間順序。同 `(context_id,target_lang,revision)` 撞不同完整 translation.id 即停止；人工 template_translation.revision 仍是只增修訂序號。translated_by 記釘住的 renderer／人工譯本作者追溯，translated_at 取依賴已採納譯本／核對的最晚時間，不用執行當下時間。生成效果譯文若任一語義模板／選詞來自 machine，origin=machine，否則為 project，authority=unofficial；單一已採納名稱／label 原樣取詞時保留該詞 origin，digital_official 對有效獨立名字政策或真人同概念證據取得的數位官方名稱；標籤仍驗原同概念證據。名字政策與 same_name 不授予效果或 glossary 官方概念權威。counterpart 固定 official_sve/sve_official。tokens 首版一律 null：目前沒有獨立公開翻譯 token 契約，不新增假引用。

公開 translation.id 目前沿上述完整 hash 配方；#53 容量驗收時評估縮短，本契約不先改 ID 配方。

### 6.3 來源 owner 與例子

use 的 owner 是原文引用者，恰一組；context.source_unit 必須等於該 owner 的原文。ordinal 只在 section/action_label 非 null。field 的正式 mapping 如下；商品使用 label，與 snapshot-format 一致。

| owner.kind／鍵 | 合法 field |
| --- | --- |
| face_revision／revision_id | name、effect、section |
| printing_face／printing_id,face_id | name、effect、flavor、section（只用該版已知 printed 字串） |
| qa_version／qa_version_id；cr_clause／cr_clause_id | 前者 question/answer；後者 effect（條文） |
| vocabulary／vocabulary_kind,vocabulary_code；product_family／product_family_id；product／product_id | label |
| keyword／keyword_id | label、effect（definition）、action_label |

例 A（一般 owner）：合成 JP revision RJ.effect=U0，U0.text=`N測試２`。context C0=(U0,default)，use=(RJ,effect,null,C0)，binding 套 §4 的 count=2；已採納譯本 `測試值 {{count}}，符號 N` 產 T0，selection(C0,zh-Hant)=T0，RJ 的 FieldTranslation={field:effect,ordinal:null,target_lang:zh-Hant,translation_id:T0,basis:own_source}。把 count 由 2 改 3 卻沿用 source_hash 或 T0，必失敗。

例 B（同字異義）：RA.name、RB.name 都指 U1=`星`，但人工分別採納角色概念與物件概念，variant=character/object。工具產 CA=(U1,character)、CB=(U1,object)，分別 use→RA/RB，採納選詞與名稱模板產 TA=`星角色`、TB=`星物件`；selection(CA,zh-Hant)=TA、selection(CB,zh-Hant)=TB，兩個 FieldTranslation 各為 own_source。補登 RB 的物件指派後直接重建其 use/binding/TB，無須修改舊 use 或為整卡補一份人工全文。

### 6.4 更新與失效

歷史清冊仍以自身凍結語義與 context 重算；歷史合法性與當次 owner／用途的適用性分開驗。
換 current observation 或增加無關身分不改寫歷史 basis，也不直接使其歷史採納失效；
當次用途變動仍依本節失效／重算。C+hash 不放寬使用新來源或錯 owner 的門檻。

模板譯本或術語 choice 更新後，反查精確依賴、重新渲染、機械驗證並產新 translation ID／selection；成功者自動選用，**不要求每張卡再簽一份 translation/selection**。原文或語義改變時舊產物不適用新 owner/context，仍從新來源重新匹配。機械失敗者列原因、該 context 回原文；缺任一句不能冒稱完整翻譯。模板來源／hash 損壞或引用閉包錯是建置錯誤，不吞成一般缺譯。

新渲染批次必報 generated_rows、changed_rows、failed_rows、sampled_rows 及精確輸入／輸出成員 hash。已有核可抽查政策時依其檢查；首輪照 §1 已核可的高頻模板／模型分歧抽查流程，記實際樣本及核對者，不預填已抽查。§2 長尾政策採納不另要求每批人工樣本，當批 sampled_rows 可為 0，另報 approved_policy_rows 與引用收據。抽查失敗隔離受影響批次並修人工來源／匹配規則，不原地修改生成文字。舊生成物可留舊快照／報告作比較，不回填成新來源的 fresh。

## 7. 跨區核對、counterpart 與語義組

region_text_review.data 恰為 `{card_id,region,jp_hash,region_hash,hash_recipe,faces,display_checks,state,checked_at,adoption_no,predecessor}`。region=en，hash_recipe=rule-bundle-v2；faces 每項 `{face_id,jp_revision_id,region_revision_id,jp_semantic_id,region_semantic_id}`，涵蓋全部面、按 face_id 排序。jp_hash/region_hash 投影 source_jp_hash/source_region_hash；跨語 hash 不要求相等，state=aligned/divergent（pending 留候選）。aligned 需 sampled/confirmed 語義對照決定，兩端身分仍另須 confirmed。

display_checks 每項 `{face_id,field,jp_ordinal,region_ordinal,jp_ref,region_ref,state,reason,counterpart}`，field=name/effect/section/flavor，ref 為 §2 source_ref。預設同面同欄對照由工具列出，採納以雙端精確 hash 的 batch 決定釘住；只有非同欄／跨 ordinal 等例外逐項人工指定。state=aligned/divergent，aligned 的 reason 可空，其他必填；counterpart 為布林，true 僅在 aligned 且該對照已採納時允許。這項決定是 counterpart 原文配對的人工採納，不是把每個推導選用寫進 git。

region_divergence.data 為 `{card_id,region,field_scope,reason,effect,override_dsl_id,resolved,jp_ref,region_ref,adoption_no,predecessor}`，沿 build-db 的 rules/name/all、manual/override_dsl enum；override_dsl 時 ID 必填且可驗。confirmed 決定釘兩端來源與差異，resolved=true 也須新 confirmed 決定與解除證據。投影當前 region_divergence，舊版本留封套。

英文獨有由既有 fresh confirmed_none 決定及其查核範圍／as_of 推導，依賴鍵與 F1 釘該決定，不另寫 en_only 的 source_exception 或要求第二次人工確認；查核失效或已有 JP 對應時重新判定，不沿用英文獨有資格。

只有來源例外另寫 source_exception.data，形狀為 `{card_id,region,scope,basis,decision_ref,adoption_no,predecessor}`，region=en、scope=rules/name/all；basis=divergence/default_jp。divergence 的 decision_ref={decision_id,record_key,record_hash} 指 fresh、未解的 confirmed divergence；default_jp 時 null，表示撤回先前例外並重新依預設來源規則推導，不強造缺少的 JP 來源。各 scope 與證據一致；這是採納英文來源的例外，不豁免文字與翻譯閉包。

### 7.1 counterpart 不占共用 selection

`translation_selection(context,lang)` 僅選本站共用的渲染譯文。官方 counterpart **逐 owner** 推導 translation，FieldTranslation 直接引用，**不寫入共用 selection**。該 translation.context_id 仍是顯示來源 owner 的 context，source_hash 驗該來源；text 取已核對 counterpart 原文，origin=official_sve、authority=sve_official，bindings/terms 空；依賴鍵包含精確兩端 owner/版本／核對決定，所以同 context 可有不同官方 translation ID。derived revision 的碰撞檢查仍照 §6.2。

例 C：K1、K2 的 JP 效果共用 UJ/context CJ，但 EN counterpart 分別為 UEA、UEB；K3 也共用 UJ 卻沒有已核對 EN。K1/K2 分別 own JP use→CJ，建置產 TOA/TOB 並在各自 FieldTranslation 設 official_counterpart；selection(CJ,en) 仍是有效的 project/machine 渲染譯文 TM，K3 用 own_source→TM，缺 TM 就回日文。不能讓 K2 用 TOA 或讓 K3 因同字 UJ 被視為已有官英。

反向 EN→JP 亦按 owner 的已採納 display_checks 直接選官方 JP。JP→繁中 TZ 是共用 selection(CJ,zh-Hant)，已核對 EN 可以 shared_jp→TZ；已確認身分與面對應、文字尚未核對且無已知相關 divergence 時，EN 以 shared_jp_unchecked→TZ 提前顯示並加標示；EN use 仍綁自身 UE/context CE，不能偽造 CE.source=UJ。EN 有相關 divergence 則停止該 scope 的 shared_jp/counterpart，從已採納英文來源例外生成 CE 的 own_source 繁中；name-only 差異不抹掉獨立有效的效果，但規則名稱依賴須重驗。

`shared_jp_unchecked` 僅限 target_lang=zh-Hant、EN 接收端及已確認同 card/face，仍須 JP 譯文 fresh、兩端來源可驗、無相關已知 divergence。它不建立 aligned review，也不影響 DSL/機制的區域阻擋。檢查完成且顯示欄位適用後改為 shared_jp；若核出差異，移除共用、依英文來源例外重算或回 EN 原文。缺來源、身分未確認、表記未定且無可用 current 時不以此例外猜配來源。basis 改變不修改共用 TZ 的內容或 origin。

counterpart 新版到齊時，僅在核對與 freshness 通過後優先於本站選用；失效時回有效本站 selection 或原文，不刪舊機翻歷史，也不令其他 owner 無條件失效。display_checks 驗 exact 字句／版本，rule bundle 則管規則等義，不能互相代替。

日英對應面的段落數不同時，受影響的 section 欄（全部 ordinal）及包含這些段落的 effect 全文均不得 shared_jp_unchecked；回 EN 原文或已採納的 own_source，不按 ordinal 猜配、截短或拼接。此限制不阻擋獨立且符合條件的 name 等欄位。例 D（合成）：JP 有 2 段、EN 有 3 段，即使同卡／面已確認且無已知 divergence，也不能把 JP 第 2 段的繁中放到 EN 第 2 段，或用 JP 效果全文代替 EN 全文；缺譯時各自回 EN 原文。完成 display_checks 採納且適用後，才依精確對照使用 shared_jp。

### 7.2 semantics 的啟用

語義能力與 [ADR-0013](../adr/0013-rule-bundle-migration.md) 一致：需要 v2 共用時才建立完整 face_semantics/revision_semantics/semantic_reference。初始 exact 原文的機械映射可以推導；跨表記等義／提示或重複 token 移除須釘既有 wording 採納政策／confirmed 決定，不把 identity confirmed 當等義證明。三表從此精確輸入與有效語義決定推導，不新增每卡一份翻譯封套。

rule_hash、references 與 rule-bundle-v2 recipe 沿 build-db §14；unknown 段落、缺同區 token target、漏背面或規則矛盾均不能產可驗 bundle。規則本文、數值、特性、rules_names、normalizer 或 token 規則閉包改就重驗；純已採納表記可共用 semantics，但新 exact 字句的翻譯仍重算。不回寫首發已有等義證明。

## 8. 顯示與發布

沿 [snapshot-format §5](snapshot-format.md#5-語言矩陣與取用)，版次決定原文／卡圖，UI 語言決定翻譯列；日／英 UI 不回退繁中，printed 不冒用 current 譯文。缺繁中明示回原文，機器與非官方來源必有標示，數位官方卡名不把整段繁中效果變官方。標籤文案是呈現方式，不擴張 2026-10-01 來源規則的核可範圍；未核對 EN 提前顯示依 §1 已核可政策；核對完成且適用時改 shared_jp，發現差異則撤下共用並走來源例外。

現有公開格式只出選定 translation/text 與 FieldTranslation，不出 context、模板、清冊或稽核 hash。glossary 的有效加粗由獨立採納鏈／型別推導；原文與譯文位置的公開承載須另審格式，[術語採納擴充 §6](glossary-adoption.md#6-公開快照影響與最小擴充提案) 列出最小提案及影響，不擅改現有白名單或啟用 translation.tokens。名稱／label 翻譯納入 bootstrap 容量驗收。無翻譯不阻止查卡／手動；本契約不宣稱雙區三語容量或匯入器已驗收。

## 9. 獨立反例與定向突變驗收

清冊 v2 另須完整通過[歷史清冊 H01–H28](template-source-replay.md#6-最小獨立驗收)，
包括混合 producer、環境差異兩例、六流漂移、決定性、全歷史、F1 與性能；以下逐筆反例仍保留。

每列以最小合成成功基例只改指定條件，檢查獨立預期；同列多個條件須各跑一例。這是實作驗收規格，不是已實跑 mutant 的宣稱。真實遷入量、合成案例與實跑數分開。

| 編號 | 反例／突變 | 預期 |
| --- | --- | --- |
| V01 | 缺分片、未索引、../、symlink 各一次 | 各自拒絕 |
| V02 | 改 record 留舊 record_hash／新增 batch 成員 | 拒絕沿用決定 |
| V03 | sampled 缺人／時間／樣本；confirmed 漏 checked | 各自拒絕 |
| V04 | raw 缺 bytes、parser hash 改、定位錯字串 | 各自失敗，不回讀草稿／latest |
| V05 | 舊 T+10hex 同鍵異 normalized／payload | 拒絕，不重配舊 ID |
| V06 | 原 ID 改 normalizer／schema／variant | 拒絕；新 ID 可採納 |
| V07 | 新短 ID 碰撞／完整 hash 撞異 bytes | 只加長新鍵／停止 |
| V08 | supersedes 成環；component 子譯本變 | 前者拒絕，首版父句渲染不受後者影響 |
| V09 | 少換行／空行、句中提示重疊、UTF-16 offset | 各自覆蓋驗證失敗 |
| V10 | 缺／多參數、字面 N 當 slot、引用懸空 | 各自拒絕 |
| V11 | use 雙 owner／錯 source／缺 section ordinal | 各自拒絕 |
| V12 | 同字異義合併，或無理由新增 variant | 檢出錯譯／拒絕例外 |
| V13 | binding.context/template/lang 不一致 | 各自拒絕 |
| V14 | selection 指錯 context/lang 或未採納譯本 | 各自拒絕 |
| V15 | 決定續版分叉／錯前件；proposed 入正式分片 | 各自拒絕，候選不占正式鍵 |
| V16 | 原文改、rule_hash 不變 | 新來源重新匹配，不能沿用舊生成譯文 |
| V17 | 模板譯字或術語 choice 更新 | 依賴者自動重算選用，不逐卡補簽；失敗列報告 |
| V18 | 只有 identity confirmed 就 shared_jp | 拒絕冒稱已核對；通過 §7.1 來源與段落閘門且無 divergence 可 shared_jp_unchecked 並強制標示 |
| V19 | 只有 aligned 就合併 card/face | 拒絕 |
| V20 | 缺 JP 或 unmapped 當英文獨有 | 拒絕假例外 |
| V21 | 日英同卡 aligned 卻另翻 EN 繁中 | 檢出違反 JP 來源，應共用 TZ |
| V22 | name-only 差異漏擋名稱／誤擋獨立效果 | 各自檢出 |
| V23 | 一端文字／身分／規則改、counterpart 不變 | 移除受影響選用，其他 owner 不受牽連 |
| V24 | same_character 代替同概念；machine 改成 project | 各自拒絕 |
| V25 | 官方卡名使整段繁中效果變官方 | 拒絕 |
| V26 | v1 證據只改 recipe 名變 verified | 拒絕；沒有實例不造遷移收據 |
| V27 | 漏 token 標頭、缺 target、漏背面、unknown 當提醒 | 各自失敗 |
| V28 | token 規則／數值／規則名稱變；純表記變 | 前者 bundle 變重驗，後者規則可不變但翻譯重算 |
| V29 | 語言切換換 printing、printed 用 current、缺譯空白 | 各自檢出 |
| V30 | 只改無關卡、執行時間、YAML 排版 | 同輸入 translation ID 不變 |
| V31 | 拆模板／修 normalizer／補 variant／改商品名 | 新建置合法重建 binding/use，無舊組 FK，首版不阻擋 |
| V32 | 兩卡同 UJ、不同 EN；第三卡沒 EN | TOA/TOB 各逐 owner，第三卡只用 TM／原文 |
| V33 | NFKC 前後字數不同仍用 normalized offset | raw roundtrip 失敗；多對多 trace 基例通過 |
| V34 | 摘錄不是整欄、字典無 digital_face；改一個 span/key | 前者可採納，錯 span/key 拒絕 |
| V35 | 改建置時間或重排輸入產新 tr ID／revision | 檢出不穩定；全 hash 或 52 bit 衝突須停止 |
| V36 | 參數缺譯卻拼原文名、tokens 填未定義結構 | 不產完整譯文／tokens 首版須 null |
| V37 | 只有提示行沒有 body、模板成員有／無提示 | 均完整覆蓋，新 reminder 譯本獨立，body payload 不改 |
| V38 | 未核對 EN 隱去標示／解開 DSL，或有 divergence 仍提前共用 | 各自拒絕；完成核對且適用才移除標示 |
| V39 | 未做人工樣本就標 sampled；首輪收據缺失／政策 hash 錯／有分歧仍走 approved_policy；審過 machine 改 project | 各自拒絕；長尾無分歧且收據完整可 confirmed，當批真人樣本為 0 仍可採納 |
| V40 | JP 2 段、EN 3 段，按相同 ordinal 提前共用或覆蓋 effect 全文 | 受影響欄不得 shared_jp_unchecked，缺譯回 EN 原文；獨立 name 不受阻 |
| V41 | fresh confirmed_none 無額外 source_exception；改成 stale／已有 JP 對應卻沿用 EN-only | 前者可推導英文獨有來源，後兩者重新判定，不重複要求人工採納 |

### 9.1 獨立名字與同名連結政策的接線

新政策入口、核可文件／操作政策兩種 hash、真人與規則界線依 [數位名字政策](digital-name-policy.md)。
不混入既有 glossary／模板封套或39分片，不借模板首輪樣本、wording核可或委託事件。
直接政策名字的 render-v1 詞彙依賴不含後補概念、owner、政策／清單版本或整批目錄；完整來源與採納留F1。
真正效果引用依賴自己的概念／choice，名字先可用不表示整段引用已完整；#53接同一resolver，#196仍守原委託。
政策核可與樣本數分開報，coverage不採納，未知保持未知。
