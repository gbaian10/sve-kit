# 翻譯、模板與當前資料契約

本契約保留四層來源 binding／未匹配清單、句型比對、固定字與參數定義、譯文、術語、卡名、風味、加粗、新卡自動套用及未匹配清單。
2026-10-04 起，翻譯資料採可直接修訂的當前值；Git 保存修改歷史，退回使用 git revert。
不再用決定封套、收據、成員雜湊、只增不改、歷史重播、首輪抽查、雙模型或手動合併前檢查作載入門檻。
本文件的四層契約依 [ADR-0016](../../adr/0016-four-layer-translation.md)，N0 已實作；
N1 由 #380／#499 接續。逐欄型別、驗證責任與固定案例以
[四層共用契約](four-layer-translation.md)為準；建置與公開 reader 依 §8 同步切換。

## 1. 來源與顯示原則

繁中以日文為唯一一般翻譯來源。有有效且已確認同卡同面的 JP 來源，一律以 JP 翻譯；
沒有 aligned 核對或已知日英效果不同，均不禁止 JP 繁中，也不切換成 EN 翻譯。
跨區卡片 ID 對應仍須人工確認，不能靠同名或移除卡號後綴推算；錯 owner／face、過期或缺來源仍拒絕。
純英文卡須確認日版沒有才個別處理（[#500](https://github.com/gbaian10/sve-kit/issues/500)），unmapped 不等於 confirmed_none。
JP 卡缺少某個 EN 段落不構成純英文卡；段落不同時顯示完整 JP 效果，不依 EN ordinal 拼接。
卡名來源順序及資格見[數位名字規則](digital-name-policy.md)，風味見[風味直接對照表](flavor-translation.md)。
JP 依據的顯示不代表規則等義；區域差異、DSL／機制及官方 counterpart 資格仍依 §7。

每筆資料只用 `origin: official|project|machine` 與 `low_confidence: Bool` 描述來源類別及信心，
不保存誰點哪個按鈕、模型互審結果或私人證據 hash。machine 經審閱仍是 machine。
結構及適用性通過的低信心譯文直接顯示「待校對」，可切回原文；旗標不是禁止入庫或發布的人工關卡。
缺譯或來源尚無可匹配的完整框架者回原文並列清單；已宣告可用卻缺必要葉引用則拒絕，不把半段原文混入完整繁中譯文。
來源損壞、錯 owner、錯型別及衝突引用屬錯誤，不能用低信心旗標掩蓋。

## 2. 當前資料入口

沿用 `authored/translations/`，分片按卡包或共用類別歸檔，單檔小於 1 MiB；序號只是檔名，不是修訂鏈。
沿 authored-layout §1 的嚴格 YAML、未知欄位拒絕、禁止 symlink／跳脫路徑。
filing_key 為 `[A-Za-z0-9_-]+`，sequence 為三位以上十進位字串；不要求連號或預先排序。
讀取當前工作樹的固定 glossary／templates／forms／overrides 子目錄一次，載入後依 record_key 排序；重複鍵拒絕。
沒有內部 checksum index；其他目錄的私人草稿不納入輸入。空目錄表示空集合。

| 檔案 | 完整頂層欄位 |
| --- | --- |
| `translations/{glossary,overrides}/<filing_key>/<sequence>.yaml` | `format: 3, kind: translation_shard, records` |
| `translations/templates/{definitions,values,candidates}/<sequence>.yaml` | `format: 3, kind: translation_shard, records` |
| `translations/forms/<sequence>.yaml` | `format: 3, kind: translation_shard, records` |

四層入口只接受 format 3 分片；glossary 與模板 reader 共用已載入的資料。
切換時已清查並轉換 format 2，正式 reader 不保留雙軌載入。
建置時由四層來源核心產生 binding／未匹配清單，不進 Git，見[來源位置契約](four-layer-source-positions.md)。
舊格式留在 Git 歷史，不作現行載入分支。

record 必填欄位為 `{kind,data}`，另可選填 origin、low_confidence、note。
省略 origin 時為 project，省略 low_confidence 時為 false；顯式填預設值可讀，寫出端省略。
真實譯文的 official／machine 與 true 仍照常讀出、驗證及投影，不從譯文內容猜品質。
record_key 由 kind 與 data 載入時計算，不接受存檔欄位；重複選擇鍵仍拒絕。
`missing_source_reason` 缺欄位時為 null，寫出省略 null；`value: null` 的缺譯／撤回語意仍須保留。
record_key 是下表選擇鍵前加 kind 的 canonical JSON 陣列字串；全入口唯一，不再包含 adoption_no 或 revision。
note 省略時為空字串，僅寫簡短資料理由，不參與 ID 或決定狀態。來源、owner、參數等結構欄位不是審查欄位。
concept_evidence 的 concept_note 可省略，只保留有實際內容的說明；精確來源核對不依賴套話。
沒有 decisions、default_decision_id、members、sample_ids、delegation、adoption_review 或 predecessor。
同一選擇鍵直接修改當前資料；null 的含義按各 kind 明定，不能以最後讀入的重複鍵覆蓋。

| kind | 選擇鍵 | data 完整欄位 |
| --- | --- | --- |
| glossary_term | id | `id,category,concept_key,source_ref,source_span,authored_source_ja,missing_source_reason` |
| glossary_choice | term_id,lang | `term_id,lang,value,concept_evidence`（選填 `source_claim`） |
| glossary_emphasis_choice | term_id | `term_id,value` |
| symbol_localization_choice | symbol_id,lang | `symbol_id,lang,symbol_basis,value,concept_evidence`，完整值依[記號文案契約](catalog-route-adoption.md#6-卡文記號與三語文案) |
| sentence_template | id | 四層 Frame：`id,source,role,semantic_variant,leaf_schema,projection,content_hash`，見[共用契約 §3](four-layer-translation.md#3-frame-與語義身分) |
| template_translation | template_id,lang | `template_id,lang,target`，target 為 Literal／LeafRef／Form／NP 節點 |
| template_translation_candidate | source_kind,candidate_id,lang | `source_kind,candidate_id,lang,text,normalized_hash,role,reasons`，未啟用原稿依 §2.1 |
| template_translation_variant | template_id,lang,variant_key | `template_id,lang,variant_key,target` |
| translation_form | id,lang | `id,lang,signature,rule,cases`，見[共用契約 §5](four-layer-translation.md#5-targetform-與部分-np) |
| glossary_choice_variant | term_id,lang,variant_key | `term_id,lang,variant_key,value,concept_evidence`（選填 `source_claim`） |
| context_assignment | owner,field,ordinal | `owner,field,ordinal,source_hash,variant,concept_key,reason` |
| card_name_concept | subject | `subject,term_id,source_ref,reason`；subject 恰為 `{card_id,face_id,source_lang,source_hash}`，不帶歷史 identity_basis |
| template_match | context_key | `context_key,source_hash,matches` |
| translation_override | context_key,lang | `context_key,lang,action,templates,terms,reason`，action=suppress/pin/default |

glossary 欄位依[術語契約](glossary-adoption.md)。context_key 恰為 `{source_unit_id,variant}`；
matches 為 null（回自動比對）或 `{frame_id,source_span,values}` 陣列，須完整且無歧義。
context_assignment 的 concept_key 可 null；非 default variant 須非空理由。card_name_concept 的 term_id 可 null，表示撤回指派。
owner／field／ordinal 的組合及來源雜湊由本次有效身分與原文自動核對，不再要求歷史背景在 base main 的祖先。
同名不代表同概念。region-reviews 中的 region_text_review／region_divergence 同時控制 DSL／機制，
封套尚未落地，見 §7.1；保留 build-db §5 的決定及 freshness 檢查，不轉成本節 record；區域核對的獨立讀取不得忽略它們。
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

`source_exception` 不再是合法 kind；原 divergence／default_jp 分支均移除。
新 reader 遇此 kind 必須拒絕，切換前列清查與處置結果，不能無聲忽略。
`region_divergence` 是 §7.1 的非翻譯資料，仍保留；不藉翻譯欄位授予或解除跨區規則資格。

source_ref 保留既有 `{batch_id,source_version_id,parser,locator,text_hash}`，只定位來源資料；
store 名稱與實體路徑由執行端設定，authored 不保存 store_id。parser 是本次支援的解析器，locator 為 JSON Pointer，
text_hash 對定位到的完整 UTF-8 字串計算；span 使用 Unicode code point 半開區間。
不把本機檔名、對話紀錄、點擊時間或私人網頁 hash 塞進來源欄位。

### 2.1 無法綁定 slot 的候選原稿

`template_translation_candidate` 保存尚無完整參數定義或無法無損綁定的專案譯文草稿。
沿用 format 3 的 kind/data record 與可省略品質欄位，origin 限 project/machine，low_confidence 保留原值；
它始終未啟用，不因低信心旗標為 false 而成為可渲染譯文，也不能用 template_translation_variant 代替。

路徑固定為 `authored/translations/templates/candidates/<sequence>.yaml`，
沿固定子目錄讀取、可修改分片及單檔小於 1 MiB 的限制；不新增 includes 或 checksum index。
record_key 為 `["template_translation_candidate",source_kind,candidate_id,lang]` 的 canonical JSON 字串。
text、reasons、note 不參與身分；同鍵原稿有不同文字時拒絕，不按檔序選取。

| data 欄位 | 型別與含義 |
| --- | --- |
| source_kind | effect |
| candidate_id | 非空的原稿穩定識別，例如效果舊 T ID；不是已驗證的模板 ID，不要求存在正式 definition |
| lang | 沿既有 Lang 型別的目標語言 |
| text | 非空 UTF-8 譯文草稿字串，保留最終原稿字元；不解析匿名 N/X 或套用可渲染譯文的 slot 語法 |
| normalized_hash | 舊 normalizer 的歷史 pattern hash；只供人工與 #499 對照，不在建置中驗證 |
| role | body／reminder／token_header／layout |
| reasons | 排序、唯一且非空的原因代號陣列，每項符合 ASCII `[a-z][a-z0-9_]*` |

data 恰含上表七欄；record 及分片沿既有封閉結構、鍵唯一、安全路徑與引用檢查。
normalized_hash 是舊 normalizer 的歷史 pattern hash，只供人工與 #499 對照，不在建置中驗證。
原稿只存我們撰寫／生成的譯文，不保存官方原文、JP normalized、完整來源欄位、私人路徑、核可 hash 或事件。
來源文字依固定來源的 source_ref／locator 取得，不另抄進 text 或 note；候選的歷史 hash 不構成當前來源 binding。

候選不加入 active targets、bindings、renderer、pin 選用或公開翻譯投影，也不計為機械有效／已翻譯覆蓋。
統一處理清單保留 candidate ID、各原因及受影響卡片數；來源未知時影響數記未知，不當成零。
日後能綁定時，以普通資料 PR 改為有效定義與 template_translation，通過原有參數／來源驗證後才可渲染。
不新增核可、收據、修訂鏈或不可變機制。

## 3. 四層來源核心與定義

四層來源核心定位來源欄位及句型位置，保留 body、reminder、token_header、layout、name 與 label 角色；風味不在其中。
每次建置從封存來源產生 binding／未匹配清單，不存 Git，見[來源位置契約](four-layer-source-positions.md)。
缺來源、unknown、空字串與空白不能混為不存在；每個來源片段都要有去向。

四層 Frame 的 source descriptor、canonical source、語義 payload 與完整 hash ID 依
[共用契約 §3](four-layer-translation.md#3-frame-與語義身分)。
舊 T/C 六欄 payload 只用於遷移映射，不再作新 frame 的定義或匹配條件。
同一來源位置至多匹配一個有效 frame；多個候選列歧義，不依檔名、頻次或 ID 任選。
相同固定字仍須驗葉槽角色、合法域及 semantic_variant；未定變體限定精確來源，不跨用途合併。
所有頻次都保留整行框架，NP/form 覆蓋變動不重配 frame；不造逐卡自由翻譯覆寫。

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
| NFKC 改變字元 | trace 保存 raw／canonical 區間，以來源與 canonical 字串取回前後 bytes；normalizer 僅做明定轉換，不做逐字可逆的假設 | 把全形標點正規化後位置當原文 offset 失敗 |
| 裸 N／『X』與原文字面字母 | 從 exact 來源與具名規則重建 canonical；葉槽明列位置，字面 N/X 保留 literal，舊 normalized 只供重鍵盤點 | 全域 replace N、把字面 N 當數字 slot 失敗 |

layout 不含待翻語義，可機械生成固定模板；reminder/token_header 是獨立句型，須像 body 一樣提供其譯本。版次欄位與 section 的完整覆蓋各自核對，不跨欄偷接；提示分類有疑義則保留原文／失敗清單，不擅自取語義等義。

風味文字不經此分段，也不進效果來源核心，直接以原文 hash 對照，見[風味文字契約](flavor-translation.md)。

### 4.2 葉 schema 與驗回來源

葉的型別、角色、合法域與 SourceBinding 依[共用契約 §4](four-layer-translation.md#4-有型別葉槽與值)及
[§6](four-layer-translation.md#6-sourcebindingtrace-與逐-occurrence-位置)。
不再將所有數字視為 N、所有概念視為任意 term；數量、次數、傷害、序數及門檻分角色。
必要引用缺失、未知概念、錯型別或已知錯來源拒絕；尚未辨識的來源留 pending，不用自由文字參數假裝解析成功。
卡名缺唯一概念時仍可保留未啟用候選／整欄原文，但不能造永久概念或可執行 token 身分。

來源重建有兩個獨立檢查：先按原文 codepoint 位置從凍結來源重組 exact UTF-8，
再以具名分類器／normalizer 驗固定字、葉值、角色、單位與 canonical source。
依計數物、所在區域集合與數量角色查表並驗回原文單位；NFKC、別名及省略均留 trace。
renderer 與 DSL 使用同一份通過檢查的值，NP 只組合引用，不建立重疊的第二份來源槽。

### 4.3 target 與自動套用

template_translation.data 恰為 `{template_id,lang,target}`，品質欄在 record 外層。
target 由 Literal／LeafRef／Form／有限 NP 節點構成，完整簽章依[共用契約 §5](four-layer-translation.md#5-targetform-與部分-np)。
未知節點、壞引用或未使用必要葉是結構錯誤，拒絕載入／建置；缺必要目標語選詞則整欄回原文列 missing_term_translation。
Literal 中不解析舊 `{{slot}}` 語法，不允許把必要引用改寫成同字 literal 躲過檢查。

句框架維持整行，子句切分可作盤點，但不啟用任意句子拼接。
未覆蓋的 NP 修飾可用固定譯句與已驗葉引用，無須等待 NP 全覆蓋；未解語義不能投成 DSL no-op。
新增卡包由四層來源核心產生 binding／未匹配清單、套用 frame／形式／詞庫並渲染，合併列出低信心、新句型、歧義、未匹配及缺譯清單。
低信心沿實際依賴 OR 傳播，machine 不自動等於低信心；不新增固定人工樣本門檻。
加粗由逐 occurrence 的來源與渲染節點產生，不能事後搜尋中文；annotation set 契約見
[共用契約 §8](four-layer-translation.md#8-建置-dbrender-projection-與依賴)。

## 5. 概念選詞與數位證據

glossary 保留 `term:<concept_key>` 永久概念，category=keyword/ability/trait/rule_term/card_name。
選詞以 `(term_id,lang)` 為唯一當前值；vocabulary 以 `(kind,code,lang)` 為唯一值。
同名、同譯不合併概念；職業／卡種代碼與泛稱、資源名依[術語契約](glossary-adoption.md)分開。
加粗可直接改值，讓所有依賴者重新產生；不需改核可 hash。

官方來源可使用 source_ref 重建文字，專案／機器詞用自寫 text；official 不能單憑相似字串判定。
來源類別 official 與公開 authority 分開：數位官方名稱仍是 digital_official，整段效果機器翻譯仍 unofficial。
source_claim 只保留可公開的作品／URL／出處主張，不記本機證據；來源不明不得硬升 official。
claimed_source 是選填的出處主張；沒有具體主張就省略，不用本機草稿套話代替。

## 6. 建置、穩定 ID 與失效

### 6.1 當前資料的建置

一般 reader 依[共用契約 §1](four-layer-translation.md#1-型別驗證責任與失敗語義)驗結構與輸入引用，
建置 validator 以本次來源驗 owner、全欄覆蓋、frame、葉值與實際依賴閉包。
四層來源核心在本次建置定位及驗證來源一次，不執行歷史 producer，也不保存新的核可證明。
缺資料不得借最新官網或個人檔補洞；來源歸檔完整性仍依[來源歸檔](../ingest/source-archive.md)。

現有 export-offline 對 face_revision 與 printing_face 的 JP 主文／section 建置四層翻譯，
並產生 3.0.0 公開 annotation；#498 的 N0 已完成 producer／reader／Web 接線。
每個來源 use 分別建 binding，source context 可共用，但 owner、field、面及所用原文均逐一驗證。
效果／section 只要自己的原文有效即可套用，不等 printed_text_state；名稱仍只用該版已知 printed 字串。
建置報告只列 ID、原因及欄位／退回計數，不輸出官方卡文。

### 6.2 穩定 ID

H 為 build-db §14 canonical-json-v1 的完整 SHA-256；相同 hash 仍比完整內容，碰撞即失敗。
Frame、binding-v2、context 聚合語義鍵、render-v3 與 annotation-v1 的精確輸入依
[四層共用契約](four-layer-translation.md#8-建置-dbrender-projection-與依賴)。
context ID 仍為 `ctx:`＋H(`{recipe:"context-v1",source_unit_id,semantic_variant}`)，
use 仍為 `use:`＋H(`{recipe:"use-v1",owner,field,ordinal,context_id}`)。
四層的 context.semantic_variant 是由來源順序的 frame／values／span 算出的聚合鍵，不能拿單一 frame 的變體物件冒充。

人工譯文及選詞可直接改值；ID／依賴 hash 用於重建和去重，不是不可變採納序號。
變更純文字／form／NP／加粗只重建 render；來源語義、角色、域或分類改變則重鍵並重驗相依 DSL／裁定。
永久卡片 ID 不受模板重鍵影響；舊引用依[多對多映射](four-layer-translation.md#9-舊模板重鍵與裁定引用)逐用途處理。

### 6.3 來源 owner

use 恰有一個有效 owner，context 原文等於該 owner 欄位；section/action_label 才有非 null ordinal。
face_revision 用 name/effect/section；printing_face 另可用 flavor。printing_face 的 name 只用自己已知的 printed 文字；
effect、section 與 flavor 只要自己欄位有原文單元，依原文 hash 套用，不等 printed_text_state 確認。
qa_version 用 question/answer，cr_clause 用 effect，vocabulary／商品用 label；keyword 可用 label/effect/action_label。

| owner.kind／鍵 | 合法 field |
| --- | --- |
| face_revision／revision_id | name、effect、section |
| printing_face／printing_id,face_id | name（只用該版已知 printed 字串）、effect、flavor、section（該版自己的原文單元） |
| qa_version／qa_version_id；cr_clause／cr_clause_id | 前者 question/answer；後者 effect（條文） |
| vocabulary／vocabulary_kind,vocabulary_code；product_family／product_family_id；product／product_id | label |
| keyword／keyword_id | label、effect（definition）、action_label |

未實作的 owner 不因有 FK 就宣稱支援。相同模板與參數預設同翻法；§2 的 pin 明示選用可重用替代譯詞，真正歧義才另用語義分支。

### 6.4 更新與失效

本次來源 binding、來源原文、參數及引用檢查通過才可輸出；壞結構／引用使建置失敗，未匹配／缺譯則回原文列清單。
舊的輸出不能冒充新來源翻譯。每次建立新 DB，不原地修改已輸出快照；公開保留窗口仍為 latest＋previous。
舊資料及舊程式留 Git 歷史，不承諾新 reader 能重播所有舊環境。

## 7. 跨區與官方對照

### 7.1 非翻譯區域核對保留原入口

region_text_review／region_divergence 會控制 DSL／機制；region-reviews 尚未落地，目前沒有 reader 或資料。
落地時再定分片封套、format、kind 與索引，不使用 §2 的翻譯 record，也不預定為 translation_shard。
以下保留採納門檻與資料語義，不宣稱已有可讀取的封套。

evidence 為排序唯一 `{source_ref,role}`；decision 沿 authored-layout §2 的完整封套與精確成員 hash。
record_key 是 `[kind,...選擇鍵,adoption_no]` 的 canonical JSON 字串，review 選擇鍵為
`card_id,region,jp_hash,region_hash`，divergence 為 `card_id,region,field_scope`。
adoption_no 從 1 連續只增，predecessor 首筆 null，其後為上一筆 `{record_key,record_hash,decision_id}`；
拒絕分叉、缺號、錯前件，舊分片及索引 hash 不改。這些門檻不擴及四層翻譯 format 3。

region_text_review 的語義對照來源 hash 與逐面對應格式須在正式規則驗證實作時另定；目前沒有規則等義 bundle 產出端，不提供此採納能力。跨語 hash 不要求相等；aligned 仍需 sampled/confirmed 語義對照決定，兩端身分另須 confirmed，不能把文字配對當作規則等義證明。

display_checks 每項 `{face_id,field,jp_ordinal,region_ordinal,jp_ref,region_ref,state,reason,counterpart}`，field=name/effect/section/flavor，ref 為 §2 source_ref。預設同面同欄對照由工具列出，採納以雙端精確 hash 的 batch 決定釘住；只有非同欄／跨 ordinal 等例外逐項人工指定。state=aligned/divergent，aligned 的 reason 可空，其他必填；counterpart 為布林，true 僅在 aligned 且該對照已採納時允許。這項決定是 counterpart 原文配對的人工採納，不是把每個推導選用寫進 git。

region_divergence.data 為 `{card_id,region,field_scope,reason,effect,override_dsl_id,resolved,jp_ref,region_ref,adoption_no,predecessor}`，沿 build-db 的 rules/name/all、manual/override_dsl enum；override_dsl 時 ID 必填且可驗。confirmed 決定釘兩端來源與差異，resolved=true 也須新 confirmed 決定與解除證據。投影當前 region_divergence，舊版本留封套。

### 7.2 逐 owner 的顯示選用

JP 繁中依 §1 選用；EN 接收端只需有效且已確認同卡同面的 JP 來源及 fresh 譯文，不要求 aligned，也不排除已知 divergence。
原文 owner 與顯示接收端分開：JP use 綁自己的 source_unit，EN 原文仍綁 EN owner，不能將 EN 的原文欄改成 JP。
日英段落不同時取完整 JP effect 及其來源對照，不按 EN ordinal 貼 JP section，也不另翻 JP 沒有的 EN 段落。
錯 card／face、錯欄位、缺來源、來源過期或仍 unmapped 時拒絕此選用；不能以同字串或低信心放行。

公開的新 JP 依據 basis（jp_source）及停用 shared_jp／shared_jp_unchecked 的 wire 變更由 #496 定義，#498 同步 producer／reader。
它只說明繁中依據，不能產生 aligned review、機制或 DSL 資格。JP 來源註記只可套到 JP 原文，不能套 EN offsets。

官方 counterpart 仍依有效 display_checks 逐 owner 產生 translation，直接供 FieldTranslation 引用，不占共用 selection；
origin=official、authority=sve_official、bindings/terms 空。其 context/source_hash 是顯示來源 owner 原文，text 是已核對的 counterpart；
依賴包含雙端 owner/ref 與 fresh 核對，不能讓共用 context 的第三張卡借到官文資格。
相關 divergence 仍阻止不適用的官方 counterpart 與跨區 DSL／機制；JP 繁中顯示不解除此限制。
來源／身分變動重驗兩端及用途，失效 counterpart 回有效本站詞或原文。來源更正另依 build-db 的獨立入口。

## 8. 公開投影與自動檢查

公開仍只輸出選中的文字、來源類別／authority、適用基礎與必要呈現資料，不輸出內部清冊或核可紀錄。
新格式的 low_confidence 必須從 producer 傳到 reader，UI 顯示「待校對」且提供原文；
啟用時同步 snapshot-format、機器 Schema、tuple、Python／TS reader 及能力版本，不能只改資料版號或冒用 reviewed 表示人工看過。
內部 annotation set／逐 occurrence 依[共用契約 §8](four-layer-translation.md#8-建置-dbrender-projection-與依賴)；
公開承載、無譯文的原文註記、basis 與 Unicode reader 由 #496 定義。內部支援不代表舊 tuple 已有這些欄。

四層實作的 CI 須驗格式、key／ID 唯一、葉角色／域、引用與 owner、來源覆蓋及 render 位置，
按[固定案例](four-layer-cases.md)逐項驗預期結果；這些案例目前是規格，不是程式通過紀錄。
官方輸入存既有永久私有 testdata repo，公開 repo 只留 commit 與檔案 hash，依既有 deploy-key 流程取得；
可信任 CI 缺資料失敗，fork 明示只跑合成檢查，不能宣稱完整資料驗收。
CI 不讀個人檔案、不即時爬站；報告列 ID／原因，不在 log、cache 或 artifact 放官方全文。
保留能抓錯的行為反例；資料 PR 不要求每個 guard 都做一次定向突變，不另加人工合併前命令。

## 9. 舊格式的保存

模板重鍵及每個裁定引用的 resolved／pending 依[共用契約 §9](four-layer-translation.md#9-舊模板重鍵與裁定引用)。
無法唯一且不擴域的引用保留原因與候選，不把 pending 掛成 active，也不刪除引用以假造完整率。

當前資料保留有效模板、參數、選詞、加粗、來源類別、排除與撤回語義；無法無損綁定的譯文依 §2.1 保留候選原稿並列原因，不混入可渲染集合。
舊決定、收據、修訂鏈、舊清冊檔及其環境留在 Git 歷史，現行入口不載入或重播，也不據此補造核可事件；原始來源歸檔的保存責任不變。
