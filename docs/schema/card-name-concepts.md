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
| select_name／populate_name_translation | 接獨立typed名字政策及自己真人 same-card／同面證據，共用SV1→SVWB resolver；same_name瀏覽不供名，不另建詞庫 |
| rules_name／face_rules_name | 構築計數的同名單位及特殊規則名稱，不是 card_name glossary 關聯；不能把構築同名當同概念 |
| identity-transition-v1 | 沿有效身分重播驗永久鍵與父卡；不能從 URL alias、舊牌組提示或同名字串推譯名關聯 |

`context_assignment` 的名稱 owner 是面修訂，修訂 ID 已包含永久 face／地區／內容指紋，
並非沒有綁定永久面。它也會隨卡文勘誤換 ID，即使名稱不變；例外關聯以永久卡／面與 exact 名稱
為鍵，避免因此失去已採納的同概念證據。它不取代語義 Code 的人工採納或官方同卡證據。
因此新增**例外關聯 kind**，只記同字異義、改名／別名及個別英文特例等需要人工證明的例外；
預設按 §3 自動推導，不需要關聯紀錄。語義 variant、glossary、數位對應與選用仍各走原入口。
不在 glossary_term 塞 card_id／face_id，不以永久卡 ID 作 concept_key，不重算已採納 key。

## 2. card_name_concept 採納格式

新 kind `card_name_concept` **只記例外**，歸 `translations/overrides/<filing_key>/<sequence>.yaml`，
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
概念的 exact `source_ja` 與 SVE JA 名稱相同且唯一時，依 §3 推導即可，**不得要求另簽關聯**。
同字異義需要指定不同概念時才記例外；預設推導不新增譯詞或人工採納決定。
名稱不同但確屬同概念（別名／改名）或 EN 名稱，reason、來源證據與人工核對必須支持此關聯；
不要求 EN 字串等於 source_ja，不以已存在 JA 關聯自動擴張 EN 範圍。

下列門檻**僅適用例外關聯**，不適用每個卡名或每個新卡包的機械推導。
decision 沿翻譯契約的完整封套：scope=batch、category=card_name_concept、
policy_id=card-name-concept-v1、state=sampled/confirmed，採**實際人工**核對。
sampled 需非空真人樣本；confirmed 的 sample_ids 為全體 checked 成員。關聯與 context_assignment 各自成批。
`delegated_glossary` 僅適用原契約的四種 glossary kind，不可擴到本 kind；
模板長尾／表記政策也不能代簽本關聯。關聯採納不代簽概念、choice、面對應或身分修復。

## 3. 名稱 owner 的預設綁定

名稱來源 owner 仍只有 `face_revision.name`、以及實際已知的 `printing_face.name`，
field=name、ordinal=null。每次建置依真正選中來源的有效永久 card／face、語言及 exact 名稱重算，
**預設不寫任何 authored 關聯**，也不因新卡包、再錄或無關身分追加要求真人抽查。

先驗所有採納入口的完整歷史／hash／來源閉包，然後按以下順序解析：

1. 查該 `(card_id,face_id,source_lang,source_hash)` 的有效例外關聯。唯一且非 null 時使用它的
   term_id；同鍵只能有一條鏈。撤回或沒有例外時回預設推導，不把撤回當永久缺譯。
2. 預設以 **`(來源語言,完整 exact 名稱字串)`** 對已採納 `category=card_name` 概念的原文：
   frozen_source 依凍結引用／span 重建，authored_concept 依已採納人工原文；不修剪、正規化、
   模糊比對或依譯文合併。恰好一個 term 才使用，default context 依既有 recipe 重建。
   現行 glossary 概念原文語言為 ja，不能把 source_ja 當 en，不能拿數位英文名猜同概念。
   已採納 registry 中，有 EN 版次而無 JP 版次的卡共 **127 張**；這是版次集合計數，
   不等於已確認英文獨有。這個量不以例外關聯處理：術語契約允許 card_name 概念以英文為原文前，
   列缺概念、顯示原文。別名、改名及個別英文特例才走有證據的例外；
   後續另開契約修訂，歸 #195 後續或 EN 術語採納另案由協調者決定。
   JP／EN 的來源選擇仍先遵守既有契約，不借此放寬。
