# 模板歷史清冊的凍結語義重算

本契約細化 [翻譯契約 §3](translation-contract.md#3-模板來源清冊與-id) 與
[風味文字契約](flavor-translation.md)。歷史清冊採 **C+hash**：在目前受審的 carddb 中，
**以凍結語義版本重算並逐項比對輸出 hash**。每次仍重讀封存來源並驗完整閉包；
清冊指定的 producer 不必等於當次執行程式，整個 installed runtime／lock 相等不是歷史清冊的門檻。
本文件定義格式及實作驗收，格式成立不代表來源重算能力或性能已通過。

不新增採納政策、真人抽查或模型互審要求，不改模板／譯本 payload、ID 或公開快照。
所有歷史清冊均驗，包括未被當前定義引用者。來源、raw、角色、owner、pending、coverage、
碰撞、schema 與採納決定的原有檢查均保留；hash 摘要不能代替這些檢查。

## 1. 清冊 v2 與首次採納門檻

位置仍為 `authored/translations/template-sources/<sequence>.yaml`，由 translations index 的
inventories 釘完整解析值 canonical hash；沿嚴格 YAML、安全路徑、完整 Git 歷史與單檔大小規則。
封套恰為：

```text
{template_source_format:2,kind:template_source_inventory,recipes,replay_context,entries}
```

format 只收整數 2，不接受 bool／字串。recipes 六欄與 entries 八欄沿翻譯契約，不增原文欄。
所有物件封閉，必填欄不補默認。新來源清冊使用 v2；v1 不具完整 expected baseline，
不能猜 expected、原地升版、改 producer SHA 或借 v2 稱為已完成 C+hash。
若正式化前發現已採納 v1，停止新採納並另審有依據的追加遷移，歷史 bytes／索引 hash 保留。

格式 reader 可以先支援 v2；來源語意入口尚未接通時必須明確拒絕，不能只驗封套便回傳成功。
模板與 glossary reader 同時驗完整 foreign v2 結構，不忽略新清冊，不遞迴呼叫同一個 source replay；
引用循環、未知語義版本均明示拒絕。glossary 的 foreign 結構驗證不宣稱完成模板來源重算。

首份正式清冊的 gate 為：凍結版本與完整 C+hash 能力已合併 main → 用實際已合併 producer
固定來源生成 → 獨立重新載入、重算核對 → 完成工程審查及該 kind 的實際採納。
在此以前不把效果或風味清冊寫進 authored。候選、性能報告及確認頁仍在私人區。

## 2. replay_context 的封閉格式

replay_context 恰為：

```text
{format:1,semantic_bindings,environment,inputs,expected_outputs}
```

format 是 strict integer 1。Git revision 是完整 40 小寫 hex，Hash 是 `sha256:`＋64 小寫 hex；
Code、UInt、安全整數與 canonical-json-v1 沿 build-db。所有 path 是安全 repo 相對路徑，
拒絕絕對路徑、`..`、symlink（含父路徑），不保存私人根路徑。

### 2.1 語義版本與 manifest

semantic_bindings 為按 id 排序唯一、非空的 `{id,revision,path,hash}` 陣列。
revision 指 producer 所用的 immutable Git commit，path 指 code-owned 語義 manifest；
hash 是其完整解析值 canonical hash，exact bytes 另釘入 F1。
manifest 固定該版本的完整凍結檔案／exact hashes、固定計算入口與必需環境套件集合，
清冊不能提供命令、任意 import path 或自行縮減集合。
所列版本恰涵蓋 recipes 及實際 identity／reference 解析所需集合，風味身份用到 EN parser 時不可漏列 EN。

manifest 恰為 `{semantic_version_format:1,id,entrypoint,files,environment_packages}`。
entrypoint 是固定 registry 認得的 Code，不是 executable／import path；id 與 binding 相同。
files 為按 path 排序唯一、非空的 `{path,hash}` 陣列，核對完整凍結集合與各檔 exact bytes；
environment_packages 為按套件名稱排序唯一的非空陣列，核對環境紀錄的必需套件集合。
manifest 的完整 canonical hash 與 exact bytes hash 分別保存，不把兩者混用。

版本登錄是目前程式中有限、只增的受支援清單；同 ID 不得改綁新 manifest 或入口。
從 producer Git 唯讀驗 manifest／recipe／凍結檔案 hash，再驗目前安裝的**同版本** exact bytes，
不執行 Git 中的程式、不退回 latest。缺歷史／檔案、未知版本、移除舊版本或改一 byte（包括註解）均拒絕。
已知錯誤或不安全版本列版控拒絕表，記版本 ID、原因代碼與修正引用，進 F1；即使輸出相同亦拒絕正式使用。

凍結邊界至少包括：

| 範圍 | 必含內容 |
| --- | --- |
| 來源 parser | JP／EN 完整欄位投影、語言、URL／卡號、HTML 邊界、presence 與 source face index |
| classification | partition、layout／reminder／token header、NFKC、raw 座標、legacy ID 與常數 |
| parameter | 數值／引用角色、raw 拼寫、trace、hint／pending、解析及規則優先序 |
| flavor exact | 固定 White_Space、四狀態、整欄 code point span、零參數與 exact bytes |
| owner／驗值 | 欄位證明的解讀；可演進身份／政策 reader 的所有 owner／pending／uses 結果仍進摘要 |
| 摘要 | semantic-output-v1 的形狀、排序、canonical／hash 計算 |

使用 code-owned 檔案清單與 import 邊界檢查，不再自動凍結整個 carddb 依賴圖。
凍結 wrapper 不得再呼叫可變的 latest helper。清冊用凍結 parser 入口與目前名稱／身份功能入口分開；
目前功能可修正，新清冊需要新語義時另加版本，不能連帶原地改歷史入口。
非凍結 I/O／模型／編排改動若影響結果，由完整輸出摘要攔住。

既有八個辨識 MATCHER_PATHS 的 bytes／路徑保持不變；若需搬移或改寫，先依
[辨識政策契約](template-parameter-policy.md) 的程序證明行為等價重釘，版本登錄不能代簽辨識授權。
recipes 的 code_path 指真正固定的計算入口，不能要求可演進的總編排 replay.py 等於 producer。

### 2.2 producer 環境與當次差異

environment 保存 producer 的不可變稽核值，必要集合由固定語義 manifest 規定，清冊不能自行省略：
Python 精確版本／implementation、Unicode 版本，以及解析、canonical／讀取結果所用套件的
exact lock 值與實際安裝 artifact 身分（至少 HTML parser；原生元件不能只記顯示版本）。
生成時從 producer lock 推導並核對實際安裝值，完整 uv.lock／pyproject hashes 另保留為 provenance。

environment 恰為 `{python,packages,uv_lock_hash,pyproject_hash}`；python 恰為
`{version,implementation,unicode_version}`，三者為非空字串。packages 按 name 排序唯一，
每項恰為 `{name,version,artifacts}`；name／version 非空，artifacts 是按 path 排序唯一、非空的
`{path,hash}` 陣列，path 為安裝套件內相對定位（不含私人根），hash 驗 producer 實際安裝檔案 bytes。
原生 artifact 保留實際檔名／hash，不能只用 wheel 顯示版本。兩個 lock／project 欄均為完整 Hash。
實際必需套件恰等於所用 manifests 宣告集合的聯集，額外／缺少或空 artifacts 均拒絕，
不能以另一份同名套件或省略原生檔規避紀錄；具體檔案集合由固定版本規格驗證。

消費時另記目前實際環境，environment_differences 逐欄記 producer 值與本次值。
**合法環境值不同只記錄，不先拒絕**：凍結 bytes 與全部輸出相同便通過；輸出不同拒絕並附差異，
差異是診斷線索，不宣稱已證明根因。缺必要欄、非法形狀由封閉模型拒絕，不用 host 值反填 producer。
指定版本不能執行、缺來源或計算失敗仍明示失敗，不能把尚未比較當相等。
完整 host lock 不須等於 producer lock，無關依賴升版本身不能擋舊清冊。
環境／host revision／執行時間／RAM／私人根路徑不進語義 root，也不藉 coverage／source_uses 間接進 root。

### 2.3 inputs 的唯一權威

效果 inputs **恰為 `{kind:effect}`**。source_batch、legacy_file_hash、references（glossary、
vocabulary proposal、basis）及 recognition_policy 只從 recipe config 讀取；在 context 重複給即未知欄拒絕。
來源 bytes 由具名供應器按 hash 提供，不能傳入 normalized／hint 或已解聲明代替重算。
新候選 normalizer config 的整個 runtime dependencies／Python 重複 metadata 移到 bindings／environment；
效果業務輸入 config 及五欄辨識 pin 保留，不修改已採納 config。

風味 inputs 恰為 `{kind:flavor,source_batch,identity_basis,identity_batches}`，重用現有 Batch／IdentityBasis 模型。
每份清冊以自身的批次和不可變背景重算，不從 caller 的單一最新 basis 覆蓋歷史。
basis=null 只可預覽並保持 pending，不能正式採納；歷史背景合法性與當次 owner 用途適用性分開驗。
flavor recipe config 仍恰 `{}`，來源／背景不塞進它，也不由目前 main 補出。

## 3. 不可變 expected_outputs

expected_outputs 保存 `semantic-output-v1` 的完整 **hash-only** manifest：以下六流各有
`{count,hash}`，再對完整 manifest 的 canonical 值（不含 root 自己）算 root。
每流 count 為非負安全整數，hash／root 均為完整 Hash；缺流、額外流、非法 count、錯 root 拒絕。
完整固定序列化與排序規則須由版本化摘要程式實作並接受決定性驗收，不能由 caller 選欄位。

封套恰為 `{format:1,recipe:semantic-output-v1,streams,root}`，streams 恰含下表六個鍵。
root=`H(canonical({format:1,recipe:semantic-output-v1,streams}))`；format 不收 bool／字串。
每流 hash 對該流完整紀錄陣列的 canonical bytes 計算，count 是陣列長度；沒有資料仍驗 canonical 空陣列，
不能以 null／缺鍵代替。不適用 checkpoint 仍用明示不適用及完整統計的紀錄，不省掉整流。

| 輸出流 | 完整內容 |
| --- | --- |
| entries | 全批八欄 entry，含 source_ref、normalizer、legacy；未被採納定義使用者也驗 |
| fields | 每來源版本／locator 的 unknown／empty／whitespace-only／present 與原欄 exact UTF-8 hash；缺欄仍留紀錄 |
| members | entry ID、normalized UTF-8 hash、span、全部 hint／slot occurrence、型別、界值、reference 身分、raw 拼寫 hash、角色、所有 pending 原因，及 flavor owner 或 null |
| coverage | 全計數、scope、unknown／history gaps、完整性、欄位和 owner 覆蓋；不能只驗 complete |
| checkpoint | legacy fingerprints／members、完整數值位置身分及比對；flavor 明示不適用與完整段落統計 |
| source_uses | 原來源與 identity／glossary／policy 的所有實際 uses、parser 語義版本、定位與 archive pins；同 raw 不同用途不得合併消失 |

有意義的順序保留，只有明定為集合者排序；不為決定性正規化原文。
先計六流，再算 manifest root，再封裝清冊／群組鍵；六流不含 enclosing inventory hash、group key 或 expected 自己。
host runtime_dependencies／runtime_configuration 移到獨立 provenance，F1 完整保存，不能靜默丟掉。
官方原欄、normalized、trace 不入清冊，私人記憶體／受保護暫存中重建後計 hash。

首次 producer 固定輸入／版本完整生成，獨立載入重算後審核摘要。
後續 expected 必須重新從 immutable 清冊讀取，loader 不更新它、不以 actual 自填 expected，
記憶體中同時覆寫兩者也不能通過。輸出不符回 `replay_output_mismatch`，列群組、流、
expected／actual hash 與 environment_differences，不印官方原文。

摘要不含稍後採納的定義列表；每個已採納定義另重建六欄 payload、逐筆驗 content_hash、
verify_schema、全家族角色一致、raw roundtrip、ID／完整 bytes 碰撞，不能用 root 代替。
未解角色保持未解；清冊重算成功不等於來源完整、定義或譯本已採納，這些分開計數。

## 4. 群組、全歷史與單次 F1

群組鍵為 `H(canonical({recipes,replay_context}))`。同組分片只重算一次，所有分片 declared entries
集合恰等於重算全集；batch／basis／expected 不同必須各算，不作未證明的跨組去重。
跨組 entry ID 的既有全歷史唯一性不放寬；同位置要並存新分段版本若撞 ID，另審版本鍵。
全部 immutable 清冊／分片及 Git 歷史都驗，改舊檔後 revert 也不能用 latest 遮過。

只用一個 BuildContext／F1。program_revision 是實際執行 H；recipe.code_revision 是歷史 producer R。
dependencies 記 H 的實際完整程式／lock、所用凍結版本與歷史 producer pins；configuration 逐組記
producer、context、目前環境、差異、預期／實際摘要與 uses 歸屬。不把 R 冒稱正在執行的程式。
相同 raw version 仍一列 source_record、parser_version=null；同 parser 版本跨 producer 的用途／pin
歸屬以群組映射保存，不能 dict 最後一筆覆蓋。report/bundle 的四檔與完整獨立 expected 驗證不變。
caller 依 sealed inventory、清冊、references 與 identity 閉包獨立列 expected uses，
再核對實際 uses／DB／archive pin；自算 output root 不能取代獨立集合。

session cache 不跨建置信任成功報告。下一次必重驗 raw／來源／immutable 基準；缺 raw 仍失敗。
現版只作同次群組去重，不做持久成功快取、歷史 Git 執行、歷史 venv、IPC 或父子 F1，
不宣稱離線／乾淨環境／唯讀輸入提供 OS 層隔離。

## 5. 漂移與資源預算

新語義加版本並保留舊入口，舊清冊不改指新版本、不更新 expected golden。
Python／Unicode／函式庫升級先重算全歷史，輸出相同即通過並記差異。
只有排除壞來源／錯 pin 後，原指定版本真的輸出漂移，且可行依賴配置、並存凍結版本／有限相容實作
仍無法讓所有舊清冊按原版本與原預期通過時，停止相關正式建置並另提歷史 executor 方案 A。
環境改變本身不觸發 A；本版不自動啟動它，也不重簽採納。

完整閉包先單行程、順序群組。工程估算以效果 E、風味 F **群組數**計，分片數不是重算數：
`T_plan = 60 + 1.5 × (160 × E + 382 × F)` 秒。
160／382 秒為先前量測的規劃基線，不是本版實測或保證；E=F=1／2／3 約 14.6／28.1／41.7 分。
有實測後以 `T_common + 1.5 × Σt_i` 更新估算；新同類按頁數、raw bytes、entry、身份觀測數比的最大值放大。
超基線 2 倍或新解析形狀先實測，不將線性外推當驗收。

峰值含 base、全域 entry／definition／uses 索引、所有保留的 raw／normalized／hint bytes，
再加最大單組 working set；不能只寫 max(單組 RSS)。逐組測 RSS、tracemalloc retained／暫存、
完成後保留量，原生 parser 以 RSS 為準。以每 entry／來源 byte 的實測成本外推，加 30% 餘裕。
必要私人暫存檔 600／目錄 700，不進 Actions cache／artifact，碰撞完整 bytes 核對不可省略。

版本化預算與 watchdog 驗收：參考環境至少 4 可用 CPU、8 GiB 可用 RAM，記硬體、限制、磁碟與負載。
E=F=1 完整閉包跑三次 fresh process、清應用 session cache（不要求清 OS cache）：
median wall ≤20 分、每次 ≤30 分；peak RSS 目標 ≤4 GiB、每次硬限 6 GiB。
預估或實跑硬限超出回 `replay_budget_exceeded`、清 staging、不回半套成功，不得在同次失敗後自動加預算。
小預算合成測 watchdog，CI 不故意 OOM；缺參考資源／真實來源明示性能未驗收，不以合成替代全批。

預估超 20 分／4 GiB，或連兩次超目標，須在新增會增加負擔的群組前另開優化／經驗證持久快取設計；
預估／實跑觸及硬限即暫停增加群組，不略過歷史。先消除同組重複解析、釋放多餘暫存再量。
未來快取 key 必含語義 exact hashes、必要環境、完整 inputs／basis／recipes、validator／輸出版本，
每次仍驗 raw／閉包／immutable expected、全域採納／當次 owner／F1；壞快取丟棄重算或明示失敗。
本版無此快取能力，也不授權只讀成功紀錄代替計算。

## 6. 最小獨立驗收

以下為實作要求，不是已跑測試。多條件拆成單因素反例，從完整成功基例修改，
拒絕訊息錨定單一完整訊息；hash-only 報告不含官方原文。

| 編號 | 輸入／變更 | 預期 |
| --- | --- | --- |
| H01 | E@R1、F@R2，同語義版；H@R3 改無關程式 | 各歷史閉包及 root 保持，不重錄採納 |
| H02 | 增刪非凍結模組、無關 lock 升版、必要 parser 套件升版，各一次 | 凍結 bytes／全輸出相同通過，差異記 F1；輸出漂移拒絕 |
| H03 | 凍結檔一 byte／註解、同 ID 換 manifest，各一次；另加合法新版本 | 前者拒絕，後者保留舊版可共存 |
| H04 | 缺 producer Git、淺歷史、symlink、錯 code/config hash、漏 binding，各一次 | 拒絕，不執行 Git 或退 latest |
| H05 | 環境不同，①全輸出相同；②一流不同 | ①通過記兩端差異；②output mismatch 附差異，不先以環境拒絕 |
| H06 | 同 recipes、兩合法 flavor batch／basis | 各用自身 context，不借另一組 owner／pending |
| H07 | 舊背景合法，新增無關身份／改 current observation | 歷史結果不變，當次 owner 另驗適用性 |
| H08 | owner 不唯一／未 confirmed／父卡錯／觀測或文字錯／非純 JP／漏身份批次 | 各自保持原 pending 或精確拒絕 |
| H09 | 漏／多 entry、跨組重複 ID、改無現行定義引用的舊清冊 | 全歷史拒絕；只重排分片集合相同可通過 |
| H10 | 改舊檔後 revert、換 producer／expected、改 index，各一次 | immutable 拒絕，不能更新 golden |
| H11 | format=true／未知版／缺 context／非法環境／effect 重複輸入／flavor 非空 config | 封閉模型分別拒絕，不補值 |
| H12 | 兩版輸出不同；舊清冊錯分派新版 | 舊版／原摘要拒絕錯分派，正常明列兩版可共存 |
| H13 | 文字 hash 不變，只改 role／pending／reference ID／raw 拼寫／span | members／checkpoint 與逐欄核對拒絕 |
| H14 | flavor CRLF→LF、trim、unknown→empty、漏 whitespace-only、換 owner | exact／四狀態與摘要拒絕 |
| H15 | 未列 helper 行為變，環境宣告相同 | 任一輸出變拒絕；全輸出保留可通過，不宣稱全域等價 |
| H16 | 缺流／錯 count／root；記憶體同改 expected 和 actual | 前者格式／hash 拒絕；後者重讀 immutable expected 拒絕 |
| H17 | payload hash、完整 hash 異 bytes、slot、review hash／採納樣本錯 | 原有逐筆／碰撞／採納仍拒絕 |
| H18 | F1 假稱執行 R、漏組 parser 歸屬／用途、錯 archive pin | F1／independent expected／DB 拒絕，不造假 raw ID |
| H19 | session cache 熱後下次缺 raw／改來源，仍有成功報告 | 新建置失敗；完整 fresh process 基例通過 |
| H20 | glossary foreign v2、循環引用、未知語義版 | 正常結構驗證不遞迴；循環／未知版拒絕 |
| H21 | 同完整群組 fresh process 重算兩次，改 hash seed／locale／時間／根／列舉序 | 六流 canonical bytes、root、payload 逐 byte 相同；F1 時間另列 |
| H22 | 合成突變讓 set/dict 亂序、hash seed 或時間進輸出 | 決定性測試擋下，不在測試端忽略差異欄 |
| H23 | 拒絕表版本的 hash 全等；合法新版基例 | 前者工程拒絕，後者完整驗，不遷移舊清冊 |
| H24 | 同組切 1／10／100 分片，完整 entries 相同 | 每次只計算 1 次，結果相同 |
| H25 | 完整 E=F=1 三次 fresh process | wall／RSS 符合 §5；缺來源明示未量 |
| H26 | 合成 1／2／4／8 合法不同組、不重複 entry，記 retained bytes | 所有組均計算；單組時間比單組基線惡化 >25% 須解釋 |
| H27 | 小預算超 wall／RSS，預估觸優化門檻 | budget exceeded、無半套成功，觸發紀錄且不加預算／跳歷史 |
| H28 | 未解輸出漂移，試改舊 expected／版本 | 拒絕並進 A 設計門檻，不自動啟動 executor |

真實全批性能由有封存資料的可信任環境驗；fork 跑合成格式／小型性能守門，明示沒跑真實全批。
required 缺來源必失敗。性能報告記 CPU／wall、RSS、輸入量、組數、解析次數、快取命中、root、F1 hash，
未量與實測分開，不以來源指紋重現代替完整覆蓋或採納。
