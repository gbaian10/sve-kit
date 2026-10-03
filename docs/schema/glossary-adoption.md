# 術語採納、委託收據與加粗

引用與授權：範例中沿用的官方卡名、商品名、詞彙及卡文片段不在本專案授權內；
專案欄位、合成值、中文說明與資料規則依文件授權。來源及適用範圍見[文件引用說明](../quotations.md)。

本文件擴充 [翻譯契約 §2／§5](translation-contract.md#5-概念選詞與數位證據) 的 authored 入口；來源／模板／跨區政策仍以該文件為準。**2026-10-02 協調者依 Opus 意見、在維護者委託下決定**本文件的採納方式，不記為使用者逐筆親自批准。這是資料與匯入器的契約，尚不宣稱現有 loader 或公開快照已支援擴充。

## 1. 永久概念與引用

概念仍不可變，`id=term:<concept_key>`。key 以英文概念命名，前綴使用穩定的大類，例如 `keyword.storm`、`action.draw`、`zone.cemetery`、`resource.ep`、`trait.fae_touched`；不用草稿列序、譯名字串或可能重分類的 `rule_term` 當前綴。草稿分類調整或譯名改字不能據此重配永久 key；正式概念 payload 仍不可改。配發前先審對照表，再由獲委託的協調者明示核可配發；提案不算已配發。借英文卡面的種族名稱命名 key 僅為內部識別，不等於採納 EN 譯詞或跨區語義。

前綴只是方便記憶的命名；程式不得從 key 前綴推導分類、加粗或行為，必須讀取正式型別、有效採納值及相應契約。

統一的引用值 `TermReference` 恰為 `{kind,key}`：

| kind | key | 建置定位 |
| --- | --- | --- |
| glossary | `term:<concept_key>` 字串 | glossary_term.id |
| vocabulary | `[vocabulary_kind,vocabulary_code]`，兩個字串 | vocabulary 的 `(kind,code)`；本入口 kind=class/type |

草稿 class／card_type 只在對應既有實體卡 vocabulary 時走該入口，不新增假的職業／卡種代碼；其餘一般術語走 glossary rule_term。實體卡職業只有六職業加中立，ヴァンパイア／ネクロマンサー／ネメシス 不進 class；カード 是泛稱，スタートアミュレット 不進 type；evolve／advance／token 依建置契約是特殊標記，本次亦作一般術語，不配成 type。EP／SEP 已有 type；其餘詞彙以已採納 catalog 核對，不從草稿分類猜代碼。EP／SEP 資源與 EP／SEP 卡種分別定位 glossary 與 vocabulary:type；エルフ 職業與 精霊 種族分別定位 vocabulary:class 與 glossary:trait。譯名相同不合併 key，不拿顯示名查概念。

這個引用值供概念定位；現有模板 `parameter_schema.reference_kind` 與 `params` 的公開形狀不在此悄悄更換。建置可把既有 `{kind:term,id}`／`{kind:vocabulary,vocabulary_kind,vocabulary_code}` 無損轉成 TermReference；若日後要更改既有傳輸形狀，先走 §6 的格式審核。

## 2. 專案概念可沒有 raw locator

`glossary_term.data` 完整為 `{id,category,concept_key,source_ref,source_span,authored_source_ja,missing_source_reason,adoption_review}`；category 沿翻譯契約 §5 的五種。加粗值不放進此不可變物件。

來源有且僅有以下一種模式：

| 模式 | 欄位與驗證 |
| --- | --- |
| frozen_source | source_ref 非 null；source_span 可 null；authored_source_ja／missing_source_reason 均 null。以 frozen recipe 的欄位語言 ja、exact 整欄 hash 與 Unicode span 重建 source_ja |
| authored_concept | source_ref／source_span 均 null；authored_source_ja 與 missing_source_reason 都是非空字串。source_ja 原樣取人工採納的概念名稱；理由記未能定位、只屬專案規則詞或草稿含消歧註記等具體原因 |

缺 raw 不等於沒有日文名稱，也不算官方證據。authored_concept 須有效採納，不用空字串／假 source_ref 填洞，不回讀研究草稿。只有 source_ref=null 而缺名稱／理由也拒收。vocabulary label 的專案原文亦可用既有 authored label 入口保存，來源未凍結的理由沿 §3 記錄。

概念首次採納後仍不可改；之後補到 frozen 證據，用新 glossary_choice adoption 附精確日文與目標語證據，不回寫舊概念的缺來源理由。官方選詞仍須驗 evidence 的 ja 等於該概念的 source_ja、target 等於譯詞，以及同概念／語言／來源閉包；不能僅因原來沒有 locator 就豁免這些核對。

## 3. 選詞的主張來源與 origin

`glossary_choice.data` 完整為 `{term_id,lang,value,origin,concept_evidence,source_claim,adoption_review,adoption_no,predecessor}`。vocabulary_choice 將 term_id 換成 vocabulary_kind／vocabulary_code，其餘相同。value、官方 concept_evidence、語言、歷史與明示撤回仍沿翻譯契約 §5。

source_claim 為 null，或恰為 `{source_work,source_urls,claimed_source,note}`：source_work／claimed_source 為非空字串或 null；source_urls 為排序去重的 HTTP(S) URL 字串陣列；note 為非空字串，說明採用理由與證據尚未驗明的部分。URL 不作執行查詢，欄位不視為已 frozen 的 source_ref，沒有 source_record 官方權威。其完整內容參與 record_hash／批次 membership 與 F1 authored pins；改說明或改字都追加 adoption，不改舊紀錄。

**2026-10-02 協調者決定**：沒有官方凍結同概念證據的合作用語與數位用語，先有效採納為 `origin=project`，value 用 authored text，concept_evidence 不捏造；主張的原作、URL、數位版官方出處與尚未驗證理由另記 source_claim。這個 origin 表示本專案採納決定，不能據此在畫面聲稱已驗原作官方來源。現有 origin 列舉不新增「官方未驗」或其他帶官方字樣的新狀態。

之後補足來源與同概念證據，以連續下一個 adoption／正確 predecessor 升級到既有可用的 official_sv1／official_svwb。其他作品的來源即使凍結也不能硬轉為數位版 origin；若需新的官方 authority，另定其證據與顯示政策。本文件不提前授予這種 origin。舊 project 採納與來源主張均保留，新的有效選詞取代舊投影。

已知機器產生的譯詞仍保留 machine，審閱／委託收據本身不改寫其生成 origin；本節的 project 是上述未驗出處用語的明示專案採納政策。效果渲染仍 unofficial，不能因用到數位官方術語就變成官方 SVE 卡文。

## 4. 實際人工或委託採納

本節的四種 glossary kind（glossary_term／glossary_choice／vocabulary_choice／glossary_emphasis_choice）各 data 帶 `adoption_review`，恰為 `{mode,delegation}`。mode=human/delegated_glossary。human 的 delegation=null，維持實際人員／日期／sampled 或 confirmed checked 集的要求。不同 mode 分批；此 confirmed＋AI 審核者例外僅適用上述四種 glossary kind，其他區域不得沿用 delegated_glossary 或省略其既有真人核對要求。delegated_glossary 不用模板的 approved_policy 例外，也不讓裸 model_reviewed 升級。

delegated_glossary 的 delegation 恰為：

```text
{authorized_by,authorization_basis,authorization_date,scope,
 decided_by,decided_at,decided_precision,decision_basis}
```

- authorized_by 記委託的維護者；authorization_basis 與 decision_basis 為非空文字，保存具體委託／追加決定的來源 URL 或正式決定紀錄的章節、原意與事件，不只寫「協調者可代決」。外部頁或正式紀錄未能 frozen 時，收據本身保存足以核對的委託摘錄／出處；這是委託收據，不是官方卡文來源。
- authorization_date 為真實 Date。decided_by 是實際作決定的協調者身分；decided_at／decided_precision 是實際決定事件 Instant／day 或 instant，day 依既有 UTC 午夜編碼。不可沿用另一譯文的時間；只有日期就不捏造時分秒。
- scope 是非空、排序去重的 record_key 字串陣列，精確列出授權與此次決定覆蓋的概念／選詞／加粗紀錄。當前 record_key 必須在內；完整 authored 入口須能驗全部 scope 成員及各自 receipt。不要把一張小範圍收據當永久通用授權，新增詞或新代決須新收據。
- delegation 完整內容放在 data、參與 record_hash，因此收據／範圍／實際決定變更必換不可變 record 與 membership 決定。不可變 glossary_term 的收據與首次配發一起保存，之後選詞／加粗的收據只進各自新 adoption，不回寫概念。
- decision.state=confirmed、sample_ids 為所有 checked record_key；reviewed_by／reviewed_at／reviewed_precision 記此次**實際協調者**與事件，與 delegation 相同。note 明示「維護者委託；協調者決定；不是維護者親自核可」。checked 表示這次授權範圍的完整檢查，不計入 human_sampled_rows／維護者親自確認數。

例如授權依據可以指「2026-10-02〈一致性調整〉、僅進化牌堆／進階起動、授權自行調整後告知」，實際 decided_at 依該追加紀錄的 10-02 事件，不能沿用草稿 10-01 欄。新收據應具體保存來源與範圍；本例是歸因說明，不是已存在的核可收據。

初次遷入的「使用者確認」、「維護者委託協調者選詞」及「協調者核可永久 key」是不同事件，不把第一種譯名核可冒作後兩種的配發／改字核可。每個 record 以實際覆蓋它的事件做 adoption_review。工具 authored_by／authored_at 仍記寫檔工具及真實時間。

## 5. 可修訂加粗

新增 glossary area 的 `glossary_emphasis_choice`，主鍵為 `(term_id,adoption_no)`，record_key 是其 canonical JSON 陣列。data 恰為 `{term_id,value,adoption_review,adoption_no,predecessor}`。value 是 Bool 或 null；非 Bool 的 0／1／字串不接受。只有 glossary_term.category=rule_term 可寫此 kind，其餘概念拒收冗餘覆寫。

採納／分片封套／全 record hash／決定門檻／連續鏈沿翻譯契約 §2。首筆 adoption_no=1、predecessor=null；後續指正確前件，不改舊值、不重配概念 ID。null 表示明示撤回；rule_term 撤回後是 missing_emphasis，不猜 false，也不由草稿分類回補。建置以完整歷史的末筆有效值推導，未採納或模型信心不能決定它。missing_emphasis 時顯示端照常顯示已選譯文與原文、不加粗，並列入報告；這是缺少呈現資料，不是有效 false，也不是整個 context 回原文的理由。首批資料驗收仍要求補齊有效 Bool。

| 正式型別 | 有效加粗 |
| --- | --- |
| glossary keyword／ability／trait／card_name | true，由型別決定，僅在當作術語引用時標示 |
| vocabulary class／type | true，由引用型別決定 |
| glossary rule_term | 必須有有效 glossary_emphasis_choice Bool |

rule_term 逐筆區分名詞與一般敘述：區域名／資源名／聯名專有詞通常 true；一般動詞、數字／數值／費用、連接詞／條件句通常 false。other 逐概念核對；不因出現在術語草稿就全部 true。判斷不了先附具體例子卡的來源 ref／card_no 列待審，不貼整句官文，不在正式入口用未決值假冒採納。

加粗是同一個概念的共用呈現值，不分 JA／繁中另配；這不提供位置。建置 resolver 從有效採納推導 `(TermReference,bold,choice_record_hash?,decision_id?)`，稽核資訊留 F1／source_record／decision_source。非 rule_term 的推導值不需要新增一張永久表，也不在 glossary_term 加欄。rule_term 的值有完整 authored adoption 可驗回。

改加粗只追加 emphasis choice，不能動 glossary_term／glossary_choice 的永久內容。後續位置產生者及顯示依賴鍵須釘有效 emphasis record_hash；加粗改版使選中產物／顯示註記重新推導，純樣式變動不改 exact 譯文字串或規則語義。原文與譯文的引用位置由同一引用關係產生，不能靠中文搜尋同字串猜位置。

首批完整遷入的驗收要求每筆術語或 vocabulary 都有有效 zh-Hant choice，所有 rule_term 有有效加粗 Bool。未驗官方來源用已採納 project 仍是有翻譯；缺任一引用譯詞依既定規則整個 context 回原文，不把 pending 當完成。這是資料驗收要求，不宣稱目前匯入量。

## 6. 公開快照影響與最小擴充提案

**目前格式保持原樣。** [snapshot-format §2](snapshot-format.md#2-公開表完整欄位與玩家用途) 的 translation 只有七欄：id/source_unit_id/target_lang/text_unit_id/origin/authority/status；沒有術語集合、加粗旗標或位置。build-db.translation.tokens 首版固定 null，而且不在公開白名單；本文件不把它當已存在的公開承載欄，不把 HTML／Markdown 標記塞入 exact text。

僅把 rule_term 的 Bool 帶到前端仍不足以實現原文／譯文對照：前端不知道哪個片段引用哪個概念。加粗資訊的出貨流程應是：有效 emphasis＋型別推導值 → 與後續原文／譯文位置產生器的 TermReference 結合 → 隨**選中 translation** 的公開註記輸出。位置與 renderer 仍由後續契約／實作處理；本文件只固定上游引用及值，不讓前端自維第二份詞庫。

建議的最小法（**待獨立格式審核，不是現在的白名單**）：在公開 translation 末尾新增一個 `term_spans` 欄，為帶 TermReference、原文／譯文 Unicode span 陣列與 bold 的具名註記陣列。用已存在的 source_unit_id／text_unit_id 取原文與譯文，不另出全份 glossary 清冊、所有 choice／delegation 收據或新的詞庫附件。沒有術語引用的譯文用空陣列；詳細 tuple 名稱、欄序、跨行／多次引用與 span 覆蓋須和位置契約一起定案，不能先使用任意 JSON。

**公開格式審核必答題**：日文／英文介面只顯示原文、沒有繁中 translation 列時，原文術語位置放在哪裡？原文位置必須跟著原文本身，或至少不依賴任何譯文列存在；格式審核須定出原文位置與 TermReference／bold 的承載、引用閉包及無譯文時的下載／取用，並以日文／英文只看原文的情境驗收。僅在繁中 translation 上附 term_spans 尚不能滿足這項要求，因此下述單欄方向不是完整定案。

本提案不出全份 glossary 表，所以 glossary key 是註記內的自描述概念識別，不冒稱有公開全庫 FK；producer 仍須驗私有建置概念閉包，reader 驗註記與同列原文／譯文 span 的完整性。vocabulary reference 可沿既有公開 vocabulary 驗 `(kind,code)`。若格式審核要求公開 glossary FK，則須另定僅出已使用概念的 descriptor 及其容量，不能讓 reader 向外查最新詞庫補洞。

原文 span 必須綁該 translation.source_unit_id 的 exact 文字。EN 提前顯示共用 JP 繁中時，對照來源仍為 JP；沒有 EN 位置對齊不能宣稱在 EN 原文同位置加粗。同一顯示名字若是不同 glossary／vocabulary，註記 reference 仍不同。前端可關閉視覺加粗，但不得改存放的概念／位置；開關政策沿既有介面決定。

| 影響 | 最小改法與驗收 |
| --- | --- |
| 邏輯白名單與固定 tuple | 經核可後同步 snapshot-format／snapshot-transport 與版本化型別；translation 新欄必有固定位置，不拿可省略欄欺騙舊 reader |
| 版本／能力與嚴格 reader | 決定 format_version／min_reader_version／required_capabilities，舊 reader 明確拒收；不是只改 data_version 就發布 |
| 機器契約與共用樣本 | 修改 source.json，再重產 contract.schema.json／descriptor；Python/TS accessor、reader、independent oracle、golden／invalid fixtures 一起驗新增欄及壞 span／reference／bold |
| producer／projection／引用閉包 | 只出已選譯文註記；span 的原文／譯文存在、Codepoint 範圍、不重疊／多次引用與 reference 目標按新規格驗；不夾帶來源收據／建置 hash |
| 分片／容量／離線更新 | 名稱及 label 仍在 bootstrap、效果在原分片；註記跟選中 translation 同片。不把全術語註記都塞啟動包；重新量測名稱類增加量、1 MiB 啟動包門檻、全文／text_all 聯集與 cache 更新 |

替代方案若另做旁表／附件，會新增集合、join／索引／下載與容器完整性規則，須比較容量；不是零格式變更。本輪推薦單一註記欄以減少新增容器，但**不自行修改**已合併的快照白名單、Schema、tuple、reader 或公開格式版本。原文單獨顯示的承載未定前，不得把此單欄方向直接視為可實作的完整格式。核可與實作前只能出現有純字串，不能宣稱位置加粗／一對一原文對照已上線。

## 7. 實作邊界與反例

新增 kind／data 欄位保持 translation_authored_format=1 的 index／分片封套與既定 hash recipe，僅因制定本擴充時尚無正式翻譯採納紀錄，才可在首次採納前安全調整必填欄位；不是可沿用同編號任意改契約的先例。已有正式採納紀錄後須另審格式版本與相容／遷移策略；有完整 loader 支援後才載入，未支援時明確拒收，不當作空集合或默默忽略。數位 link/coverage 另見[待審的獨立入口契約](digital-link-adoption.md)，不擴充本 glossary 格式，也不宣稱已實作；本文件沒有 EN 術語採納，英文此階段只盤點。

獨立反例至少包括：rule_term 缺值／非 Bool／錯前件／withdrawal；非 rule_term 寫 emphasis；同名不同 reference；收據缺具體委託／scope 不含成員／實際決定者或日期不符／新增詞沿用舊委託；缺 JA 只填 null 或假 locator；來源主張誤升官方、官方 choice 無真 frozen concept evidence；有效 vocabulary 引用有缺譯；加粗改版未重新推導顯示。這些是驗收要求，不是已實跑結果。公開註記的反例隨格式審核另加，不用修改現有快照讓本文件通過。

## 8. 名字顯示與同名瀏覽分離

[數位名字政策](digital-name-policy.md) 的直接官名不採納同概念，不供glossary_choice.digital_name證據，
不擴大delegation或把project／machine升成official。真人親自詞優先只計confirmed checked／sampled實際sample成員，
sampled非樣本依真實origin放其他合法選詞順位。直接名字與term引用不同需依精確委託scope續版choice或交維護者，
不能借same_name或名字字串配概念／語音；不修改264概念與39既有分片。