3. 在未有有效人工消歧時，以下三種情況停止未解概念的推導並列待人工報告；直接名字另依有效名字政策／自己的真人供名與總順位，不因概念待件一律回原文：

| 報告 reason | 機械條件 | 後續處理 |
| --- | --- | --- |
| ambiguous_name_concept | 同語言、同 exact 名稱對到兩個以上已採納 card_name 概念 | 人工確認概念與語義分支，採納例外關聯／必要的 context_assignment |
| missing_name_concept | 沒有任何概念對得上 | 直接名字另驗有效名字政策；效果引用需概念／choice；改名、別名或 EN 特例仍依關聯 |
| conflicting_official_name | 該 owner 自己有效同卡連結所得官方譯名，與候選概念的有效目標語 choice 不同 | 純譯名字串差異按總順位顯示勝出名字並報差異；真正語義衝突仍消歧，不借其他 owner 官名 |

第三項沿 `names.py` 已有的同原文不同官方名稱歧義防線，#53 並須在概念選詞與該 owner 的
官方 API 結果之間作同等檢查；現有 API 尚未包含這個 glossary 對照，不能宣稱已完整接入。
有效例外或指派仍須與自己的官方證據相容，錯配不能藉例外豁免。
報告逐 owner 列永久 card／face、owner ID、語言、source_hash、候選 term_id、reason，
並分別彙總三類 owner 筆數，不輸出原文／譯文。壞外鍵、hash、歷史鏈則建置失敗，不冒充待人工。

