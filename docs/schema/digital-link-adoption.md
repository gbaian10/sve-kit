# 數位對應採納與名稱證據入口

本文件細化 [build-db §8](build-db.md#8-數位對應與語音) 的 `digital_link`／`digital_link_coverage` 人工入口，及 [翻譯契約 §5／§6](translation-contract.md#5-概念選詞與數位證據) 的名稱使用條件。入口與離線候選工具的實作邊界見 §8；**尚未遷入正式採納資料**，不代表維護者已核可任何新關係。永久身分、數位建置表及公開欄位沿既有契約，不增設平行卡名詞典。本文名稱與驗收情境皆為合成資料。

## 1. 範圍與既有接點

只收已採納的 SVE↔數位關係與查核覆蓋，不收數位卡文、圖片、語音或未決候選。`same_card` 指同一改編卡片概念，不要求兩遊戲效果一致；`effect_similarity` 與 relation 正交。`same_character`／`name_only` 可作瀏覽對應，但不能直接套官方數位卡名。owner 另有符合獨立名字政策的有效依據時，可依該政策取名，不以這些關係供名。相同日文、角色、畫師、圖或信心分數均不是同概念採納。

另有獨立的 [名字／同名瀏覽政策](digital-name-policy.md)：卡名只採用合格官方字串；同名連結另產卡層級 same_name，依規則先視為同卡但未逐筆確認，只作瀏覽，不作同概念、效果、圖或語音證據。新能力未實作前不能寫入真人入口。

現有 `translations/digital.py` 的 `import_digital()` 可重建指定數位卡、父卡與名稱面／語言的最小閉包。`snapshot/offline_names.py` 的 composer 將 link 結果交給 `digital_name_policies.application`，由 `current_application` 產生 current 名稱與公開 bindings；真人同卡候選先經 `DigitalLinkResult.eligible_owner()` 逐 owner 驗證，再沿 `translations/counterparts.py` 的 sv1→svwb 順位選取。`translations/current_names.py` 以 current 概念與語義指派解析 owner，`translations/importer.py` 驗 glossary 的 `digital_name` 證據，不建立 link。舊名稱入口已移除；#53 的後續 owner-aware 工作須接上述 current 路徑，不能恢復歷史名稱重播。逐 owner 的採納結果界線見 §8。

registry 固定永久身分；`curation/` 尚無格式／loader。glossary 現行入口只收 current format 2，不能混入本入口的 kind。本契約採獨立的 digital-links format 2 入口：每筆就是一個 subject 的現行關係，沿既有 YAML、canonical hash、SourceRef、凍結 parser 與 F1；沒有批次決定、採納序號或前件鏈，修改歷史由 Git 保存。

## 2. 入口與分片

路徑相對 `authored/`，完整頂層欄位如下；所有欄位必填，未知欄位拒絕。

| 路徑 | 完整頂層欄位 |
| --- | --- |
| `digital-links/index.yaml` | `digital_link_authored_format: 2, kind: digital_link_index, includes` |
| `digital-links/<area>/<filing_key>/<sequence>.yaml` | `digital_link_authored_format: 2, kind: digital_link_shard, records` |

area 目前只有 `links`，`coverage` 路徑明確拒絕。filing_key 為 `[A-Za-z0-9_-]+`，只歸檔，不決定身分、商品或地區；可沿 card 的歸檔代號。sequence 從 001 起按 area/filing_key 連續，至少三位十進位。每片非空。

沿 [authored-layout §1／§2](authored-layout.md#2-分片批次決定與來源) 的嚴格 YAML 1.2、canonical recipe、單檔 <1 MiB／512 KiB 目標。includes 映射完整分片路徑到**解析後 canonical 內容 hash**；完整 index／分片原始 bytes 另釘 F1。拒絕缺檔、hash 不符、重複 key、symlink、絕對路徑、`..`、跨入口引用及未索引檔案。先驗全入口，再按公開範圍投影；不能先濾 JP。啟用此能力時缺 index 必須失敗，空集合只能明示 includes={}；未支援的新格式不能當空集合。

record 恰為 `{subject,value,review_level,reason}`：

| 欄位 | 規則 |
| --- | --- |
| subject | §3 的完整選擇鍵；全入口每個 subject 只能有一筆 |
| value | §3 的完整關係值，不是局部 patch |
| review_level | `sampled`（維護者抽查過的批次成員）或 `confirmed`（逐筆確認）；不接受 proposed 或 model_reviewed |
| reason | 非空理由，說明實際關係與查核方式，不含官方原文 |

修正 relation、名稱依據或 review_level 直接改該筆並經 PR 審核；撤下關係就刪除該筆，舊內容由 Git 歷史保存。換 card、面或數位目標是另一個 subject。不另記採納序號、前件、核對背景 hash、批次成員或抽樣名單。

### 2.1 建置 DB 的決定列

build DB 的 `digital_link.decision_id` 必填，所以匯入器為每筆 materialized link 產生一列 record 範圍的 decision：`id="d:"+H(["digital-link-decision-v1",完整 record])` 的 64 hex、`state=review_level`、`scope=record`、`category=digital_link`，membership_hash／policy_id／sample_ids 為 null，note 為 reason；decision_source 連到該 record 所在分片的 authored source_record。這是由 authored 資料推導的投影，不是另一份收據。H 為 authored-layout 的完整 SHA-256 canonical JSON recipe，Hash 帶 `sha256:`。

真人 same_card 的實際核對要求不變，工具不能把規格通過或模型結果寫成 sampled／confirmed。
新增 same_name 的規則另依[數位名字政策](digital-name-policy.md#2-同名規則瀏覽連結)，不放寬本入口的同卡核對。

## 3. links 記錄

subject 恰為 `{card_id,face_id,game,official_id,digital_phase}`。card_id／face_id 沿永久 registry；game=sv1/svwb，official_id 沿來源的 9／8 位 ASCII 數字字串，不能猜去後綴、丟前導零或由草稿名稱配卡。face_id 與 digital_phase **同時有值或同時 null**；前者驗 face.card_id，後者須是該數位卡實際存在的 phase。

此入口第一版只支援「精確面對應」與「兩端皆未定位面的 card-level 對應」。DB nullable 欄位較寬不表示入口接受單邊面對應。card-level 可以瀏覽，但永遠不供官方名稱。數位 phase 與 SVE front/back／ordinal 分別核對，不能以相同順位、normal=正面、evolved=背面推導。契約的 phase 枚舉沿 build-db；runtime 未支援的額外 phase 必須拒絕，不能丟棄後假裝閉包完整。

value 恰為 `{relation,effect_similarity,sve_names,digital_names}`：

| 欄位 | 定義 |
| --- | --- |
| relation | same_card／same_character／name_only，按實際人工核對記錄 |
| effect_similarity | near_identical／core_kept／reworked 或 null；未知保持 null，不硬轉草稿 close/partial/redesigned |
| sve_names | 非空、排序唯一的 `{printing_id,face_id,name_ref}` 陣列，指核對時 SVE 的完整名稱字串 |
| digital_names | 非空、排序唯一的 `{phase,lang,name_ref}` 陣列，指核對時數位卡的完整名稱字串 |

name_ref 就是 SourceRef，locator 定位完整 name 欄，text_hash 驗 exact UTF-8，不 trim／NFKC／以搜尋名替代。sve_names 按 `(printing_id,face_id,canonical(name_ref))` 排序；digital_names 按 `(phase,lang)` 排序，同 phase/lang 不得重複。

每筆 SVE 證據必須從凍結 JP 卡頁重播：registry 的 source_face_map 明示來源面對應，printing_id 所屬 card 須等於 subject.card_id；精確面 link 的每筆 face_id 均須等於 subject.face_id。不能只驗名字相同、FK 存在或 card_id。card-level 的證據可包含該 card 的不同面，但不得冒充任一精確面 link。

每筆數位證據須定位同 game／official_id 的實際 API 項及明示 phase，provider／語言／phase／完整字串均驗回。精確面 link 只收 subject.digital_phase；card-level 可列實際核對的不同 phase。每個列出的 phase 必有 ja，並完整列入其引用批次在該 phase 所有非空的 ja/en/zh-Hant 名稱；缺語言維持缺少，不能填 null 名稱或借另一面。目前最小匯入器支援的名稱語言以這三種為限，不能把未知 layout／parser 靜默降格。

同卡判斷不由兩端名稱相等推出，也不要求 SVE 與數位日文名稱一定相等；人工決定須明示為何是同一改編概念。兩端各自 exact 名稱是可重驗的依據，不能以同角色或圖片相似取代。

以上維持真人 same_card；新增政策 same_name 是獨立規則路徑，只產 card/game/ID、face_id 與 digital_phase 均 null，不推導面機制。人審精確面是另一 subject／ID，真正人審優先、同組規則重複列抑制。

匯入時以 `(game,official_id)`／phase 解析 digital_card／digital_face，不另配一份數位永久 ID。`digital_link.id="dl:"+H(["digital-link-v1",subject])` 的 64 hex；此為既有 ID 欄的內容定址配方，不另建 registry。relation／證據／review_level 修訂不換 id；換 card、面或數位目標是另一 subject。每個 subject 只有一筆，materialize 時遵守 DB 同卡同對象同面唯一性。

## 4. coverage 記錄（未實作）

subject 恰為 `{card_id,game}`；value 恰為 `{state,as_of,sve_names,catalogues,links}`。as_of 為實際查核範圍截止 Date，不由建置日、latest cache 或沒有 link 推出；不得晚於真實審核日期。state 沿既有 unreviewed/partial/reviewed_none/reviewed_matches，不是關係的 review_level。

coverage 的 sve_names 恰為排序唯一的 `{face_id,name_ref}` 陣列，保存各永久面出現過的相異完整名稱，不含 printing_id；link 的 sve_names 仍沿 §3 的版次／面證據形狀。每個 `(face_id,name_ref.text_hash)` 只留一項可重播的代表來源，按此鍵排序；不同面同名不合併。由核對時 registry/source_face_map 驗來源面的 card／face 歸屬，不能以去掉 printing_id 免掉來源驗證。catalogues 為排序唯一的**完整凍結目錄引用**，每項恰為：

```text
{batch_id,source_version_id,parser,inventory_hash}
```

前兩欄沿來源歸檔版本引用，store 由執行設定解析，parser 沿釘住的數位 API recipe；引用的是完整 JSON 投影，不是任意 locator 或可執行查詢。inventory_hash=H(從該完整投影得到的排序唯一 `{official_id,phase,name_hash}` 陣列)，依 `(official_id,phase)` 排序，ID 原樣保留，name_hash 為該 phase 完整 JA 名稱的 exact UTF-8 hash，缺名稱則明示 null。只能用同 game 的 ja 目錄；重複 official_id／同卡同 phase 或不支援的面結構須拒絕，不能去重吞掉來源衝突。這是新的結構性引用型別，重用歸檔與 parser 驗證，不把 array/object hash 冒稱 SourceRef.text_hash；來源語言名稱仍用普通 SourceRef。

完整目錄也只表示**此次凍結來源的範圍**，不是未來或所有外部來源的全球無對應證明。不能從只匯入已對應卡的 digital_card 表、名稱搜尋命中子集或草稿已選候選算目錄 hash。catalogues 及 F1 保存實際搜索的全部目錄；不能把一個局部清單說成涵蓋未檢查的來源。catalogues 按 canonical 引用排序；空目錄本身不能證明無對應。

links 為排序唯一的 link id（`dl:`…）陣列，恰指本 card/game 本次範圍內**全部 fresh** link。不包含 stale 或已刪除的關係，不借單純 confirmed 身分／glossary 決定。same_character／name_only 也算找到的關係，coverage 不只計可提供名稱的 same_card。

| state | 必須成立的查核事實 |
| --- | --- |
| unreviewed | 明示尚未查核；sve_names/catalogues/links 皆 []，也無已有效採納的 link，不宣稱無對應 |
| partial | 已做部分查核但不足以封完整範圍；sve_names/catalogues/links 至少一者非空，允許尚未找到有效 link |
| reviewed_none | 實際完整查核；sve_names/catalogues 非空且通過完整範圍重驗，links=[]，該範圍沒有任何有效 relation |
| reviewed_matches | 同樣完成完整查核；links 非空並恰等於全部有效 relation，不表示每張有官方譯名 |

完整 SVE 範圍含該 card 的所有永久面，在釘住的凍結輸入中各面出現過的相異 JP 完整名稱；wording 未定／勘誤及歷史名稱觀測仍納入，不能只看 current 或只查正面。工具以實際來源與 registry 面對應重算排序去重的 `(face_id,exact name hash)` 集合，與 sve_names 重播的集合完整比較；不以版次為範圍或計數單位。同卡同面同名再錄、另一 raw 版本或換代表來源不增加集合，無須重簽；但每次仍驗新來源與面歸屬。缺某面 JP 名稱、漏相異名字或尚有未決候選，都不能宣稱完成。printed 名稱 unknown 不捏造成 current 名稱；未知印刷資料也不被這份查核宣布為已知。完整查核只對已知凍結名稱集合成立，不聲稱未知資料查完。

完整數位範圍要能重播所有 catalogues，檢查每個 SVE 面／名稱。目錄中某 phase 的 JA 名稱 unknown 時，本版名稱查核不能宣稱完整，維持 partial；不能略去該項縮小目錄。批次 sampled 不能把沒查過的 card 升成 reviewed_none；模型 confidence／漏列候選也不能證明完整。無 coverage 紀錄或對本次範圍 stale 時，公開只呈現缺少覆蓋／unknown，不能合成帶真人 decision 的 reviewed_none。

SVE 新面／相異名稱 hash 或數位目錄成員／名稱 hash 變動、link 修改／新增／刪除使原 links 精確集合不再相符時，舊完整 coverage 不適用本次範圍；列 stale 原因，不自動改寫成 partial／none。名稱集合、目錄相關內容及 link 集合都未變時，新增同名再錄版次、raw 頁其他欄或建置程式版本改變不要求重新簽 coverage；本次來源仍完整驗回並納入 F1。重新查核後更新 coverage 記錄，舊內容由 Git 保存。as_of 不保證其後新卡仍查完；sv1 仍 frozen，svwb 新來源只用另行封存的明示輸入，不在匯入時連外。

**首批只遷 link，不採納 coverage**，入口不產生 coverage 分片，不為每個 card/game 填 unreviewed 決定。沒有紀錄即未知；官方名稱的選取依 §6 的 fresh link，不以 coverage 是否存在／完成為前提。公開 required 集合仍輸出 `digital_link_coverage=[]`。發布按 [build-db §16／§18](build-db.md#16-發布閘門與投影邊界) 與 [snapshot-format §7](snapshot-format.md#7-發布閘門與變動報告) 驗已啟用能力、已有列及非空引用閉包，不要求每個 card/game 都有 coverage 紀錄。日後若要採納覆蓋，仍沿本節真人門檻；不先放寬 reviewed_none 成純機械採納。

## 5. 匯入順序、來源與原子性

本節規定 composer；§8 已實作 link 匯入 API，coverage 與完整發布 CLI 仍未實作。

1. 驗完整入口與 authored commit bytes。缺 raw／批次／recipe／hash、錯 locator／語言／父層或來源歸屬均是輸入錯誤，整次失敗，不把失敗行藏成 unknown。
2. 用既有 registry／TextPlan 的公開身分與名稱觀測解析 SVE 卡／面／版次，包含 pending wording；不因 current 尚未決定丟掉 identity。目標合法退役或相關名稱／目標來源變動屬 freshness 問題，不能轉移到猜出的替代卡。
3. 以目前解析器重驗每筆 link 自己引用的兩端名字與來源版本，再與本次釘住的數位目錄比較：目標的現行名稱不同或已不存在時該 link stale，不 materialize，也不以 latest 補名字。新 owner 名稱不同依 §6 個別判斷，不因同 card 就搬用。
4. 從 fresh link 的明示 targets 用 `import_digital()` 建最小數位閉包：全部必要父卡、實際名稱面／語言及凍結來源；coverage 目錄也不要求公開整個數位庫。
5. materialize fresh link 與其 §2.1 決定列；驗 coverage 的完整來源、範圍與 link 集合，再 materialize 適用的 coverage。引用未知永久 ID 或偽造來源不能當合法退役略過。所有 stale／partial 與待件原因列私人建置報告。
6. 此後 glossary `digital_name` concept_evidence 及名稱產生者才能引用它們；仍重驗精確 face、decision、JP／目標語 SourceRef，不以 FK 存在替代。所有 DB 寫入在 caller-owned transaction 內；任一步失敗 rollback，不能留下半批 link／coverage／翻譯。對外獨立匯入包裝才擁有 transaction。

authored source_record 保存分片 bytes 與 authored commit，decision_source 把決定列連到分片；名稱和 coverage 的目錄 raw、batch descriptor／receipt、parser 程式／設定、來源 usage 全部納入 F1。loader 返回可按 link id 查核的型別化結果，供匯入、glossary 與逐 owner 驗證共享；依據可由釘住的 authored／F1 重播，不新建一張平行名稱表，也不從 translation.id 解碼所有者。後續新 loader／validator／importer 須加進 recipe runtime 依賴閉包。最後由完整建置的 `record.verify` 驗實際 DB source 使用閉包，不能只用各子匯入器的 partial verify 宣稱完成。

官方原文／數位譯名只從凍結 SourceRef 重建，不抄入 authored、測試或報告；目錄只存 ID／phase hash 引用。研究草稿、網站 URL、latest cache、live manifest、網路補抓都不是 runtime 輸入。缺凍結前置先停止遷入，不擅自封存或更新 sv1。公開仍用既有 digital_link／digital_link_coverage 欄位白名單，不公開源文件。

## 6. 每個名稱 owner 的使用閘門

關係採納不是 context 級別的永久名稱授權。current 名稱路徑對每個 `face_revision.name`／`printing_face.name` 驗自己的來源；#53 後續接線也須遵守以下條件，再產生或引用 translation：獨立名字政策先依第1步驗 owner、再驗政策完整目錄資格；下列第2至4步只在第5步允許的真人供名路徑使用，不是合格政策名字必須先有 link 的條件。

1. 取得 owner 自己的實際原文、source_unit／hash 與凍結來源；由 registry/source_face_map 驗同 card／同 face。face_revision 用該 revision 的來源；printing_face 只用該版已知 printed 字串與它的印刷依據，unknown 直接缺譯，不以 current 填補。
2. 找到本次 fresh、sampled/confirmed、relation=same_card 且精確兩面皆有值的 link。它的 card_id／face_id 必與此 owner 相同；digital_face 必屬宣告 digital_card。不同版次可共用同永久 card/face 的 link，但仍各自驗自己的來源，不能用另一張卡同名當身分證據。
3. 重播 link 的 SVE 名稱依據與 owner 自己的凍結名稱。owner 的完整 exact 名稱必等於此同 card/face 的至少一項已採納 sve_names；可由同卡同面再錄的另一來源證明相同名字，不要求原 raw 頁永不更新，但不能只比共用 context／顯示名。印刷名與 current 不同時，印刷名須有自己的已採納名稱依據，不能繼承 current 的結果。
4. 重播該 link 的 digital_names：同 game／official_id／phase 的 ja 及目標語名字須與本次數位 DB 完整字串相符；目標語缺少仍缺少。digital ja 與 SVE ja 不必相等，但這一對 exact 名稱必在同概念人工核對依據內。不得借另一數位面、base 卡或另一語言的名字。
5. 依 [數位名字政策 §1](digital-name-policy.md#1-名稱採用與關係分開) 的完整順位，先處理有效的逐 owner 選詞覆寫，再取合格政策名字；政策因目錄同名異譯／缺譯而不合格時，才取前述自己的真人同卡精確面候選。逐名排除不得借任何真人 link 繞回自動官方名。現行選取由 `digital_name_policies.current_application` 處理；政策路徑只有一代完全無此名才直接用二代，真人路徑由 `counterparts.first_counterpart()` 在沒有合格一代候選時退合格二代，同一遊戲多個相異已採納名字則拒絕。凍結名稱證據、`origin=official`／`authority=digital_official`、品質旗標與 current 穩定 ID 都由 current 路徑產生，不帶舊名稱選取的決定欄位。純譯名字串差異按總順位取勝出名字並報差異，不退回原文；真正語義歧義仍須指派，不按 ID、hash 或 confidence 任取。兩條皆無合法來源才走其他合法選詞或 pending／原文。
6. 只有此 owner 的產生呼叫返回的 translation 才可建立 use／FieldTranslation；不能先有 translation_selection 就跳過前五步。即使其他 owner 共用 source_unit/context 且已有官方譯文，沒有自己有效政策證明或真人 link 的第三張卡仍缺譯。

**現有函式的界線**：`DigitalLinkResult.eligible_owner()` 接受 face_revision 或 printing_face 的 `NameOwner` 與可選的精確 `name_ref`，逐 owner 驗自己的原文與 link 證據。`current_names.prepare()` 驗當次概念／語義指派；`current_application` 選取並產生 current translation/use 與 display bindings。#53 須在這些 current 接點補其餘需求，不另實作一套排序，也不能把全域 DB 暫改後假裝只剩某 owner 的合格候選。link 入口匯入通過不能宣稱 #53 全部需求或所有 owner 已完成供名。

兩張同 JP 原文、不同改編概念／官方譯名，須有有效的 current `context_assignment` 釘每個 owner 的 source_hash／variant／concept_key 與同字異義理由，分開 context。`current_names.prepare()` 驗指派與當次來源，返回的 resolver 在概念不唯一／缺少時分別回報 `ambiguous_name_concept`／`missing_name_concept`；這是 current 診斷，不是舊入口的例外訊息。不能據此按 card ID 自動開 variant 或任選概念；沒有確定概念便不能取該概念的 glossary choice，其他合法政策／真人候選仍各自驗證。`counterparts.first_counterpart()` 對同一遊戲多個相異已採納名字另以「Ambiguous adopted digital names」拒絕。#53 須沿這些 current 診斷與供名閘門接線；第三張沒有自己的同卡 link 且政策目錄不合格時，仍不能借用任一份。

`glossary_choice.concept_evidence.digital_name` 沿既有 `{digital_face_id,sve_owner,jp_ref,target_ref,decision_id}`，其中 sve_owner 是永久 SVE face；jp_ref／target_ref 是數位 JA／目標語名稱，不等於 owner 的 SVE 名稱依據。匯入器須同時驗本契約的 fresh link 及其 SVE 證據；不能因 glossary choice 有官方 origin 就放行另一 owner。card_name 概念的預設指派另依翻譯契約／後續 intake，不在這裡靠字串生成 term ID。

單一 current 官方名稱使用 `origin=official`／`authority=digital_official`，遊戲與凍結來源留在其依據；效果的專案翻譯仍為 `authority=unofficial`，取了官方卡名不升整段效果權威。缺官方候選可走另外有效的 current 專案選詞或明示 pending／原文，不自動採納草稿譯名。printed 的缺譯也不回用 current 譯文。

## 7. 草稿遷入前置與反例驗收

研究資料只提供候選與先前比對線索。正式遷入須核對每個 SVE 永久 ID／面、數位 official_id／phase、凍結兩端名字與實際查核方式；缺來源、面定位或維護者實際查核就列待件。`needs_decision=false` 不自動寫成 confirmed 或 sampled；自譯卡名的 glossary 委託不授權數位同卡關係。首批只遷 link，coverage 依 §4 留未知。風味文字、單卡自由覆寫與其他 intake 不在本契約範圍。

### 7.1 連結分兩層採納

分層只決定抽樣批次／逐筆確認的範圍，**不改真人 sampled／confirmed 門檻**，也不將同名機械結果當已確認同卡。先通過 §2／§3 的入口、身分、面、來源與閉包硬檢查；工具以釘住的凍結來源重算下列條件，不直接採信草稿文字或 confidence。

第一層須原候選為 same_card，且每個納入此候選的 SVE／數位面全部通過：

1. 兩端完整 JP 名稱 exact UTF-8 相等，不 trim／NFKC。
2. 職業相同，以本節經維護者確認的明示 enum 表，將數位職業映射到 SVE class code 後比較；表內沒有、明示無對應或不能驗回來源代碼就不通過。不依名稱相似或工具自行推導新增對照。
3. 基本卡種相同（follower／spell／amulet）；進化／token 標記另驗，不能用卡種推面或 phase。未知代碼／其他卡種不通過。
4. 該 JP 名稱在**同遊戲完整凍結目錄**僅對到一個非空 zh-Hant 名稱。查全部 ID／面，包含其他候選之外的同名卡；有第二個譯名或未取得目標語的同名項，都不通過。目錄須驗全部頁及來源閉包，不能只取草稿 sources 或已對應卡的子集。
5. 原比對未標待確認；232 筆先前待決不得因重算條件通過而自動移入第一層。

職業對照表如下；數位代碼是凍結 API 的 sv1 `clan`／svwb `class` 值，非 SVE 永久 ID。SVE code 沿[正式 catalog 契約](catalog-inputs.md#2-永久代碼)，本表只判分層條件，不代替 catalog 的有效採納／來源重驗。`無對應` 必須判失敗，不能以兩端 null 當相同。

| 遊戲 | 數位代碼 | 數位職業識別 | 對應 SVE class code |
| --- | --- | --- | --- |
| sv1 | 0 | neutral | neutral |
| sv1 | 1 | elf | elf |
| sv1 | 2 | royal | royal |
| sv1 | 3 | witch | witch |
| sv1 | 4 | dragon | dragon |
| sv1 | 5 | necromancer | nightmare |
| sv1 | 6 | vampire | nightmare |
| sv1 | 7 | bishop | bishop |
| sv1 | 8 | nemesis | 無對應 |
| svwb | 0 | neutral | neutral |
| svwb | 1 | elf | elf |
| svwb | 2 | royal | royal |
| svwb | 3 | witch | witch |
| svwb | 4 | dragon | dragon |
| svwb | 5 | nightmare | nightmare |
| svwb | 6 | bishop | bishop |
| svwb | 7 | nemesis | 無對應 |

維護者於 2026-10-02T17:33:18+08:00 經協調者對話確認：比對數位對應時，sv1 的 necromancer／vampire 各自對到 SVE nightmare。這是多對一的比較規則，不把兩個數位職業互相視為相同，也**不代表任何一筆 link 已採納**；其餘四個條件及實際真人 sampled／confirmed 門檻照常。新增或變更本表任何對照須維護者確認並進版控（`digital_links.candidates.CLASSES`），不由工具沿新枚舉猜對照。

第一層先機械全查並出候選報告；正式建置只讀 authored record 與 frozen 證據，不把研究草稿變成 runtime 來源。維護者親自看非空樣本、確認該批同卡關係後，整批以 `review_level: sampled` 寫入；樣本數由他決定，工具不得預填 30／50 或假稱看滿固定數量，也不由模型代看。工具可按穩定排序提供抽樣候選；實際抽樣前只留候選，不能先寫 sampled。抽查名單與事件不進 authored，PR 審核與 Git 歷史就是修改紀錄。

第二層為任一條件不成立或無法驗證者，包含 232 筆待決、same_character、mixed、name_only／僅同名、兩端名字不同、職業／卡種不符或同名異譯。逐筆交維護者確認後以 `review_level: confirmed` 寫入，或先擱置；未決的不進正式入口。擱置只表示暫無此來源的官方譯名，可顯示另行已採納的自譯名或原文，不影響其他卡的抽樣採納與發布。relation 仍依實際判斷，same_character／name_only 即使已確認也不能供官方名；mixed 必須拆成明確的各遊戲／目標判斷，不能寫入 DB enum 或整列升 same_card。擱置與該關係不阻止 owner 另符合獨立名字政策取名，該政策的資格須獨立驗回。

reason 如實寫「名稱／職業／卡種機械相符、同遊戲完整目錄繁中名唯一、原候選未標待決；隨批次真人抽樣採納」，不說「已逐筆人工確認」；公開的 sampled／confirmed 由 review_level 決定。抽樣不免除逐 owner 的 §6 重驗。分層筆數只報數字，不附官方名字／卡文；另註明計數單位及重疊項，不拿草稿行數冒充已採納 link 數。

### 7.2 最小反例

下面是**後續實作必備的合成反例，不是已跑測試結果**。I 為本入口匯入驗收；N 為名稱/use 接線驗收，含 #53 尚未實作部分；兩者須分別報告，不用 I 通過宣稱 N 完成。

| 編號／層 | 最小反例 | 必須得到的結果 |
| --- | --- | --- |
| D01／N | 卡 A／B 原文同為自撰「範例旅人」，各有已採納同面 link，數位譯名分別為「範例甲」「範例乙」 | 無有效 context_assignment 時拒絕歧義；具各 owner 的有效同字異義指派後分別產生／綁自己的名字，不任選或覆蓋共用 selection |
| D02／N | 第三張 C 與 A/B 共用原文／source_unit，但 C 無 link；context／官方 translation 已存在 | C 有自己合法政策名字證明即可用；政策目錄異譯／缺譯且無自己真人 B 時不得借前兩張官名；未採納 owner 反例仍保留 |
| D03／I、N | 雙面 SVE 僅正面有 link，背面原文碰巧同名；或把背面指定為另一個明示 phase | 無背面證據不能借正面；有核對的背面／phase 才取該面的名，禁止 ordinal 推導 |
| D04／I、N | 已採納 svwb same_card，但該 phase 缺 zh-Hant；sv1 可有／可沒有目標語 | 不捏造語言；取詞改一代優先；sv1 合格先用，無合格一代真人候選才用合格二代；皆無則缺譯／回原文 |
| D05／I、N | 只有 same_character 或 name_only，官方名、角色、圖與原文都相似 | 關係可瀏覽，不能靠該關係取 digital_official；自己有效名字政策仍可供名，不能升效果權威 |
| D06／I、N | link 已刪除；或新釘住數位來源的名稱／phase 已改而舊 relation 未重審 | 不恢復較早真人 link、不借舊 context；前者不 materialize、後者 stale。自己名字政策仍依指定目錄驗資格，規則連結不靜默復活真人待件，舊內容由 Git 保存 |
| D07／N | 同 card/face 的 current 名稱甲已有譯名，printing_face 印刷名稱乙／unknown | 乙須有自己的完整印刷來源與已採納名稱依據；缺任一條件不得套甲，unknown 也不得造甲原文 |
| D08／I、N | link 指 A 的 face，但 SVE frozen 名稱證據／名稱 owner 實為 B；或 digital 名稱屬另一 official_id／phase／語言 | 來源歸屬／父層錯誤在匯入或使用時拒絕，不只檢查字串／FK |
| D09／I | 沒有 link／只有待決候選，卻填 reviewed_none；或 coverage 只查正面、局部數位清單 | 無實際完整範圍／事件拒絕；有效 partial 或缺覆蓋維持 unknown，不偽裝已查無 |
| D10／I | coverage links 漏列一個 same_character、指已刪除的 link 或多列一個他卡 link | 完整集合／精確 link id 驗證失敗；不得因名稱不可用就不計該關係 |
| D11／I | 新增同卡同面同名再錄；另對照新增相異名稱／背面／目錄成員 | 同名再錄不使 coverage stale、仍重驗來源；相異名稱／新面／目錄變更依 §4 標 stale，不改 as_of 或自動造 partial 決定 |
| D12／I | 同 subject 兩筆、改分片未更新 index hash、未索引分片 | 全入口拒絕；失敗交易不留半批 DB 寫入 |
| D13／I | 只改 frozen 名稱一個字但維持長度、錯 parser／batch，或刪掉某來源 usage | exact 名稱／來源 pin／完整 F1 使用閉包必須抓到，不以 byte 長度或子匯入器 verify 取代 |
| D14／I | needs_decision=true、confidence=high 或 model_reviewed 被工具直接寫成採納 | loader 只收 sampled／confirmed，工具不寫 authored；待決候選不取得官方名 |
| D15／I | 草稿所選同名卡只有一個繁中名，但完整目錄另有同名不同譯名／缺繁中項 | 第一層不通過，留第二層逐筆確認／擱置，不以子集的唯一結果分批採納 |
| D16／I | 五個機械條件通過但未實際抽樣 | 不寫 sampled；reason 不假稱逐筆確認 |
| D17／I、N | fresh 已採納 link 與名稱證據完整，但完全沒有 coverage 紀錄 | 官方名不因缺 coverage 被拒；公開 required coverage 集合為 []／未知，不補造決定；其他發布閘門照常驗證 |
| D18／I | sv1 代碼 5／6 對 SVE nightmare；另對照未列的新代碼、nemesis 或對到 SVE 其他職業 | 前者僅通過職業分層條件，仍驗其餘條件與真人採納；後者不通過，不由工具補表或以 null 相等放行 |

docs 階段只審上述形狀與邊界；程式階段再測嚴格入口、交易、來源重播、閉包及名稱呼叫接線。真實遷入另報能重驗的採納數、232 筆待決及新增待件、覆蓋範圍／as_of，不用合成測試數或草稿信心當人工採納數。

## 8. 實作入口與離線候選報告

`digital_links.loader.load_links(authored_root)` 驗全入口並返回不可變 snapshot；每次取封套得到獨立的型別化值。第一版只接受 links，coverage 路徑／kind 明確拒絕，沒有紀錄即未知。`digital_links.importer.Inputs` 驗本機入口 bytes 與明示 authored commit 完全相同，再把 index／分片 exact 與 canonical hash 納入 `digital_link_authored` 設定。不以目前 working tree 或單一分片代替全入口。

建置設定另需 `digital_link_sources`：按 canonical 值排序唯一的 `{batch_id}` 陣列，列本次明示凍結來源。`catalog_registry` 沿既有 registry pin；`translation_recipes` 沿既有凍結 parser pin。`digital_evidence` 沿既有 `translations.digital.configuration()`，列明示 API refs 與 targets。目前建置驗載入的執行期；每筆 link 的引用以目前解析器重驗，名稱閉包也與本次來源比較，不能只交目標語或已選目標的子集。

在 caller-owned transaction 內先呼叫 `populate_links()`；`snapshot/offline_names.Composer` 將結果以 `links=result` 交給 `digital_name_policies.application.prepare()`／`populate()`，再委派 `current_application` 驗期望來源閉包與產生 current 名稱。獨立的 `import_links()` 包裝才開 transaction。link 結果含 fresh records、stale 的 link id 與原因及 F1 input record，並按 card／face 索引 fresh records。application 的 `_counterparts()` 經 `result.eligible_owner(db, sources, owner, name_ref=...)` 每次重驗此 owner 的 JP hash、registry/source_face_map、實際 frozen printing 來源與採納的兩端名字，並比對 materialized link 的完整 subject／值／decision；`counterparts.first_counterpart()` 只在此證據範圍內選真人候選，不另建名稱算法。

離線 composer 以本機 `authored/digital-links` 是否存在判定是否匯入 link；入口存在時由 loader 驗全入口與 authored commit bytes，沒有入口不偽造空採納結果。沒有 link result 時，不提供真人同卡候選；政策名字仍按其獨立條件判斷。另一 owner 不因已有 context／translation 得到權限；同原文異譯而未有 context_assignment 時回報上述概念診斷，不任選概念。pending wording 的 JP revision 同樣可驗；printed owner、語義指派與 current translation/use／bindings 已有現行接點，#53 的其餘需求須沿此路徑接續，不能用 link API 宣稱全部完成。匯入器只做 partial F1 verify；完整建置必須把 result.record.uses、逐 owner 的 sources.uses 及其他階段使用合併，用 `input_record(...).verify(..., complete=True)` 驗實際 DB raw source 閉包後才輸出。

### 8.1 候選工具

`sve-carddb digital-links candidates` 只讀私人研究草稿與明示凍結來源：

```bash
sve-carddb digital-links candidates \
  --draft /absolute/private/research.json \
  --context /absolute/private/context.json \
  --repository /absolute/repository \
  --store declared-store=/absolute/archive \
  --output /absolute/private/candidates.json
```

context 是既有 BuildContext JSON，含上述來源批次、registry 與 translation recipes；程式與新增 runtime 依賴都須釘住。候選模式不要求正式 digital-links 入口，正式匯入不讀研究草稿。store 可重複指定不同 ID，不開 live manifest、不抓網路。output 必須絕對、非 symlink 且與 repo／來源／草稿／context 隔離；暫存完整寫好才替換報告，不寫 authored 或採納決定。

工具重播同遊戲全部凍結 API 頁，sv1 限完整 cards URL，svwb 限 include_token=1 的未篩選頁，驗 count／offset／card_details 閉包。不用 data.cards 的稀疏索引代替內容。完整目錄中另有同名異譯或缺目標語會使第一層失敗；未知職業／卡種也不能當相同。候選卡種目前以複合原文分隔後的基本標籤做固定表比較，僅是機械分層啟發式，不是正式詞彙採納；正式分類沿已採納詞彙，後續候選工具應接其推導結果，不能由切字結果授權採納。SVE 頁只沿 registry 明示 face map 配面，所有命中名稱的版次／面都比較。數位實際 phase 列作 possible_phase，仍需真人明示配面，不能由順位替他決定。

報告不含名稱／卡文，只有草稿行號、ID、name_ref/hash、可能的面及機械失敗條件。summary 的 row 是草稿行，source_occurrences 是來源關係出現次數；by_game 的 tier 是承襲整列分層，local_failures 只計該遊戲，whole_row_failures 包含其他遊戲影響。失敗條件有重疊，不能直接相加。原待決、mixed 或任一條件不成立均留第二層；confidence 不提高門檻，也不寫 sampled／confirmed。reason 只說機械比較，沒有真人採納事件。

報告保存 draft_hash、完整 F1 使用與職業表 mapping_hash。後續採納仍需維護者實際抽樣／逐筆確認、明示 card／face／phase 及來源，再寫成正式 record；分層通過不代表關係已採納，數位同名候選也不自動升成 same_card。

### 8.2 同名規則與能力邊界

[數位名字政策](digital-name-policy.md) 的 links 政策只保存凍結來源批次與排除清單，同名規則的條件在程式常數。
本入口只收真人關係，不接受 same_name 或假的真人 record。
規則連結目前只出現在私人診斷報告；正式投影待 same_name 能力實作，缺支援時不產生公開規則連結。
真正 same_card 的兩層排審、明示職業對照與 sampled／confirmed 不改；same_name 不把 84 同角色草稿記成 same_card。
93 個警訊與 232 待確認依維護者選的「照規則連」處理，但不算真人樣本；候選指令仍只產排審報告。
coverage 仍不採納；未來完整集合須含有效規則連結，不能以此簽 reviewed_none。
當批 caller transaction 及 complete record.verify 沿 §5／§8，不以 partial 驗證代替正式發布。
