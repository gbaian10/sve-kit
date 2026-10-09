# 職業、卡種的正式 catalog 輸入

引用與授權：範例中沿用的官方卡名、商品名、詞彙及卡文片段不在本專案授權內；
專案欄位、合成值、中文說明與資料規則依文件授權。來源及適用範圍見[文件引用說明](../../quotations.md)。

本文件細化[詞彙當前格式](catalog-route-adoption.md#41-詞彙與語言)，不新增第二份代碼表。

## 1. 唯一入口與建置

權威是當前工作樹 catalog/adoptions 固定 area 中 format 2 分片的 vocabulary_adoption；不使用 checksum index。
subject 為 `{kind,code}`，value 為 `{label,raw_mappings,active}`；基本原值映射與語言檢查保留。
一般讀取驗結構／引用，建置從本次來源自動驗 exact 原值；不掃歷史採納或重播私人核可。
Catalog／Vocabulary 記憶體物件由此推導，不是另一份權威。

## 2. 永久代碼

`(kind,code)` 不隨譯名重配，停用亦不重用；新增及修改在一般 PR 更新當前 value，自動驗原值映射，不附事件收據。代碼範圍如下：

| kind | code 清單 |
| --- | --- |
| class | elf、royal、witch、dragon、nightmare、bishop、neutral |
| type | follower、spell、amulet、crest、equipment、leader、ep、sep |
| special_kind | evolve、advance、token |

保留 preview 已使用的六職業與 follower／spell／amulet／leader，修正 neutral 與其他基本卡種的暫碼。`raw_jp_…`／其他 `raw_…` 不繼承為永久 code；已有正式採納的 code 改動須另審遷移與資料改版。特殊標記不是基本卡種，不擴充本次 label 翻譯的 kind。

## 3. JP／EN 原文映射

以既有有效映射為對照，不用私人 preview 檔作載入條件。JP／EN 在同一採納入口以 `{region,lang,raw,source_ref,special_kinds}` 保存完整 exact 原值；特殊標記的欄位、依賴與來源核對沿契約 §4.1。`domains/registry/inputs.py::CLASSES` 只供一致性檢查，不是配發收據；`-` 沿缺值 recipe 投影 null，不配成職業、不冒作 neutral。

例如經來源核對的「フォロワー・エボルヴ」／「Follower / Evolved」在 type:follower 的 raw_mappings 帶 `[evolve]`；「イクイップメント・トークン」在 type:equipment 帶 `[token]`。這些是示意，不宣稱實際資料已核對。每個完整 spelling 保留自己的凍結 source_ref，不切字後假造 ref。不靠通用 split／trim／大小寫轉換猜新對應；未知原值拒絕／列缺項，不生成暫碼。新增 spelling 或修訂標記直接改當前值，自動重驗唯一映射。

## 4. 15 筆繁中選詞

七職業、八基本卡種的 zh-Hant 選詞屬 #51，存在該詞彙記錄 `value.translations`（`lang,text,origin,low_confidence`），label 仍只存日文基準。code 配發不授予翻譯權威；origin、低信心及撤回依[翻譯契約](translation-contract.md)。resource EP／SEP 與 type EP／SEP、trait 精霊與 class エルフ即使同譯仍分別定位，不合併 glossary。EN 原值映射不等於此次採納 EN 術語翻譯。

缺有效繁中選詞時，模板的整個 context 回原文並報 `missing_term_translation`；UI fallback 不套用卡文。標籤翻譯選用投影屬 #53，必須驗有效 catalog 主體與當前選詞，不由相同字串或裸 FK 借用其他概念的翻譯。

## 5. preview 與前端驗收

以有效採納推導的 JP／EN 映射重建 preview，保留舊版，逐項列出保留 code、暫碼替換、新增標記及未覆蓋原值，重驗 class/type/special_kind 引用閉包。其餘 preview 詞彙不能因本次職業／卡種採納而自動升為正式資料。

前端核對職業快速列、卡種篩選、特殊標記及牌組條件，包含 neutral、ep／sep、equipment／crest、進化／進階／衍生物實例；寫死暫碼或靠原文猜卡種的地方須清掉。JP／EN 同概念同 code，未知 raw 拒絕，缺譯降級分別驗收。公開欄位及格式版本不變、資料版本更新，詳見 [snapshot-format](../export/snapshot-format.md)。
