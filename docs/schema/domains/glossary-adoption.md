# 術語、選詞與加粗

本文件補充[翻譯當前資料格式](translation-contract.md)。概念、選詞與加粗全部保留，
不再使用委託收據、採納序號、成員雜湊或人員／日期驗證；改字與加粗可直接修訂，Git 保存歷史。

## 1. 永久概念與引用

概念 ID 為 `term:<concept_key>`，以英文概念命名，例如 `action.draw`、`zone.cemetery`；
不以草稿行號或譯名配號，不隨分類及譯名改字重配。同名不同概念分開，key 前綴不推導型別。
category=keyword/ability/trait/rule_term/card_name。class/type 是 vocabulary 代碼，不另造同義 glossary 代碼。
EP／SEP 的資源與卡種、職業與同譯種族仍分別定位，不因中文同字合併。

TermReference 恰為 `{kind,key}`：glossary 的 key 為 term ID，vocabulary 的 key 為 `[kind,code]`。
模板內既有 term/card/vocabulary 參數可轉成此引用；相同顯示文字不是有效引用。
既有 code 必須存在，未知 class/type 不從草稿分類自動配發。

## 2. 概念來源

glossary_term.data 為 `{id,category,concept_key,source_ref,source_span,authored_source_ja,missing_source_reason}`。
missing_source_reason 缺欄位時為 null，寫出省略 null；自撰來源的非空原因仍必須保存。
source_ref 非 null 時以其 exact 字串及可空的 span 重建日文名稱，後兩欄均 null；
沒有來源定位的專案概念則 ref/span 均 null，後兩欄為非空名稱與簡短理由，不造假 locator。
origin／low_confidence／note 在 record 外層；source_ref 不保存私人 pathname 或核可頁引用。

## 3. 選詞的主張來源與 origin

glossary_choice.data 為 `{term_id,lang,value,concept_evidence}`，另有選填的 source_claim。
每個鍵只保存當前值，value=null 是撤回，不能偷偷退回歷史舊值。
value 非 null 時為 `{kind:authored,text}` 或 `{kind:source,source_ref,span}`；span=null 表示全字串。
文字與來源語言必須正確，span 為 code-point 半開區間，不得越界。

source_claim 可省略；有值時為 `{source_work,source_urls,claimed_source}`。
沒有作品、URL 或出處主張時省略即可。
source_work／claimed_source 可省略，source_urls 是 HTTP(S) URL 陣列。不使用 URL 查詢、私人檔案或 hash 來證明核可。
既有未驗來源的專案採用詞仍 project，已知機器生成仍 machine；補到官方來源可直接修正為 official。
分類簡化不抹去已知第三方來源，亦不把 project 分類當成權利聲明。

concept_evidence 為下列物件陣列；official 必有與 value 相符的同概念來源，其餘可空：

| kind | 其餘欄位與自動檢查 |
| --- | --- |
| digital_name | `digital_face_id,sve_owner,jp_ref,target_ref`；實際同卡精確面關係及語言／名字要吻合，不保存 decision_id |
| effect_term | `jp_ref,jp_span,target_ref,target_span,concept_note`；同一來源卡的對應欄位、exact 摘錄與非空概念說明 |
| dictionary_entry | `dictionary_kind,entry_key,jp_ref,target_ref,concept_note`；skill_names/tribe_names 的相同官方 key |

來源 bytes 與定位由建置自動驗；一般 reader 不重新執行歷史 parser 或核可流程。
官方來源標記不能由同頁共現自動推論同概念。數位官名、術語與整段 SVE 效果的 authority 不相互授予。

## 4. 修改與載入

翻譯 pin 使用的具名替代值沿 translation-contract §2 的 glossary_choice_variant；
不參與一般自動選詞，仍驗同概念／來源／語言。普通撤回不會自動選候選；明示 pin 才可選它。
選詞、分類與說明直接修改當前 record；保留 term ID，不製作新 approval／delegation／membership。
撤回保留 null 值，讓自動供詞不意外復活明示停用項；刪除概念前須確定沒有依賴。
語義不同則配不同概念。採納程序由一般 PR 負責，loader 不驗人名、時間、樣本數或雙模型。
低信心資料通過結構檢查後可用，顯示「待校對」；未知 key、錯來源或缺必要譯詞照常拒絕／回原文列清單。

## 5. 可修訂加粗

glossary_emphasis_choice.data 為 `{term_id,value}`，value 為 Bool 或 null；只允許 rule_term。
其他 glossary category，以及 vocabulary class/type，在術語引用位置的 bold 固定 true。
rule_term 用當前 Bool；null／缺值時列 missing_emphasis，仍可顯示譯文但不加粗，不能把未決值當 false。
一般動詞／連接詞通常不加粗，區域／資源／專有詞依概念明示；不用詞庫中的出現次數決定。

加粗改值重產相依位置，不改譯文字串、不重配概念，也不要求重簽。原文及譯文位置
由同一概念引用產生，不用字串搜尋取代語義定位；語義相同的引用共享設定。

## 6. 公開快照影響與最小擴充提案

