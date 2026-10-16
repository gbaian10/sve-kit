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

### 4.1 詞表選詞投影

匯出時由 `export/project/translations.py` 將已解析的當前詞表選詞投影為 vocabulary 的 `label` 翻譯，
沿用普通翻譯與文字閉包。此時 catalog 與 glossary 均已建好，無須在 catalog 匯入時提前解析詞表，
也不讓前端另讀 authored 或推導概念對應。

族群只接存在且 category 為 trait 的 `term:trait.<code>`；`cutthroat` 明示對應 `term:trait.killer`
（既有 raw mapping 的「キラー」）。`new_year` 尚無對應選詞，保持缺譯；不更動 code。
特殊標記 token 與 `term:object.token` 同指衍生物，可共用選詞。
作品沒有同概念的既有選詞；evolve／advance 標記不借用 evolve_marker／advance_marker 物件名。
這些缺項維持原有非空退回，不以相近字串猜譯文。卡種 leader 仍由 vocabulary 的 `value.translations` 提供「主戰者」。
EP、SEP 與稀有度縮寫不投影詞表。

投影讀取 `glossary_translation` 已解析的文字，包含 authored 與 source choice；保留 choice 的 origin，
低信心合併概念與 choice。官方選詞沿用 digital_official 權威，不升為 SVE 官方翻譯。
凍結來源與 authored 來源紀錄保留在建置 DB 的 glossary 與 source_record。投影 ID 由詞彙主體與選詞的概念、文字、品質、凍結來源推導，
不含帶建置 revision 的 authored 來源紀錄，選詞未變時重建 ID 不變；
公開快照沿用既有 vocabulary label 指標，不公開私人來源定位。重建時選詞改字即更新標籤譯文。

## 5. preview 與前端驗收

以有效採納推導的 JP／EN 映射重建 preview，保留舊版，逐項列出保留 code、暫碼替換、新增標記及未覆蓋原值，重驗 class/type/special_kind 引用閉包。其餘 preview 詞彙不能因本次職業／卡種採納而自動升為正式資料。

前端核對職業快速列、卡種篩選、特殊標記及牌組條件，包含 neutral、ep／sep、equipment／crest、進化／進階／衍生物實例；寫死暫碼或靠原文猜卡種的地方須清掉。JP／EN 同概念同 code，未知 raw 拒絕，缺譯降級分別驗收。公開欄位及格式版本不變、資料版本更新，詳見 [snapshot-format](../export/snapshot-format.md)。
