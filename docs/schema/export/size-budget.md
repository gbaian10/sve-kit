# 卡表快照容量與記憶體預算

本文件定義容量帳的對象、量法與設計門檻；分片與欄位白名單依 [snapshot-format](snapshot-format.md)，容器與 join 依 [snapshot-transport](snapshot-transport.md)。
基本目錄的載入分類與容量分界依 [#506 的維護者決定](https://github.com/gbaian10/sve-kit/issues/506)。
N0 的 producer／reader 已接線；下列基本目錄裝檔與部分 reader 語意是設計要求，不宣稱現有 exporter／Worker 已完成或手機效能已通過。

## 基本目錄與載入分類

基本目錄是所選版本（jp／en）那一區查卡搜尋需要的完整基本資料，與 UI 語言分開。
包含該區全部卡片／面、current／暫顯 revision、卡號定位、名稱及可用名稱翻譯、基本數值／facet 及所需共享字典；pending／未知狀態保留，缺譯回原文。
卡包篩選使用永久 printing.home_set 的投影，不由卡號前綴猜測。基本文字與日文依據／annotation 的就緒狀態依[公開 annotation §4.1](public-annotation.md#41-三種就緒狀態與部分-reader)。

| 類別 | 內容與時機 | 容量帳 |
| --- | --- | --- |
| 首屏必載 | app 外框、首頁與隨 app 打包的共用佔位圖；首頁不等快照 | app 與首屏實際請求另報；實際阻擋首屏的快照資料不能隱藏成背景 |
| 背景預載 | 首頁顯示後下載所選區基本目錄；整區到齊並驗畢才搜尋；之後補印刷稀有度、異圖與完整版次 | 基本目錄冷載與後續印刷增量分列，所有連線同一政策 |
| 按需載入 | 另一區在切換或對照時下載；效果全文及其譯文、日文依據／標註、商品收錄、引擎支援、Q&A／CR／裁定、printed／history、說明正文與 media 按用途載入 | 首次來源對照、詳情、共享依賴與必要跨區增量分列 |

下載中可打字，顯示「資料下載中」；整區基本目錄及搜尋索引完成後自動搜尋一次，不顯示部分結果。
印刷 facet 尚未補齊時顯示未備妥，使用該條件須等其完整閉包，不能以已載入版次形成部分命中。
不因另一區尚未下載而刪掉已採納永久 ID／引用，也不因容量排除卡片、名字、譯文或 pending。

## 預算

MiB = 1,048,576 bytes，KiB = 1,024 bytes。raw 是 canonical-json-v1 未壓縮 bytes；Brotli quality 11，gzip level 9（mtime=0、無檔名），recipe 釘住壓縮器及版本。
多檔帳的壓縮量是各 File（含 manifest）各自壓縮後 bytes 的和，不以合併成單一串流的壓縮量代替。

| 項目 | 門檻／分界 | 對象與量法 |
| --- | --- | --- |
| 基本目錄冷載，依所選版本 | Brotli 2 MiB（2,097,152 bytes）分界 | 完整 manifest＋config＋整區基本目錄實際 File 及必要依賴；按 key 去重 |
| 基本目錄容器 | 解壓後 ≤ 2 MiB | 穩定封存檔、最近包各自的檔與共用檔；整個 File 的 types、字典、fragment metadata、rows 都計入 |
| 詳情與其他資料檔 | 解壓後 ≤ 512 KiB（524,288 bytes） | 基本目錄容器以外裝 fragment 的實際 File，含詳情、印刷擴充、history 與 images；整檔計，不只量單一 fragment；config／programs 另報 |
| 全部文字 raw | ≤ 40 MiB（41,943,040 bytes） | 全部分片的 bootstrap／text／config 聯集＋manifest，兩區合計；changes 另報 |
| 完整文字閉包，依單一版本 | Brotli ≤ 8 MiB（8,388,608 bytes） | 該區全部文字及其完整引用閉包＋manifest，含共享、JP 來源與必要跨區資料 |
| 完整文字閉包，依單一版本 | gzip ≤ 10 MiB（10,485,760 bytes） | 與 Brotli 相同的實際 File 集合，不能改用只含顯示譯文的集合 |

基本目錄略超 2 MiB 可接受，報精確 bytes、差額 `actual_br - 2,097,152` 與比例；明顯超出才交維護者看數字與改善方案。
不自行發明「略超」的百分比硬門檻，不把跨過分界一律當停止條件，也不自行放寬分界。
raw／gzip 另報基本目錄實測；不另設其啟動壓縮門檻。
完整文字與單檔的上限仍是發布效能驗收門檻；超限須調整投影或固定配置，不能宣稱通過。
容量不是快照格式或 reader 相容性條件；reader 核對實際 bytes、hash、結構與閉包，不以預算拒絕合法快照。
manifest、changes、text_all 的封套與整檔量另列，不套資料 File 的單檔上限，仍須量其解析記憶體。

## 容量帳與完整來源

基本目錄、後續背景與各按需用途均列實際 File key 集合、raw／gzip／br、檔數及依賴。
依單一版本計壓縮完整文字時，從該區所有文字用途取傳遞閉包，不限當前 UI 語言、current 或基本目錄：
完整 source 原文 owner、text_unit、sections、annotation，official_counterpart 雙端，必要跨區 card／face／printing／revision、說明正文、卡號索引與所有字典都必須計入。
EN 的 JP 依據即使平常按需才下載，仍屬 EN 完整文字帳。共享／混區 File 整檔計入每個需要它的版本，不按列數或語言比例分攤。

同一用途帳內相同 File key 只加一次；相同文字／字典若實體重複裝在不同 File，每份都計，不能用邏輯去重掩蓋實際傳輸。
全量 raw 帳對 files 中 role=bootstrap/text/config 的所有 File 按 key 去重，再加 manifest；其他用途不能改列 DSL／debug 逃避文字門檻。
單區帳加 manifest 與 config 一次；兩個單區帳可能都包含同一共用檔，不能相加冒充全量。
基本目錄是單區完整文字的子集，背景／按需增量扣除該 session 已取得的 key；不再把子集加進全量帳。

text_all 是同批原 File 聯集的替代下載：另報封套與整檔 raw／gzip／br，不與分片重複加總，也不以單流較佳壓縮率代替分片門檻。
annotation before／after 的 raw 增量、manifest／changes 摘要與分片重複 descriptor 另列；已含在 File 帳中的增量不重加。
圖片 bitmap、語音與 DSL AST 不屬完整文字；images metadata 另報，實際進入某冷載用途者仍加進該用途帳。

表記未定的公開呈現須量測日版／英版基本目錄，包含當次實際可用的名稱翻譯；manifest、config、實際必載容器與 pending wording 引用依上述各版本 Brotli 分界計算。基本目錄須計入每個 pending face-region 最多一筆 display revision 的輕量投影、名稱及可用名稱翻譯文字閉包，讓整區完成後可讀暫顯卡名、搜尋名稱與建立 facet。display_ref 以外的候選 revision 與文字閉包按需載入，另報容量，不算成已完成候選索引；不能讓其餘候選引用形成必載依賴後仍漏算基本目錄成本。

## 手機記憶體與解析

以下是設計門檻，以中階 Android 與 iPad 的實測為準；桌面量測不能代替。

| 項目 | 門檻 | 對象與量法 |
| --- | --- | --- |
| 穩態 | main＋Worker 合計 ≤ 48 MiB | 整區基本目錄驗畢、索引建立並釋放原始 tuple／解碼字串後，以及詳情／標註／切換用途各自的穩態場景；JS heap＋ArrayBuffer、TypedArray、唯一字串池、索引、詳情 LRU、當頁 view／annotation 與 pinned 工作集 |
| 更新峰值 | main＋Worker 合計 ≤ 80 MiB | 從舊 active 開始更新到新 active 啟用、舊暫存釋放的同時存活最高值，含解壓、解析、傳訊複本與 staging 索引；不把兩端分開驗收 |
| 主線程單段工作 | ≤ 50 ms | 真瀏覽器 performance trace 中快照解碼／驗證／建索引／合併／呈現造成的每個連續主執行緒工作段，包含 Worker 結果的主端處理 |
| 完整解析 | 累計 ≤ 1 秒 | 所選整區基本目錄 bytes 可用後至可搜尋，解壓、解碼、驗證、建索引及主端合併各段有效工作時間（含 Worker）累計；另報牆鐘時間、排程等待與網路時間 |

48／80 MiB 不含 app 自身、WebGL 與圖片 bitmap；這些另報，不能把文字 store 歸入 app 排除。
CacheStorage／IndexedDB 的持久 bytes 是磁碟帳，不能當 JS heap；重複存在於 main／Worker 的文字與索引均計其實際配置；量測報表若已含 ArrayBuffer，不再加一次同一配置。
更新峰值以同時刻的 main＋各 Worker 合計為準，不把各自不同時刻的峰值當穩態。
reader 保留 TypedArray、緊湊欄式 store 與唯一字串池；view 只持 ID／ordinal，不閉包引用整列解碼物件。
詳情與影像 metadata 按頁解析並釋放；當頁 UTF-16 前綴表、ranges／說明 view、衍生 Map、pin 與傳訊暫存都列入。
影像 metadata 的 12 MiB raw 對應量／64 檔調度上限不等於 heap，基本目錄容器 2 MiB 也不等於 JS 配置量。
桌面 tuple heap、下載壓縮率、合成案例或 ID URL 不能證明手機通過。

## 3.0 annotation 與 JP 來源的計帳

[公開 annotation 契約](public-annotation.md)的必要資料全部屬完整文字預算：
annotation_set 的 ID／exact text／occurrences／ranges／bold、field_annotation 的 owner／field／ordinal、
annotation_concept 的 category／card_ids／explanations、translation 第八欄、FieldTranslation 的 source／counterpart、CR translations，及其完整原文／譯文／詞彙／卡片／說明閉包。
同文字不同概念的集合不能算成同一份；只有完整 set 相同才去重。
空集合不出 annotation_set／field_annotation，translation 保留第八格 null；不改建置端空集合身分。
容量報告列實際非空集合／用途列數、空集合省略量；producer 逐用途確保非空 occurrence 不被省略。
容量以實際輸出計，不以推定可省略的集合量代替實測。

基本目錄可先達基本文字就緒而日文依據／標註未備妥；完整文字帳不因此縮減。
首次來源對照、printed／history、概念說明另報按需增量；若實際成基本目錄必載依賴，就回基本目錄帳。
文字、只改 bold、概念說明等更新，各自報重建檔數及下載 bytes，base／字典變更的重抓亦計入。
N0 已完成完整輸出量測，既有超標依維護者豁免交付；該豁免不代表新裝檔或手機驗收通過。
