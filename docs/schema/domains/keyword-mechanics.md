# 關鍵字登錄與自身宣告投影

自身宣告（內部代號 H1）指「整行只有關鍵字」，表示卡片本身具有該能力。

本入口依[建置資料庫 §10](../build/build-db.md#10-dsl驗證與未實作卡片頁)與
[快照三態](../export/snapshot-format.md)提供查卡正標籤，不產生 DSL，不變更引擎支援狀態。

## 採納輸入

`authored/rules/keyword-registry.yaml` 是唯一入口；有界 YAML 讀取器拒絕 symlink、
未知欄位、非整數 format、重複 ID／code／term_id。形狀如下：

```yaml
format: 1
kind: keyword_registry
decision:
  id: decision:keyword-registry-v1
  state: confirmed
  note: Adopt the explicit existing keyword concepts for the manual H1 source adapter.
records:
  - {id: 'kw:ward', code: ward, term_id: 'term:keyword.ward'}
suppressions: []
```

records 明示永久 `kw:<code>` ID、英文 code 與既有 glossary term ID。
目前只登錄 `category=keyword, kind=printed_keyword`，definition 為 null、actions 空。
缺 term、非 keyword 類別或多重概念不能靠拼寫補配。日文名稱及多語 label 使用同一
已採納 glossary 概念的選詞與 low_confidence，不另維護譯名。保留 choice 的 origin、
source_id 與來源權威；已驗的官方數位詞典選詞為 official／digital_official，
不得改成 project／unofficial，也不得冒充實體 SVE 官文。品質合併 term／choice 的 low_confidence。

decision 是 mapping 與有限 H1 adapter 的明示採納，不是逐卡真人審核、DSL review
或 engine pass。建置保存 registry exact bytes 的 source_record、decision 及引用；
F1 configuration 納入 registry hash 與 adapter 版本。
此檔與舊 `authored/rules/keywords.yaml` 的 astra/1 原型是不同入口；後者不會被這條管線匯入。

## 來源與關係

只檢查已確認 card 的 JP face_current 主效果，逐面保留 exact owner。
雙面卡採「任一面有即標 has」：任一面證明自身宣告，就在 card 層級標記，
不表示正面一定具有該能力；公開投影沒有面向維度。
預設建置由已審查解析輸入取得來源版本及 locator；sealed/raw bytes 在產生解析資料及明示 `archive` 模式驗證，
不在普通建置重跑，見[解析卡片輸入〈建置入口〉](../ingest/parsed-card-inputs.md#建置入口)。更正沿用既有 corrected-source recipe。
空主文只記未抽取，不授予完整 coverage；不合併所有 printing、歷史 revision 或他 owner binding。

H1 只接受 `card_keywords.v2` 的完整 body part：整個片段由唯一解析的 keyword
宣告組成，葉皆為 Concept／keyword／GlossaryReference。SourceBinding 與來源位置重驗後，
每個已登錄概念產 `relation=has`。同欄位其他片段未翻譯或 pending 不妨礙這個正標籤，
卻仍阻止完整 coverage。不依翻譯 target、annotation、加粗或 HTML 圖示推論。

授予、選擇／數量參照、引號內能力、未知句型及 sections 均不由 H1 產生標籤。
token 定義的 keyword 不表示原卡自身具有。H1 不產 grants／refers／counts，
沒有標籤亦不代表沒有能力。

## 新鮮度與補正

`mechanic-source-bundle-v1` 對 card／shared 全部面（含缺 current 的面）、
所選 revision／來源、current basis、主文及有序 section kind／exact hash 計算 source_text_hash。
其他面、補充段落或 basis 變動因此使舊標籤失效；bundle 本身不是完整語意檢查證明。

producer_version 包含 `manual-keywords-h1-v1`、來源 normalizer／葉規則、keyword mapping
與 keyword 概念集合，不含中文呈現、無關 commit 或 suppression。
抽取或 bundle 演算法變更必須更新 adapter／recipe 版本；keyword/action universe 與 coverage 每次重建。

card_mechanic 使用 `source_kind=manual, dsl_id=null`，source_id 指實際原文，
decision_id 指採納的 mapping。候選 ID 由精確 binding、card／keyword／scope、
source hash 與 producer 決定。fresh 正標籤依 `(card_id,keyword_id,scope)` 合併，
relations／actions 排序去重，不公開原始證據或 unknown／absent 列。

選填 suppressions 每筆必有 `id,target_tag_id,source_text_hash,producer_version,reason,decision`；
decision 同上例且必須 confirmed。只有候選 ID、source hash 及 producer 完全相符才寫入並套用；
不相符報 stale_suppression，不遮蔽新候選，不把 partial 改 complete。
補正使用獨立的 mechanic_suppression decision category；只有實際寫入 override
時才保存其 decision 與來源，不為 stale suppression 留下未使用的 confirmed decision。

## Coverage 與承載

H1 一律寫已檢查 keyword 的 partial；公開每 card／shared 一列，complete_all=false、
complete 集合空。partial 選較短 include／exclude，同長選 include；只投影 fresh coverage。
has present 不證 grants absent；缺標籤且 partial／缺 coverage 仍 unknown。
此 producer 拒絕 complete／unreviewed 來源列冒充其檢查結果。

目前不產 en_override，不授予 shared 跨區資格；reader 沿既有 aligned／region block 條件，
不能因 JP 繁中可顯示就套用 EN 機制。分母依 manifest 全地區 non-retired，含 provisional。

公開 keyword／mechanic_projection／card_mechanic_coverage 使用既有欄位與分片配置；
前者在共用 catalog，後兩者在 card 原有 engine 檔案。不改格式、capabilities、band width，
不新增邏輯分片。report.mechanics 保存 adapter／producer、證據／投影／coverage 列數及
拒絕原因，不含官方句子。建置 keyword 子能力包含七張機制表，DSL 仍未啟用。
