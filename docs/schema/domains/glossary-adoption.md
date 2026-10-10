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

## 6. 公開標註

內部逐 occurrence 與 annotation set 依[四層契約 §8](four-layer-translation.md#8-建置-dbrender-projection-與依賴)，
公開承載依[公開 annotation 契約](../export/public-annotation.md)：translation 的第八欄為 annotation_set_id，原文另由 field_annotation 定位。
只輸出實際引用的概念及非空位置集合，不公開完整建置詞庫、選詞採納或來源收據；不把 HTML／Markdown 塞入 exact text。

加粗由當前 emphasis 與型別推導，綁定精確文字及概念引用的位置；不同概念即使同字也不合併。
EN 卡面顯示 JP 依據繁中時，來源仍為 JP；沒有 EN 對齊資料就不能把相同位置套到 EN 原文。
前端可以關閉視覺加粗，但不能改存放的概念及位置，也不自行維護第二份詞庫。
欄序、Unicode 範圍、引用閉包與 Python／TS 共用反例依公開契約；實際容量與載入用途依[容量契約](../export/size-budget.md)。

## 7. 自動檢查

保留 key／概念唯一、語言與 source span、官方來源分類、必要譯詞、加粗型別及依賴更新的行為反例。
官方／數位來源辨識隨來源資料保存；machine 不因人審變 project。

## 8. 名字顯示與同名瀏覽分離

直接官名依[數位名字規則](digital-name-policy.md)逐 owner 取詞；同名瀏覽本身不證明同概念。
明示名稱選詞／排除可直接修改，不需私人核可收據；優先序必須由資料與程式明示，不能再依 sampled 成員推斷。