四層的內部逐 occurrence／annotation set 以[共用契約 §8](four-layer-translation.md#8-建置-dbrender-projection-與依賴)為準；
以下只記舊公開格式與早期承載提案，正式 wire 選擇由 #496 定義，不能據此新增欄位或省略原文註記。

**目前格式保持原樣。** [snapshot-format §2](../export/snapshot-format.md#2-公開表完整欄位與玩家用途) 的 translation 只有七欄：id/source_unit_id/target_lang/text_unit_id/origin/authority/low_confidence；沒有術語集合、加粗旗標或位置。build-db.translation.tokens 首版固定 null，而且不在公開白名單；本文件不把它當已存在的公開承載欄，不把 HTML／Markdown 標記塞入 exact text。

僅把 rule_term 的 Bool 帶到前端仍不足以實現原文／譯文對照：前端不知道哪個片段引用哪個概念。加粗資訊的出貨流程應是：當前 emphasis＋型別推導值 → 與後續原文／譯文位置產生器的 TermReference 結合 → 隨**選中 translation** 的公開註記輸出。位置與 renderer 仍由後續契約／實作處理；本文件只固定上游引用及值，不讓前端自維第二份詞庫。

建議的最小法（**待獨立格式審核，不是現在的白名單**）：在公開 translation 末尾新增一個 `term_spans` 欄，為帶 TermReference、原文／譯文 Unicode span 陣列與 bold 的具名註記陣列。用已存在的 source_unit_id／text_unit_id 取原文與譯文，不另出全份 glossary 清冊、內部選詞資料或新的詞庫附件。沒有術語引用的譯文用空陣列；詳細 tuple 名稱、欄序、跨行／多次引用與 span 覆蓋須和位置契約一起定案，不能先使用任意 JSON。

**公開格式審核必答題**：日文／英文介面只顯示原文、沒有繁中 translation 列時，原文術語位置放在哪裡？原文位置必須跟著原文本身，或至少不依賴任何譯文列存在；格式審核須定出原文位置與 TermReference／bold 的承載、引用閉包及無譯文時的下載／取用，並以日文／英文只看原文的情境驗收。僅在繁中 translation 上附 term_spans 尚不能滿足這項要求，因此下述單欄方向不是完整定案。

本提案不出全份 glossary 表，所以 glossary key 是註記內的自描述概念識別，不冒稱有公開全庫 FK；producer 仍須驗建置概念引用，reader 驗註記與同列原文／譯文 span 的完整性。vocabulary reference 可沿既有公開 vocabulary 驗 `(kind,code)`。若格式審核要求公開 glossary FK，則須另定僅出已使用概念的 descriptor 及其容量，不能讓 reader 向外查最新詞庫補洞。

原文 span 必須綁該 translation.source_unit_id 的 exact 文字。EN 顯示 JP 依據繁中時，對照來源仍為 JP；沒有 EN 位置對齊不能宣稱在 EN 原文同位置加粗。同一顯示名字若是不同 glossary／vocabulary，註記 reference 仍不同。前端可關閉視覺加粗，但不得改存放的概念／位置；開關政策沿既有介面決定。

| 影響 | 最小改法與驗收 |
| --- | --- |
| 邏輯白名單與固定 tuple | 經核可後同步 snapshot-format／snapshot-transport 與版本化型別；translation 新欄必有固定位置，不拿可省略欄欺騙舊 reader |
| 版本／能力與嚴格 reader | 決定 format_version／min_reader_version／required_capabilities，舊 reader 明確拒收；不是只改 data_version 就發布 |
| 機器契約與共用樣本 | 修改 source.json，再重產 contract.schema.json／descriptor；Python/TS accessor、reader、independent oracle、golden／invalid fixtures 一起驗新增欄及壞 span／reference／bold |
| producer／projection／引用閉包 | 只出已選譯文註記；span 的原文／譯文存在、Codepoint 範圍、不重疊／多次引用與 reference 目標按新規格驗；不夾帶來源收據／建置 hash |
| 分片／容量／離線更新 | 名稱及 label 仍在 bootstrap、效果在原分片；註記跟選中 translation 同片。不把全術語註記都塞啟動包；重新量測名稱類增加量、依所選版本的基本目錄 Brotli 2 MiB 分界（略超報精確差額、明顯超出才交維護者，量法依[容量契約](../export/size-budget.md)）、全文／text_all 聯集與 cache 更新 |

替代方案若另做旁表／附件，會新增集合、join／索引／下載與容器完整性規則，須比較容量；不是零格式變更。本輪推薦單一註記欄以減少新增容器，但**不自行修改**已合併的快照白名單、Schema、tuple、reader 或公開格式版本。原文單獨顯示的承載未定前，不得把此單欄方向直接視為可實作的完整格式。核可與實作前只能出現有純字串，不能宣稱位置加粗／一對一原文對照已上線。

## 7. 自動檢查與轉換

保留 key／概念唯一、語言與 source span、官方來源分類、必要譯詞、加粗型別及依賴更新的行為反例。
舊收據與採納鏈只在轉換時取最後有效值，不再進新分片或一般讀取。
舊 origin=official_sv1/official_svwb 在新 authored 統一為 official，provider 仍由來源資料保存，
公開的數位來源辨識不能丟失；machine 不因人審變 project。

## 8. 名字顯示與同名瀏覽分離

直接官名依[數位名字規則](digital-name-policy.md)逐 owner 取詞；同名瀏覽本身不證明同概念。
明示名稱選詞／排除可直接修改，不需私人核可收據；優先序必須由資料與程式明示，不能再依 sampled 成員推斷。