名字 `context_assignment` 自己的 identity_basis 必填，歷史核對與當次適用性分開，
依 [翻譯契約 §6.1.1](translation-contract.md#611-context_assignment-的不可變身分背景)。
消費端 registry observation 更新不能讓舊合法採納報錯，也不把舊指派搬給新 owner；
印刷名稱未變且本次有效永久卡／面與 printed 狀態仍成立時，可沿用該 owner 的指派。

有效 context_assignment 的 source_hash 必須吻合實際 owner；concept_key 必須等於選中 term 的
**concept_key**（不是 term: 前綴的 id），矛盾是建置錯誤。人工指派能消歧時照它的已採納 variant
推導；例外關聯只證明概念，不自行配發 variant Code。同原文選不同概念仍須有理由的人工語義
分支，未具足時列 ambiguous_name_concept，不以 card_id 生 variant 或把兩個概念塞同 default。
同字且同概念則共用 term／variant，不因 owner 或官方資格不同就開新語義。
新 owner 加入使集合歧義時須重驗舊用途。

取該 term 的有效 target_lang choice，未採納／已撤回列 `missing_term_translation`。
單一名稱原樣取詞沿既有名稱推導路徑：不新增任意全文 kind、不假造 literal slot；
translation_term 記概念，dependency_key 釘當次推導 recipe／概念來源、實際用到的例外關聯／語義指派、
精確 choice record_hash／decision 與來源，
生成 ID／revision／source_hash 沿翻譯契約 §6。將來 renderer 應保留引用追蹤，不從純字串猜加粗位置。
context/use／selection 仍是每次建置的產物，不寫入 authored。

`printed.name` 先要求該 printing_face 的 printed_text_state 與 exact 名稱可驗，
再以該版次的原文依本節推導／查例外，不從 current 填 unknown。
同一 card／face 的不同歷史名稱是不同 source_hash 鍵，各自推導，必要時可採納例外到同概念或不同概念；
current 改名不搬移舊指派，printed 的有效舊名不因 current 改字被覆寫。
表記／效果採納仍 pending 時，已知身分及該名稱來源照常參與判斷，不整卡排除。

## 4. 卡文中的卡名引用與選面

模板參數形狀不變。`{kind:card,id}` 的 id 一律是永久 **card_id**，
其**預設名稱面固定為該卡 front／ordinal=0 的有效面**，不是「目前來源剛好是哪面」、
數位 normal／evolved 或第一個 SQL row。雙面卡的 back 不冒充 front；一般進化前後是不同 card，
不能把前後兩卡以 face 規則合成一張。

解析 card 引用前須有已採納的身分引用，選面後依同區、真正選中名稱來源依 §3 推導概念與 choice，必要時查例外。
名稱可用來源遵守翻譯契約既有 JP／EN 政策，不以缺 JP 就猜 EN-only，不靠 frozen 名稱字串猜 card_id。
若規則引用明確指背面／其他名稱，既有 card 參數不足以表達這個選面，
改用已採納 `card_name` 概念的 `{kind:term,id}` 與 `reference_kind=term`，
由來源／匹配採納證明這個概念；不得在 card 參數偷偷加入 face_id。
schema 換種類須新模板 ID、重驗 raw span／引用與譯本，不能原地改舊模板。
沒有足夠證據的引用留待確認，不猜對應；既有「無法對到 card 的名稱可採納原創 term」途徑仍可用，
但那不宣稱已建立 SVE 身分關聯。

引用缺可推導概念、語義未解、選詞缺譯或目標退役時，整個效果 context 回原文並列原因，
不混入未翻卡名後標完整譯文。明確的壞外鍵／hash／閉包則是建置失敗，不吞成缺譯。
translation_term／dependency_key 保留實際使用的 term、關聯及精確選詞 hash；
term 的字改由 choice 續版帶動重新渲染，不逐卡重簽關聯。

## 5. 數位官方名稱不能從共用 context 借資格

官方名稱走同一 `select_name()`／`populate_name_translation()` 路徑，明示接獨立typed名字政策結果及自己有效真人same-card／同面／目標語證據；依 [數位名字政策](digital-name-policy.md) 一代優先。不得為相容舊API塞假digital_link。
例外關聯不是預設必填資料；它與 glossary 官方 choice 均**不是 owner 的 digital_link**。
每個 revision／printing owner 仍須重驗自己有效 card／face、完整exact名稱及所用政策或自己真人link證據；
same_character、同字、數位前後面名一樣、同 context 或已有官方 translation 均不足。
printed 名稱與凍結的可用官方名不符時不可借 current；第三張無link但有自己合法政策證明可供名；政策異譯／缺譯又無自己真人link才不得借前兩張官名。逐名排除維持不能借任何link繞回自動官名。

既有名稱 API 目前只產 JP→zh-Hant、default context 的證據列，不建立 use／selection；
非 default、印刷模式及 EN 等能力由 #53 擴充同一路徑，而不是繞過現有拒絕或另造名稱詞庫。
未支援時停在明示能力缺口，不宣稱上述路徑已可用。

同一 context 的 owner 官方資格不同時，不能把 owner 限定的官方名放進共用 selection
讓全部 use 都拿到；#53 必須按 owner 使用重驗後 API 回傳的精確 translation ID，
沿既有逐 owner 選用原則組成 FieldTranslation。若該輸出所需資格還未接入，保留原文／
可用的專案選詞，不能以新的假 variant 分隔官方資格。相同 source／概念的普通譯本仍共用 selection。
SVWB／SV1、machine／project 與 authority 的既有標示不變，數位官方卡名不使效果譯文變官方。

## 6. 身分修復、保留與相容

預設推導每次建置重算，身分修復後自動反映有效 card／face 與真正選中來源，不搬舊資格。
例外載入先驗歷史 identity_basis 與關聯當時成立，再對本次有效身分／來源驗適用性。
卡／面永久鍵不得重配，退役原卡的關聯留歷史，不當作新卡的名稱關聯。
merge／split／reassign 後，printing 移到新 card／face，預設仍重新機械推導；
若仍需人工例外，須新選擇鍵、新關聯與人工核對；
不得沿修復路由搬 term、僅換 parent 或繼承原卡的名稱／官方資格。
分拆不能猜哪個新卡承接卡名概念；概念本身仍可在真正確認同概念後被新關聯引用，不改舊 term。
卡文勘誤換面修訂 ID，但 card／face、語言、名稱 hash 未變時，例外關聯仍有效；
use／binding 依新 owner 重建，owner 指派的適用性仍依 §3 重驗，不冒用舊修訂的 context_assignment。
同 subject 仍成立的無關身分追加不要求重簽，F1 另釘本次有效內容；撤回修復也不能使舊翻譯資格自動恢復，
須重驗有效父卡、來源與指派。若重播能力尚未支援該 transition，建置應拒絕，而不是跳過。

目前的 264 個概念與 39 個已採納分片一律不改，glossary record_key／data／hash 與 format=1 封套不變。
新增 kind 有封閉欄位、完整歷史／來源／採納驗證才可載入；舊 glossary loader 對非空 overrides
仍應拒收，不能改成只讀 glossary 並忽略新入口。索引只追加新分片映射，既有 hash 不改。
單檔完整 YAML（封套、evidence、決定、成員在內）仍須 <1 MiB，歷史只增不改。

這是建置期關聯，不新建公共名字庫或改七欄 translation／FieldTranslation／引用參數形狀。
公開仍出實際選中的名稱文字與來源標示，名字在 bootstrap 閉包內；
無格式升版，公開加粗位置仍由獨立契約處理。

## 7. 新卡名概念永久 key 的配發建議

沿術語採納契約的手配英文概念 key，建議 card_name 使用穩定大類 `name.<slug>`，
例如合成概念 `name.test_star`。每個候選均須配得到永久 key，不能因缺現成英文名而保持未配發；
key 是內部識別，不顯示給使用者，只要求唯一、穩定、能核對概念，不要求正確英文。
英文名稱只借來命名，不等於採納 EN 譯詞、官方繁中譯名或跨區關係。

候選 slug 依序取用，前一來源不可用時才往後：

1. SVE 官方英文卡名：須該卡有已確認的日英同卡關係與可驗來源。
2. 官方數位英文卡名：須有已採納的同卡連結與可驗的同數位面名稱。
3. 仍無上述名稱時，模型依日文名提出英文意譯或羅馬字代號，由協調者在完整對照表核可。
   不讓「英文不夠準」阻擋原創／合作角色概念與譯名採納。

以上只產生候選，不從名稱自動認定同卡。slug 僅含 ASCII 小寫英文字母、數字與底線，
符合 `[a-z][a-z0-9]*(?:_[a-z0-9]+)*`；key 完整長度（含 `name.`）最多 96 bytes。
空白／標點轉成分詞底線、小寫化，非 ASCII 部分另提意譯／羅馬字；不可留下空 slug，
連續／首尾底線須消除。超長、同字異義或撞名，協調者改用較短代號或英文語義限定詞，
重新全體檢查；不自動加流水號或 hash，也不用卡 ID、日文／繁中譯名字串或草稿列序配 key。

配發前對**本次完整候選集合與所有已採納 key** 一起驗字元、長度及全體唯一性，
相同概念重用同一 key，不同概念不得同 key；全部通過後才配發，不能只逐分片檢查。
完整候選→概念對照表由獲委託的協調者依既有收據範圍核可；不另造委託格式。
這是給協調者決定的命名提案，不表示 3,010 筆候選的 key 已配發。
現有 264 概念及 39 分片的 key／hash 一律保留；名稱改字或再錄也不重配。

## 8. 合成驗收清單

名稱 `測試星`／`測試月` 等均自撰；ID、hash、收據亦用合成資料。下列是後續實作驗收，非已執行測試。
每條拒絕從已合法、可達該層的成功基例單獨改一個條件。

| 編號 | 合成案例／最小變更 | 預期 |
| --- | --- | --- |
| N01 | confirmed 單面卡、凍結 `測試星` 唯一對到已採納概念與 machine choice；沒有任何關聯紀錄 | default name use 可推導，保留 machine／unofficial |
| N02 | 兩個 owner 同原文唯一對到同概念，無關聯紀錄；再錄增加 | 共用 term／variant，無需每 owner 新 key 或每次重簽 |
| N03 | 同字不同概念，兩份有理由的 context_assignment | 不同語義 context；人工採納不由 card_id 自動替代 |
| N04 | 雙面前後名稱不同，card 引用與明確 back 的 term 引用各一例 | card 選 front；back 走 term，二者不混；不改參數形狀 |
| N05 | printed 舊名／current 新名，各自唯一對到已採納概念，或各有合法例外 | 各自取詞，current 改字不冒用於 printed |
| N06 | 首筆、連續改 term 的下一筆、null 撤回各一例 | 正確鏈可驗；撤回後重新預設推導，仍歧義則待人工 |
| N07 | 同鍵多鏈／分叉、前件 hash 錯、term 非 card_name 或未採納各一次 | 各自建置拒絕，不能靜默改走預設 |
| N08 | 凍結 name hash 改、ref 指效果／另一來源面、identity_basis hash 錯各一次 | 各自拒絕，不只驗外鍵或字串 |
| N09 | EN 字串不等於 JA 但已人工證明同概念；缺 EN parser pin 的相同案例 | 前者在已支援能力中可用；後者能力拒絕，不偷偷用 JP ref |
| N10 | 同字不同概念卻無語義指派；指派 source_hash 改；concept_key 與關聯矛盾各一次 | 分別待消歧／舊指派失效／建置錯誤；不得任取譯詞 |
| N11 | template card 參數加 face_id、依 SQL 首列選 back、卡名猜 card_id 各一次 | 各自拒絕，背面用已採納 term／新 schema |
| N12 | 兩張同名有不同有效 digital 名稱、第三張無 link | 政策目錄同名異譯時，前兩張有自己真人 B＋必要指派才各取自己的名；第三張無 B 不借。若另有合法政策名字則驗自己資格 |
| N13 | same_character、錯數位面、缺 zh-Hant、已失效 link、printed 名不符各一次 | 各自不能靠該真人關係供官名；自己的獨立名字政策另驗，原文／可用專案詞仍可顯示 |
| N14 | split 後新 card／face、merge 退役原卡、reassign 面對應變更各一次 | 預設重算；舊例外不搬移，新 subject 需例外時才新採納；term key 保留 |
| N15 | 普通 choice 改字；只改無關身分；source 原文換字各一次 | 依賴者重渲染／關聯仍有效／新 source_hash 回預設、仍需例外才另採納，不重配 term |
| N16 | 關聯用 delegated_glossary、approved_policy、wording 核可各一次 | 各自拒絕，例外不跨 kind 代簽 |
| N17 | glossary 加 card_id／改已入庫 record_hash／未知 kind 靜默忽略各一次 | 各自拒絕；新關聯不用改舊 39 分片 |
| N18 | 無例外／指派，同語言同字串對到兩個已採納概念；移除所有概念各一例 | 分別 ambiguous_name_concept／missing_name_concept，概念引用待件並計數；直接名字另驗自己的名字政策，不一律回原文 |
| N19 | 唯一概念 choice 與 owner 自己已驗同卡連結的官方譯名不同 | 純譯名字串差異按實看選詞／政策／必要真人供名／其他詞順位顯示勝出名並報差異；真語義歧義仍需指派，不任取概念 |
| N20 | 已採納同字異義例外，卡文勘誤換 revision ID，但名稱及永久卡／面不變 | 例外仍有效、不重簽；新 owner 的語義指派另驗，壞舊指派不沿用 |
| N21 | 缺 SVE／數位英文名的合成候選；模型提羅馬字 slug、協調者核可 | 可配永久 key，不因無現成英文名缺概念／譯名 |
| N22 | slug 含非 ASCII／首尾底線、key 超 96 bytes、跨分片撞既有 key 各一次 | 配發前全體檢查拒絕，改候選後重驗，不加流水號 |
| N23 | 指派的歷史 basis／名稱不變，只更新消費端 printing observation；另改名字及面修訂 ID 各一次 | 前者重驗後仍適用；後兩者舊指派仍可歷史核對，但不搬至新字串／owner，依翻譯契約 I06／I07 |

後續 #51／#196 的正式資料、#52／#53 的載入與選用、#37 的有效身分能力須各自驗收；
本文件不宣稱數位同卡證據、真實委託收據或 repaired registry 已可發布。
