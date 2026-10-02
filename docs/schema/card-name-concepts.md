# 卡片身分與卡名概念的關聯

本文件補充 [翻譯契約 §5／§6](translation-contract.md#5-概念選詞與數位證據) 的 N-a：
SVE 永久 card／face 身分及 exact 名稱如何對到已採納的 `category=card_name` 概念。
本文件是技術契約，不表示資料、loader、renderer 或公開路徑已接入。
卡名採納仍沿 [術語採納契約](glossary-adoption.md)；維護者 2026-10-02 委託協調者處理自譯卡名，
不等於已配發所有 key、核可本關聯或逐筆看過譯詞。真正的委託範圍與事件須另存既有 delegation 收據。

## 1. 既有入口與新增邊界

| 既有入口 | 本契約的用法與限制 |
| --- | --- |
| glossary_term／glossary_choice | 概念與譯詞沿用原格式；`id=term:<concept_key>`、手配英文 key、不可變概念與選詞續版均不改 |
| context_assignment.concept_key | 重用於 exact 名稱 owner 的概念／語義指派；非 default 仍只處理真實同字異義，不是讓不同卡任意換翻法 |
| select_name／populate_name_translation | 重用已採納 same-card、同數位面與凍結語言證據，SVWB 優先 SV1；不另建平行官方名稱詞庫 |
| rules_name／face_rules_name | 構築計數的同名單位及特殊規則名稱，不是 card_name glossary 關聯；不能把構築同名當同概念 |
| identity-transition-v1 | 沿有效身分重播驗永久鍵與父卡；不能從 URL alias、舊牌組提示或同名字串推譯名關聯 |

`context_assignment` 的鍵是來源 owner／field／ordinal，而非永久 card／face；既有 data 也沒有核對時
registry／transition 基準，不能單靠它替 `{kind:card,id}` 選面或證明修復後仍是同概念。
因此新增**獨立關聯 kind**，只記身分與名稱概念的人工關聯；語義 variant、glossary、數位對應與選用仍各走原入口。
不在 glossary_term 塞 card_id／face_id，不以永久卡 ID 作 concept_key，不重算已採納 key。

## 2. card_name_concept 採納格式

新 kind `card_name_concept` 歸 `translations/overrides/<filing_key>/<sequence>.yaml`，
沿翻譯契約 §2 的 index、分片五欄、完整 record 五欄、單 kind／單 decision、全入口驗證、
排序成員與 canonical hash。filing_key 只歸檔，例如 `name-concepts`，沒有選用優先序。
不新增永久庫、公開表或新的 index；`translation_authored_format=1` 的既有 glossary record 不加欄。

record_key 為 `["card_name_concept",subject,adoption_no]` 的 canonical JSON 字串。
data **恰為** `{subject,term_id,source_ref,identity_basis,reason,adoption_no,predecessor}`：

| 欄位 | 定義與拒絕條件 |
| --- | --- |
| subject | 恰為 `{card_id,face_id,source_lang,source_hash}`；card／face 為永久 ID，source_lang=ja/en，source_hash 為完整 exact 名稱 UTF-8 Hash；整個物件是選擇鍵，續版不可換鍵 |
| term_id | 已採納、不可變的 glossary_term.id，category 必須 card_name；null 為明示撤回，首筆不得 null。不放譯文字串或未採納候選 |
| source_ref | 翻譯契約的六欄 frozen 引用，定位 SVE parser 的完整 name 欄；語言與 subject.source_lang 一致、text_hash 等於 subject.source_hash。不得用效果摘錄、數位卡名或 URL 代替實體卡來源 |
| identity_basis | 恰為 `{authored_revision,registry_index_hash,transition_index_hash}`；完整 40 碼 commit、該 revision 的 ids/index.yaml canonical hash、identity-transitions/index.yaml canonical hash 或 null |
| reason | 非空，人寫同概念理由／撤回理由；可記核對結論，不能保存官方原文、私人路徑或 confidence |
| adoption_no／predecessor | 沿既有只增採納鏈；1/null 起，其後精確指緊接前件 record_key／record_hash／decision_id；拒絕分叉、缺號或錯前件 |

`transition_index_hash=null` **只代表在該 immutable revision 沒有此入口檔**，讀者須驗明缺檔；
有空 index 也要釘它的 hash。有非空入口須完整重播，不能用原始 registry 當有效投影。
兩個 index 及全部分片的 canonical／exact bytes、有效 record 決定與重播 recipe 都納入核對／建置 F1。
本關聯不能引用尚未寫出的自身 commit：identity_basis 釘已存在的核對基準，
新的 translations index／分片另由消費端的 authored_revision 釘住，沒有 hash 自我引用。

核對時由 source_ref 的凍結卡頁來源、原樣 region／card_no、parser source_index 與基準的
source_face_map **共同**驗 `(card_id,face_id)`，不得僅驗外鍵、依面 ordinal 猜對應或依名字找卡。
原始 registry 的每筆 observation／cross-region 證據及所有相關 transition／來源均須可驗。
概念的 exact `source_ja` 與 SVE JA 名稱相同時仍要有這份身分關聯，不把字串相等當人工採納。
名稱不同但確屬同概念（別名／改名）或 EN 名稱，reason、來源證據與人工核對必須支持此關聯；
不要求 EN 字串等於 source_ja，不以已存在 JA 關聯自動擴張 EN 範圍。

decision 沿翻譯契約的完整封套：scope=batch、category=card_name_concept、
policy_id=card-name-concept-v1、state=sampled/confirmed，採**實際人工**核對。
sampled 需非空真人樣本；confirmed 的 sample_ids 為全體 checked 成員。關聯與 context_assignment 各自成批。
`delegated_glossary` 僅適用原契約的四種 glossary kind，不可擴到本 kind；
模板長尾／表記政策也不能代簽本關聯。關聯採納不代簽概念、choice、面對應或身分修復。

## 3. 名稱 owner 的預設綁定

名稱來源 owner 仍只有 `face_revision.name`、以及實際已知的 `printing_face.name`，
field=name、ordinal=null。工具依**實際選中原文**的永久 card／face、語言與 exact hash 查有效關聯，
唯一命中時得到 term_id；缺關聯／已撤回則列 `missing_name_concept`，不靠全 glossary 搜同字補洞。
同一選擇鍵只允許一條採納鏈，不能同時指兩個概念。

一般關聯的預設為 `variant=default`。已有 context_assignment 時，source_hash 必須吻合實際 owner，
concept_key 必須等於關聯 term 的 **concept_key**（不是帶 term: 前綴的 id），variant 依該採納指派。
指派與關聯矛盾是建置錯誤，不能以檔案順序取其中一份。

同語言、同 exact 原文若被採納為不同概念，不能全塞 default context；須為各語義有理由地採納
context_assignment，再各自用 `(source_unit_id,variant)`。未完成消歧時列
`ambiguous_name_concept`，受影響名稱／引用回原文，不任取一個譯詞或以 card_id 機械生成 variant。
同字且同概念則共用同一 term／variant，不因 owner 或官方資格不同就開新語義。
不存在人工指派時 default 只在已驗無歧義的集合可用；新 owner 加入使集合歧義須重驗舊用途。

取該 term 的有效 target_lang choice，未採納／已撤回列 `missing_term_translation`。
單一名稱原樣取詞沿既有名稱推導路徑：不新增任意全文 kind、不假造 literal slot；
translation_term 記概念，dependency_key 釘有效關聯、語義指派、精確 choice record_hash／decision 與來源，
生成 ID／revision／source_hash 沿翻譯契約 §6。將來 renderer 應保留引用追蹤，不從純字串猜加粗位置。
context/use／selection 仍是每次建置的產物，不寫入 authored。

`printed.name` 先要求該 printing_face 的 printed_text_state 與 exact 名稱可驗，
再以該版次的原文查關聯，不從 current 填 unknown。
同一 card／face 的不同歷史名稱是不同 source_hash 鍵，各自可採納到同概念或不同概念；
current 改名不搬移舊指派，printed 的有效舊名不因 current 改字被覆寫。
表記／效果採納仍 pending 時，已知身分及該名稱來源照常參與判斷，不整卡排除。

## 4. 卡文中的卡名引用與選面

模板參數形狀不變。`{kind:card,id}` 的 id 一律是永久 **card_id**，
其**預設名稱面固定為該卡 front／ordinal=0 的有效面**，不是「目前來源剛好是哪面」、
數位 normal／evolved 或第一個 SQL row。雙面卡的 back 不冒充 front；一般進化前後是不同 card，
不能把前後兩卡以 face 規則合成一張。

解析 card 引用前須有已採納的身分引用，選面後依同區、真正選中名稱來源查 §3 的關聯與 choice。
名稱可用來源遵守翻譯契約既有 JP／EN 政策，不以缺 JP 就猜 EN-only，不靠 frozen 名稱字串猜 card_id。
若規則引用明確指背面／其他名稱，既有 card 參數不足以表達這個選面，
改用已採納 `card_name` 概念的 `{kind:term,id}` 與 `reference_kind=term`，
由來源／匹配採納證明這個概念；不得在 card 參數偷偷加入 face_id。
schema 換種類須新模板 ID、重驗 raw span／引用與譯本，不能原地改舊模板。
沒有足夠證據的引用留待確認，不猜對應；既有「無法對到 card 的名稱可採納原創 term」途徑仍可用，
但那不宣稱已建立 SVE 身分關聯。

引用缺有效關聯、語義未解、選詞缺譯或目標退役時，整個效果 context 回原文並列原因，
不混入未翻卡名後標完整譯文。明確的壞外鍵／hash／閉包則是建置失敗，不吞成缺譯。
translation_term／dependency_key 保留實際使用的 term、關聯及精確選詞 hash；
term 的字改由 choice 續版帶動重新渲染，不逐卡重簽關聯。

## 5. 數位官方名稱不能從共用 context 借資格

官方名稱重用 `select_name()`／`populate_name_translation()` 的 same-card／同面／目標語／frozen 名稱證據。
本關聯與 glossary 官方 choice 只是必要採納資料，**不是 owner 的 digital_link**。
每個 revision／printing owner 仍須重驗當下有效 card／face、已採納 link 與 exact 名稱；
same_character、同字、數位前後面名一樣、同 context 或已有官方 translation 均不足。
printed 名稱與凍結的可用官方名不符時不可借 current；第三張無 link 的同名卡不得借前兩張的官名。

既有名稱 API 目前只產 JP→zh-Hant、default context 的證據列，不建立 use／selection；
非 default、印刷模式及 EN 等能力由 #53 擴充同一路徑，而不是繞過現有拒絕或另造名稱詞庫。
未支援時停在明示能力缺口，不宣稱上述路徑已可用。

同一 context 的 owner 官方資格不同時，不能把 owner 限定的官方名放進共用 selection
讓全部 use 都拿到；#53 必須按 owner 使用重驗後 API 回傳的精確 translation ID，
沿既有逐 owner 選用原則組成 FieldTranslation。若該輸出所需資格還未接入，保留原文／
可用的專案選詞，不能以新的假 variant 分隔官方資格。相同 source／概念的普通譯本仍共用 selection。
SVWB／SV1、machine／project 與 authority 的既有標示不變，數位官方卡名不使效果譯文變官方。

## 6. 身分修復、保留與相容

載入先驗歷史 identity_basis 與關聯當時成立，再對本次有效身分／來源驗適用性。
卡／面永久鍵不得重配，退役原卡的關聯留歷史，不當作新卡的名稱關聯。
merge／split／reassign 後，printing 移到新 card／face 須新選擇鍵、新關聯與人工核對；
不得沿修復路由搬 term、僅換 parent 或繼承原卡的名稱／官方資格。
分拆不能猜哪個新卡承接卡名概念；概念本身仍可在真正確認同概念後被新關聯引用，不改舊 term。
同 subject 仍成立的無關身分追加不要求重簽，F1 另釘本次有效內容；撤回修復也不能使舊翻譯資格自動恢復，
須重驗有效父卡、來源與指派。若重播能力尚未支援該 transition，建置應拒絕，而不是跳過。

目前的 264 個概念與 39 個已採納分片一律不改，glossary record_key／data／hash 與 format=1 封套不變。
新增 kind 有封閉欄位、完整歷史／來源／採納驗證才可載入；舊 glossary loader 對非空 overrides
仍應拒收，不能改成只讀 glossary 並忽略新入口。索引只追加新分片映射，既有 hash 不改。
單檔完整 YAML（封套、evidence、決定、成員在內）仍須 <1 MiB，歷史只增不改。

這是建置期關聯，不新建公共名字庫或改七欄 translation／FieldTranslation／引用參數形狀。
公開仍出實際選中的名稱文字與來源標示，名字在 bootstrap 閉包內；
無格式升版，公開加粗位置仍由獨立契約處理。

## 7. 合成驗收清單

名稱 `測試星`／`測試月` 等均自撰；ID、hash、收據亦用合成資料。下列是後續實作驗收，非已執行測試。
每條拒絕從已合法、可達該層的成功基例單獨改一個條件。

| 編號 | 合成案例／最小變更 | 預期 |
| --- | --- | --- |
| N01 | confirmed 單面卡、凍結 `測試星`、human 關聯、已採納 machine 卡名 choice | default name use 可推導，保留 machine／unofficial |
| N02 | 兩個 owner 同原文／同概念；再錄增加，相關原文與身分不變 | 共用 term／variant，無需每 owner 新 key 或每次重簽 |
| N03 | 同字不同概念，兩份有理由的 context_assignment | 不同語義 context；人工採納不由 card_id 自動替代 |
| N04 | 雙面前後名稱不同，card 引用與明確 back 的 term 引用各一例 | card 選 front；back 走 term，二者不混；不改參數形狀 |
| N05 | printed 舊名／current 新名，各有 exact 關聯 | 各自取詞，current 改字不冒用於 printed |
| N06 | 首筆、連續改 term 的下一筆、null 撤回各一例 | 正確鏈可驗；撤回後缺關聯，不搜索同名回補 |
| N07 | 缺／多關聯、同鍵分叉、前件 hash 錯、term 非 card_name 或未採納各一次 | 各自拒絕；真正未採納關聯列 missing，壞閉包為建置錯誤 |
| N08 | 凍結 name hash 改、ref 指效果／另一來源面、identity_basis hash 錯各一次 | 各自拒絕，不只驗外鍵或字串 |
| N09 | EN 字串不等於 JA 但已人工證明同概念；缺 EN parser pin 的相同案例 | 前者在已支援能力中可用；後者能力拒絕，不偷偷用 JP ref |
| N10 | 同字不同概念卻無語義指派；指派 source_hash 改；concept_key 與關聯矛盾各一次 | 分別待消歧／舊指派失效／建置錯誤；不得任取譯詞 |
| N11 | template card 參數加 face_id、依 SQL 首列選 back、卡名猜 card_id 各一次 | 各自拒絕，背面用已採納 term／新 schema |
| N12 | 兩張同名有不同有效 digital 名稱、第三張無 link | 每 owner 只可用自己的已驗名稱；第三張不借官方 context |
| N13 | same_character、錯數位面、缺 zh-Hant、已失效 link、printed 名不符各一次 | 各自不能用該官方名稱，原文／可用專案詞仍可顯示 |
| N14 | split 後新 card／face、merge 退役原卡、reassign 面對應變更各一次 | 舊關聯不搬移；新 exact subject 需新採納，term key 保留 |
| N15 | 普通 choice 改字；只改無關身分；source 原文換字各一次 | 依賴者重渲染／關聯仍有效／新 source_hash 另採納，不重配 term |
| N16 | 關聯用 delegated_glossary、approved_policy、wording 核可各一次 | 各自拒絕，例外不跨 kind 代簽 |
| N17 | glossary 加 card_id／改已入庫 record_hash／未知 kind 靜默忽略各一次 | 各自拒絕；新關聯不用改舊 39 分片 |

後續 #51／#196 的正式資料、#52／#53 的載入與選用、#37 的有效身分能力須各自驗收；
本文件不宣稱數位同卡證據、真實委託收據或 repaired registry 已可發布。
