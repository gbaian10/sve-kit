# 身分修復與決定續版

**使用者 2026-10-01 核可**（含指名撤回錯誤修復）。 本文件將既有永久身分與不可變決定原則具體化為
`identity-transition-v1` authored 契約；格式核可不表示已有實作或真實資料採納。
已定語意仍以 [build-db §3.1](build-db.md#31-永久身分)、[§13](build-db.md#13-官網更正與身分修復)
及 [authored-layout §2–§3](authored-layout.md#2-分片批次決定與來源) 為準。

## 1. 範圍與不變條件

- 永久 card／face／printing／art ID 不重新計算；`int_id→printing`、region 及既有 owner 不變。
  舊 record、分片與 decision 的 bytes 保留，不以重排 YAML、更新 index hash 或 Git 歷史代替原檔保存。
- `identity_change` 只表達既有 printing 的父 card 真正改變或既有 card 合併／拆分；首次登錄、
  來源改字、重新確認同一對應、新增再錄、confirmed_none／reskin 續版均不憑空建立修復事件。
- 來源版本、歷史核可與本次投影有效性分開。歷史 confirmed 不降級、不覆寫；新來源沒有受審，
  就不能冒用舊核可。現行表記、來源更正、勘誤與 DSL 分別重驗，不因身分確認而繼承。
- 本格式僅處理 registry 的 card、face、printing、art、region_mapping_review、card_related。
  不改配號格式，不提供商品內容／商品身分對照的續版，亦不替代表記採納或來源更正生命週期。
  商品誤配仍依 [authored-layout §11.3](authored-layout.md#113-首次採納重建匹配與改址) 停止受影響匯入。

## 2. 獨立入口與不可變封套

| 路徑（相對 authored 根） | 完整頂層欄位 |
| --- | --- |
| `identity-transitions/index.yaml` | `identity_transition_format: 1, kind: identity_transition_index, includes` |
| `identity-transitions/<sequence>.yaml` | `identity_transition_format: 1, kind: identity_transition_shard, default_decision_id, records, decisions` |

sequence 是從 1 開始、至少三位十進位數字的連續序號；每片恰一筆 transition 與一個 confirmed batch decision。
空入口為 includes={}。尚未接入此能力的舊工具不能忽略非空入口後仍宣稱 registry 可發布。
路徑安全、YAML 限制、canonical recipe 與單檔小於 1 MiB 沿 authored-layout §1、§2、§10.2；
禁止絕對路徑、`..`、symlink 逃逸、缺檔、未索引分片、重複鍵與未知欄位。
includes 釘解析後完整分片的 canonical hash，歷史 entries 只增不改；不納入 ids/index.yaml。

每筆 transition 恰有 `{record_key,kind,action,reverts,sequence,previous,registry_basis,review_context,updates,repairs,routes,evidence,reason}`：

| 欄位 | 完整定義 |
| --- | --- |
| record_key | `["identity_transition",sequence]` 的 canonical JSON 字串，全域唯一 |
| kind | 固定 `identity_transition` |
| action, reverts | action 為 `apply/revert`；apply 的 reverts=null，revert 指名 `{record_key,record_hash,decision_id}`，見 §4.2 |
| routes | 完整路由變動陣列，無變動為 []；見 §4.2，參與本次成員 hash |
| sequence | 正整數，決定套用順序，不表示官方年代或發布版號 |
| previous | 首筆 null；其餘為 `{record_key,record_hash,decision_id}`，恰指 sequence−1 的完整 transition 與決定 |
| registry_basis | `{authored_revision,index_path,index_hash}`；完整 Git commit SHA、固定 `ids/index.yaml`、其 canonical hash，釘核對時原始 registry |
| review_context | 沿 authored-layout §9.2 的 `{context,source_batches}`；完整 F1 程式、依賴、設定與凍結批次，釘前件及核對來源，不引用尚未產生的自身分片 |
| updates | 非空陣列，按 target_key 排序且唯一；元素見下節，完整新內容參與本次 record hash |
| repairs | 按 id 排序唯一的修復群組陣列；純來源更新／決定續版為 []，見 §4 |
| evidence | 沿 authored-layout §10.3 的非空 `{batch_id,source_version_id,locator,role}` 陣列，按 canonical bytes 排序唯一；全部批次列入 review_context |
| reason | 不含官方卡文、私人路徑的非空說明；公開事件另用各群組的公開 reason |

decision 完整欄位沿 authored-layout §10.4；category=`identity_transition`、policy_id=`identity-transition-v1`、
scope=`batch`、state=`confirmed`。members 恰為本片 transition 的 `[record_key,semantic_hash]`；
sample_ids 恰含該 key。核對者必須實際核對 updates 所涵蓋的全部新舊版次、全部面與證據；
外層只有一個成員不表示只看一筆 printing。草稿留在 authored 外，不先寫 proposed 再原地改 confirmed。

`H(x)` 指 authored-layout §2 的 canonical JSON SHA-256（含 `sha256:` 前綴）。
semantic_hash=H(完整 transition)，membership_hash=H(排序後 members)，decision.id=`d:` 加 membership hash 的完整 64 hex。
完整 after、前件 refs、移轉清單與 evidence 都在 hash 內；不是只 hash 永久 ID 或新增 printing 的差集。
沒有 record 或 decision 自我引用。新的核可必須是新 transition、新 members、新 decision；
不沿用舊決定冒充這次核對；決定不保存姓名／時間。

## 3. 完整替代內容與決定續版

每個 update 恰有 `{target_key,before,after,allocation_anchor}`。target_key 是原 registry record_key，
before 為 `{transition_key,record_key,record_hash,decision_id}` 或 null：

- transition_key=null 指原 registry；非空指產生該有效內容的前序 transition。
  record_key 必須等於 target_key；record_hash=H(那次完整 record)，decision_id 指實際核可該內容的封套。
  必須命中緊接本次套用前的有效版本，不能越過已存在的續版。
- before=null 只用於新增 card／face／art；其 ID、record_key 與配發 anchor 全域未使用。
  新 printing 與 int_id 仍走既有 registry 追加交易，再由本格式核對其對相關決定的影響。
  allocation_anchor 只在 before=null 時為非空、全域未用的人工配發字串，其餘一律 null。
- after 是完整 `{record_key,kind,owner,data}`，不接受局部 patch；record_key=target_key。
  null 表示明示停止選用，只允許 region_mapping_review／card_related，仍保留其歷史記錄與決定。
  card 退役使用完整 after 且 identity_state=retired，不刪永久實體。

after 沿 registry 各 kind 的欄位與證據限制；穩定主鍵、owner 不變。
既有 card 只允許更改 identity_state；face 的 card_id／ordinal／side 不改；printing 只允許
card_id／source_face_map（有對應 repair 或合法 revert 時）、observation／cross_region_review（重新完整核對時）改變；
art 只更新 uses／observation，不改其 card_id／face_id／classification。
review 的 card_id／target_region、relation 的 id／兩端／relation 不改；要改關係端點須明示停用舊關係、
按原 registry 首次採納流程另建新關係，不以續版偷換其永久身分。其他欄位變動超出本格式即拒絕。覆蓋層允許 card.identity_state=retired，
其餘 enum 不擴張。停用後若要再次採納同鍵，before 仍指最近 transition，record_hash=H(null)，
不能以 before=null 假裝首次登錄。每次套用只在建置記憶體中建立有效投影，絕不改寫原 registry。

**決定續版的完整集合**：

| 情境 | 本次必須核對的內容與結果 |
| --- | --- |
| confirmed_none 新增版次或來源版本 | 完整 after review，observations 包含該 card 的全部受審來源區版次及完整觀測；重新釘 target_region、as_of、coverage_scope、coverage_hash 與凍結目標區全集。既有無對應批准不延伸到新 printing |
| reskin 任一端新增版次或來源版本 | 保留 relation 永久 id／兩端，after.evidence 完整涵蓋兩端全部受審版次（含 role），不是只列新增者；兩端目前來源逐筆匹配後才投影適用地區 |
| 原判斷不再成立 | after=null 明示停用該 review／relation；若出現跨區正對應，再以實際 printing.card_id 與修復表達，不寫第二份映射真值 |
| printing 來源更新，父 card 不變 | after 保留 ID／owner／region／card_no／variant_key／面對應，換成受審新 observation；EN 目標觀測、art observation 及相關 review／relation 一併更新或明示停用；repairs=[] |
| raw／parser 改版，但 registry observation exact 不變 | 保存新來源版本與 F1 使用證據；只有完整受審觀測、核對範圍與既有決定仍匹配時才可沿用。wording 等以來源版本為鍵的決定仍各自重驗 |

review 的穩定 target_key 沿原 registry key，不以新的 as_of 另建獨立分支。
建置 region_mapping_review 歷史日期列仍留；同日續次審核由 authored 鏈與 decision 保存，
同一 DB 主鍵只投影最後有效內容，不能插兩筆碰撞或以 as_of 排序選勝者。
reskin 同理不產重複／反向關係。失效舊批准不自動恢復；新來源未核對時報待審，不 fallback 舊版通過。

本次 registry_basis 可以比前次新增原始登錄；須驗舊記錄與配號 bytes 未變、ID 無重用，
也須重驗新增 printing 對所有既有 confirmed_none／reskin 的影響。
讀取原始 registry 的結構／hash 驗證與有效集合驗證分兩階段；不得在套用續版前，
就以舊 review 未涵蓋新 printing 為由否決一筆其實完整的交易，亦不得跳過最後的完整集合檢查。

## 4. merge／split／reassign 的完整移轉

每個 repair 恰有 `{id,kind,old_card_id,new_card_ids,printing_moves,face_moves,art_moves,retire_old,reason}`。
id 是首次配發的永久 ID，不由目前排序重算；kind=`merge/split/reassign_printing`。
new_card_ids 排序唯一，不含 old_card_id。moves 陣列按各自完整 canonical bytes 排序唯一。

| kind | 精確條件 |
| --- | --- |
| merge | 恰一目的 card，移走 old 的全部有效 printing，retire_old=true；多個 old 合併各列一群組、同 transaction 原子套用 |
| split | 至少兩個目的 card，各分得非空 printing 集合；恰分割 old 的全部有效 printing，retire_old=true；若保留 old 作其中一組，改用逐 printing reassign，不以 old→old 假事件表示 |
| reassign_printing | 恰一目的 card、恰一 printing；retire_old 只在本 transaction 後 old 無有效 printing 時為 true，否則 false |

每個 printing_move 恰有 `{printing_id,from_card_id,to_card_id,faces}`，from 必須是修復前的 old，
to 屬 new_card_ids，from≠to。同 transaction 同 printing 只移一次；有效修復有向圖不得有環（撤回依 §4.2 移除原邊），
每個 old_card_id 在同 transaction 至多一個 repair 群組，避免各自計算 remaining_uses 互相衝突；
分次移走同 old 的數張 printing 使用連續 transaction，每次以前次結果核對。
retired ID 不重用；只有 §4.2 指名撤回可恢復被該修復退役的原 card，不是重新配發身分。

faces 恰覆蓋該 printing 全部來源面，每項為 `{source_index,from_face_id,to_face_id,from_art_id,to_art_id}`；
source_index 沿原 source_face_map，含雙面背面，不以 ordinal、名稱或圖片相似度猜。
兩個 art 欄必須明示（未知時 null）；不能把已知 art 任意丟成 null。
printing after.source_face_map 必須與此映射完全相同，after.card_id 必須等於 to。

**face／art ID 政策**：舊 face／art 保留原父層，跨 card 不搬動同一 face ID；
目的 card 使用其既有且人工確認對應的 face，或首次配發新 face。
舊 art 跨 face 時也使用目的面已有、已確認同圖的 art，否則配新 art ID；
不能讓一個 art ID 同時屬於兩個 face。這避免覆寫舊 face_revision 與圖像歷史的歸屬，
代價是同圖因身分修復可能有新 art ID；公開清單依下述現行用途篩選，不重複展示歷史圖。

- face_moves 每項恰為 `{from_face_id,to_face_ids}`；逐 old face（含無 printing use 的面）列一次，
  to_face_ids 為非空排序唯一目的面。與 printing moves 一致；split 可一對多，merge 可多對一。
  沒有 use 的映射仍須證據。原卡保留部分 printing 時舊 face 繼續供它們使用，移轉清單只描述目的面。
- art_moves 每項恰為 `{from_art_id,targets,remaining_uses}`；逐 old card 的所有 art（含未使用候選）列一次。
  targets 元素恰為 `{to_art_id,to_face_id,uses}`，uses 為排序唯一 `{printing_id,face_id}` 陣列；
  remaining_uses 與全部 targets.uses 恰分割修復前 uses（目的 face 以 faces 映射反算比對）。
  無 use 的 art 明列 targets=[]、remaining_uses=[]，原列保留；沒有 art 時整體 art_moves=[]。
- 所有改變 use 集合的 art 都要有完整 after（舊 art 留 remaining_uses，目的 art 保留原有 uses 加新 uses）；
  非空 uses 的 observation 必須指該 art 的一個實際 use。歷史 art 的 uses=[] 是本格式容許，
  僅限移轉後無現行 use 的保留列，其 observation 留歷史證據，不拿來通過現行來源檢查。
- 新 card／face／art 必須隨 updates 完整建立。為避免拆分時原代表 printing 的 anchor 已被舊 card 佔用，
  新實體以既有固定 namespace、UUIDv5 名稱 `kind + NUL + "identity-transition-v1" + NUL + allocation_anchor`
  首次配發，保留 anchor 且檢查全域碰撞；只有修復新增實體使用這個 recipe，不改任何舊 ID。
  存活 card 仍驗 single／double_faced 數量、side、ordinal 唯一；墓碑亦保留原有完整 faces。

完整移轉不限於 printing FK：printing_face、art uses、圖像／標誌、printed observation 等以具體版次來源
重建至新面；face_revision、current、表記採納、規則同名、baseline、語義、翻譯、DSL 與換皮等
不可只換 face_id 繼承核可。舊歷史留原面，新面缺 fresh 採納時依該能力顯示未知／未實作，
不能造 confirmed 或已驗 DSL。來源更正若仍綁舊 face，須由其獨立契約取得新有效採納，否則阻止受影響結果冒充正確。
建置逐一列出所有實際啟用的面／插畫依賴之重建、保留或失效結果，漏引用為錯誤。

墓碑保留 card.id、owner、layout、原 faces，identity_state=retired；不帶有效 printing／current／自動能力，
原採納與歷史 revisions 可追溯。原 card 未退役時只移指定版次，不能把其餘版次或 defaults 一併丟掉。
公開墓碑 card 與原 faces 只保留修復事件／歷史引用閉包，不列入一般卡表、搜尋、卡包、
插畫或繪師瀏覽；support 仍保留 required 列並標未實作，不帶自動能力。
無現行 printing_face use 的舊 art 不進公開 art 清單，其 art_artist 不投影成公開 art.artists；僅因這些 art 被引用的 artist 亦不出貨；
仍被其他現行 art 引用的 artist 保留。公開 baseline／巢狀引用不得指向被排除的 art，
依欄位契約留 null 或移除相應非必填列，不可留下懸空 FK。保留窗口內舊快照的 JSON 閉包不改寫，輪替後可回收；圖片只留 current。

### 4.1 三種修復的合成對照

下表符號代表已登錄實體，並非真卡資料；每格的面／插畫清單都須完整展開為上述物件。

| 情境 | 修復前 | 必要 updates／moves | 結果 |
| --- | --- | --- | --- |
| merge A→B | A 有 P1/F1/X1，B 有 P2/F2/X2 | P1 改 B/F2；F1→F2；X1 的 P1 use 改到新 X3/F2，X1 uses=[]；X3 新列；A 改 retired | P1、P2 與各自 int_id 不變；A/F1/X1 留歷史；B 保留 P2 的原有圖 |
| split A→B,C | A 有 P1、P2，共 F1/X1 | P1→B/F2/X2，P2→C/F3/X3；F1→[F2,F3]；X1 全部 uses 分別列入 X2/X3；新增 B/C/F2/F3/X2/X3；A retired | 兩個 split 公開事件，玩家選目的，不自動改牌組 |
| reassign P1 A→B | A 有雙面 P1、P2；B 已有 F3/F4 | P1 前／背 F1/F2→F3/F4，兩面 art 的移出與留下 uses 均列；P2 留 A；retire_old=false | P1 父層改 B；A 與 P2 不退役；P1 URL、int_id 不變 |

同圖同框不能省 art 移轉；雙面不能只移正面。純來源 effect 改字而父 card 未變，
只續版觀測與受影響決定，禁止用空 moves 的 reassign 混入公開修復。

### 4.2 指名撤回修復

**使用者 2026-10-01 核可**：新增 action=revert，指名撤回上一筆或較早的修復 transition。
reverts 恰有 `{record_key,record_hash,decision_id}`，精確釘住已存在、action=apply 且 repairs 非空的整筆
transition；不只指 repair.id、日期或最新一筆。不允許部分撤回一個 transaction、重複撤回、指向未來、
撤回純來源續版，或 revert 指向 revert。若撤回本身判錯，另以新的 apply 重新採納修復，不能復用原事件 ID。

revert 仍在全域 sequence 鏈尾追加，previous 指當前鏈尾（不一定是 reverts）；它本身須有新的 confirmed
決定、完整 updates／evidence／review_context／routes，repairs=[]。舊 transition、record、decision 及已發布
事件 bytes 完全不改。reverts 的三個欄位一併參與 record hash；來源與人工核對不是用舊 reviewer 代簽。

**指定較早修復的條件**：重建目標套用前後的完整狀態，並檢查所有後續有效 transition 及 registry 追加。
目標的每個 update key 必須仍有效指向目標產生的版本；不得有後續改寫、以其產物為證據的採納，或
新增 printing／use／路由依賴受影響 card／face／art。檢查範圍包含完整移轉與其啟用能力的依賴閉包，
不只看 key 是否相撞；previous 的順序引用及 F1 對全 registry 的釘選本身不算語義相依。
後續若已合法撤回且狀態 exact 還原，允許以該撤回產生的最新 before ref 繼續驗證；不能跳過任何歷史。
無關卡片的後續採納可以保留。存在相依修復時，先按相依的反向順序逐筆撤回；有其他無法撤回的採納
則拒絕整筆撤回、列精確衝突，不能強制覆蓋。這是避免破壞後續決定的驗證，不限制只能撤回鏈尾。

updates 恰覆蓋目標的全部 update keys：每個 before 指目前有效版本，after 由已驗證的目標前態反算，
不能任意補修其他問題。原已存在實體恢復完整前態；原 printing 回到原 card、全部原 faces／art，
原 card 若被目標退役，恢復其原 identity_state。這是 retired 恢復的唯一例外，不重配 ID。
目標新建的永久實體不刪除：新 card 改 retired、新 face 留原父層、新 art 留 uses=[] 作歷史，
allocation_anchor=null（不是再次配發）。曾被停用的 review／relation 若恢復，仍以本次決定重驗
完整觀測與目前 coverage，不自動繼承舊批准；來源已變而無法重現合法前態即拒絕，不能盲目倒退 current。
DSL、表記與來源更正依其本身 freshness 閘門，不因撤回而恢復已失效能力。

**路由一併還原**：apply／revert 的 routes 都是排序唯一的 `{printing_id,before,after}` 陣列，
恰列本次路由投影實際改變的 printing。before／after 各為 `{canonical,aliases}`；canonical 是
`{namespace,route_key}`，aliases 是排序唯一的 `{namespace,route_key,reason}` 陣列，全部直指該 canonical。
namespace 與 reason 沿 build-db §15。這是已釘住路由規則與來源推導出的完整預期投影，
不是任意 override 或更改 printing.card_no 的入口；前後狀態及其來源／已採納 route override 必須可重建。

撤回恢復目標前的路由解析結果與必要的原 canonical；移除目標引入的有效轉址，再展平所有舊入口。
已公開的新入口不能刪：若撤回使其不再 canonical，保留為同 printing 的永久 alias，reason 沿既有
`merged`（見 §15）；不能把不再為 canonical 誤當刪除 URL 的許可。新增 alias 與 canonical 互斥，
原 provisional 入口亦永久保留。路由未改的身分修復，apply／revert 都 routes=[]。
獨立發生的後續改號不倒退，若與恢復衝突則拒絕並列出相依；不能挪用其他 printing 的舊 URL。

**兩張有效圖與歷史分開**：按 sequence 套用 apply 的每一修復邊，以其事件 ID 區分相同端點；
revert 只從有效集合移除被指名 transaction 的全部邊，不新增 B→A 反向修復邊。
其他事件的同端點邊不能一併刪掉。每步仍驗有效修復圖無環，撤回後的 card_route_alias 圖也須無環、
無 alias chain 且每個入口指原 printing。歷史事件含撤回紀錄而不參與有效圖的環檢查，
不能為通過無環而刪歷史，亦不能把任何反向 apply 冒稱合法撤回。

## 5. 有效投影順序與原子性

1. 釘住本次 authored revision、ids 與 transitions 兩個完整入口。先驗全區所有 index／分片 hash、
   原 registry 決定、配號及歷史 bytes；不是先濾 JP 再重算 members。
2. 依 sequence 重建每次 registry_basis 及之前 transition 的有效狀態。basis 之間只容許原始登錄追加；
   驗 previous、全部 before refs 命中當時有效版本，禁止缺號、跳號、分叉、循環與過期前件。
   後來新增的 printing 不能倒灌進歷史決定的 checked 集合。
3. 從該次 review_context 重建原始觀測、target coverage 與 evidence，核對新決定完整集合。
   在暫存投影同時套用該 transaction 的所有 updates／moves；不逐列提交會短暫違反 FK 的半套修復。
   apply 的 repairs、revert 的精確前件與反算結果必須恰好解釋全部父 card 變更、退役／恢復與路由變動；
   無多餘／遺漏事件，未列的永久欄位變更拒絕。
4. 每次驗 face／art 閉包、uses 分割、card layout、confirmed_none／reskin、跨區全部面與唯一性。
   撤回先停用目標修復邊再驗有效圖無環；純來源更新還須確認 card 關係未變；不能透過 after 偷改父層。任何失敗回滾整個候選建置。
5. 將歷史鏈套到本次完整 registry，重新比對本次凍結來源與新增版次，計算 freshness；
   歷史成功不代表本次仍適用。來源缺失、hash 不符與非法前件是輸入錯誤；新增未核對觀測列待審，
   不以舊 confirmed 投影受影響關係。再套各能力的來源更正／表記／勘誤政策。
6. 最後才做 JP／EN 地區投影、DB FK／完整性、公開引用閉包與發布閘門。決定與移轉清單留建置端，
   公開僅依 §6 的白名單（含撤回引用）；無效閉包不可靠地區過濾藏起來。

沿 F1 保存本次兩個入口的 `{authored_revision,index_path,index_hash}` 與全部檔案 exact bytes hashes。
每片有 authored source_record（parser_version=`identity-transition-v1`），decision_source 連完整封套與
所有 raw evidence；歷史前件亦納輸入閉包，不只保留最終 after。這是可重建輸入，無須新增泛型 subject DB 表。

寫入沿 registry 全域鎖，採固定順序而不設恢復紀錄：先驗完整計畫，持久寫入所有新 registry／配號
與 transition 分片，再原子替換 ids/index.yaml，最後原子替換 transitions/index.yaml；各步成功才做下一步。
兩次 rename 不是跨檔原子交易，讀寫共用鎖；讀取端驗全部分片納入索引、registry_basis 與有效全集合。
中斷留下未索引分片、hash／基準不符、或新增版次尚未被 confirmed_none／reskin 涵蓋，立即失敗，
不使用半套資料。即使只改 transitions，也先持久寫新分片再替換其 index。

所有分片先落地，因此前一入口已換、後一入口未換時，未索引 transition 分片即能攔截，
不只依賴新版次剛好涉及 confirmed_none／reskin。失敗後依原完整計畫與 Git／備份驗 exact hashes，
補齊缺失寫入並依同一順序完成入口；同內容已存在可重用，異內容即停。
不得刪未索引檔後重新猜游標，不回退或重用已配號證據。
完整但尚未有 transition 分片的新增版次若違反全集合審核，亦由既有閘門拒絕。
乾淨 checkout 以同一 Git commit 的完整兩入口重建；未實作前既有追加拒絕行為不放寬。

## 6. 公開事件、墓碑與路由

以下永久相容原則沿既有規格；本節補明映射方式與已核可的撤回例外：

- repair 群組在 DB 展開：merge 一列，split 每個目的各一列，reassign 一列且 printing_id 必填；
  其他種類 printing_id=null。事件 ID 以既有 registry 固定 namespace 的 UUIDv5、canonical `[repair.id,new_card_id]`
  首次配發並查碰撞，後續不重算／重用。公開 reason 不能含決定、私密證據或本機路徑。
- revert 在 DB／公開 identity_change 對目標的每一事件各新增一列 kind=revert、reverts_id 指該事件；
  old_card_id／new_card_id／printing_id **沿被撤回事件原方向原值**，表示撤回哪一條邊，不是反向移卡指令。
  事件 ID 以相同固定 namespace、canonical `["identity-revert-v1",transition.record_key,reverts_id]` 的 UUIDv5 配發。
  apply 事件的 reverts_id=null；revert 必須指先前非 revert 事件，全域唯一且 confirmed，整組事件原子撤回。
  公開不出私有 transition key／decision，只出公開事件間的 reverts_id；consumer 先解析撤回再計有效圖。
- data_version 是該事件**首次正式發布**版號。候選建置以目標版號暫填；發布流程以精簡的事件 ID→首次 data_version／manifest hash 收據固定，索引提交成功後確認收據；中斷以提交結果恢復，不把預留當已發布。
  收據耐久保存並備份，後續重建驗它並沿用，不靠永久 CDN 索引／快照，不每版改時間；preview 不登錄首次正式發布。
  無法取得歷史發布證據時拒絕發布，不把舊事件當首次發布。
  同事件 ID 已公開但內容不同即拒絕。公開修復歷史與舊墓碑保留，changes 只列本次新增事件。
- 卡片網址綁 printing；合併 card 不代表把該版次 URL 轉到另一個 printing。原卡號沒變就不新增 alias。
  確實改號時依 build-db §15 永久保留舊入口、展平 alias 到同 printing 的最新 canonical，
  禁止鏈／環、精確撞號、搶走舊入口與 provisional override。
- 舊分享碼保留 int_id、數量、區域、位置。split 必須讓玩家選擇，不能靜默換 printing／刪行；
  已知 int_id 仍解析原 printing 並呈現修復提示。目前不承諾歷史對局回放或舊卡表重新下載；
  保留窗口內 JSON 不回寫新父 card／面／文字，公開保留依 snapshot-format §4.1，不影響身分事件／採納鏈。

公開形狀新增 kind=revert 與 required nullable reverts_id。format `1.0.0` 仍為候選時，依
[機器契約的候選期規則](snapshot-contract.md) 在候選內同步修訂 Schema、欄序、golden 與 reader，
不要求額外升版；正式凍結後才至少升 minor、加入 `identity-revert-v1` required capability，
並提高 min_reader_version。尚未完成 reader／匯出器相容實作時不得發布含此格式的快照。

## 7. 完整封套合成例子與 hash 驗算

以下 Python 產生完整 **confirmed_none 決定續版**封套與 index（JSON 也是 YAML 1.2 合法輸入），
不讀資料目錄、不含官方卡文。輸出就是 已核可格式的完整形狀，不是只列欄位差異。
`0/1/2` 等重複 hex、example store、核對者及依賴 hash 都是合成 pin，不能作真實採納；
context 仍遵守 source-archive §2.2.1：dependencies 非空，configuration 是 canonical JSON 字串。
真實輸入必須能重建 context／raw 閉包。舊 registry 已另追加第二張 printing，此次重新核對兩張全部觀測。

```python
import copy
import hashlib
import json


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":")).encode("utf-8")


def digest(value):
    return "sha256:" + hashlib.sha256(canonical(value)).hexdigest()


def observation(number, digit):
    return {"region": "en", "card_no": number,
            "recipe": "registry-observation-v1",
            "observation_hash": "sha256:" + digit * 64,
            "rules_hash": "sha256:" + "3" * 64}


card = "c:" + "a" * 32
key = "region_mapping_review:" + card
old = {"record_key": key, "kind": "region_mapping_review", "owner": "EXAMPLE",
       "data": {"card_id": card, "target_region": "jp", "state": "confirmed_none",
                "as_of": "2026-09-28", "coverage_scope": "jp:all",
                "coverage_hash": "sha256:" + "4" * 64,
                "observations": [observation("EXAMPLE-001EN", "1")]}}
new = copy.deepcopy(old)
new["data"]["as_of"] = "2026-10-01"
new["data"]["coverage_hash"] = "sha256:" + "5" * 64
new["data"]["observations"].append(observation("EXAMPLE-002EN", "2"))
old_members = [[key, digest(old)]]
old_decision = "d:" + digest(old_members).removeprefix("sha256:")
basis = {"authored_revision": "0" * 40, "index_path": "ids/index.yaml",
         "index_hash": "sha256:" + "6" * 64}
evidence = [{"batch_id": "sha256:" + "7" * 64,
             "source_version_id": "src:v1:" + digit * 64,
             "locator": locator, "role": role}
            for digit, locator, role in [
                ("8", "EXAMPLE-001EN", "identity_observation"),
                ("9", "EXAMPLE-002EN", "identity_observation"),
                ("a", "jp:all", "mapping_coverage")]]
evidence.sort(key=canonical)
record = {
    "record_key": '["identity_transition",1]', "kind": "identity_transition",
    "action": "apply", "reverts": None, "routes": [],
    "sequence": 1, "previous": None, "registry_basis": basis,
    "review_context": {
        "context": {"program_revision": "b" * 40,
                    "dependencies": [{"name": "carddb/uv.lock", "sha256": "sha256:" + "c" * 64}],
                    "configuration": canonical({"registry": basis,
                        "observation_recipe": "registry-observation-v1"}).decode("utf-8")},
        "source_batches": [{"batch_id": "sha256:" + "7" * 64}]},
    "updates": [{"target_key": key,
                 "before": {"transition_key": None, "record_key": key,
                            "record_hash": digest(old), "decision_id": old_decision},
                 "after": new, "allocation_anchor": None}],
    "repairs": [], "evidence": evidence, "reason": "合成例：新增版次後完整重審無對應",
}
members = [[record["record_key"], digest(record)]]
membership_hash = digest(members)
decision = {
    "id": "d:" + membership_hash.removeprefix("sha256:"),
    "state": "confirmed", "scope": "batch", "category": "identity_transition",
    "policy_id": "identity-transition-v1", "membership_hash": membership_hash,
    "members": members, "sample_ids": [record["record_key"]],
    "note": "合成例，非真實核可",
}
shard = {"identity_transition_format": 1, "kind": "identity_transition_shard",
         "default_decision_id": decision["id"], "records": [record], "decisions": [decision]}
index = {"identity_transition_format": 1, "kind": "identity_transition_index",
         "includes": {"identity-transitions/001.yaml": digest(shard)}}
assert digest(old) != digest(new)
assert decision["id"] != old_decision
print(json.dumps({"old_record": old, "shard": shard, "index": index},
                 ensure_ascii=False, indent=2))
```

可重算的完整 hash（與上述程式輸出一致）：

| 輸入 | SHA-256（省略 `sha256:`） |
| --- | --- |
| old record | `eb2e777e37086e85161fdcf71f90ce6ae30a2705c283f9daa6b64fb63f33d10e` |
| new record | `b5e9034517d9fd97a48fa3c44482c91dc8c761aa730a276dd70cddfb24d21995` |
| 完整 transition 成員 | `99eff6ebf528bbda88624461f10f7920f686013901247bcd389f6146b993eb57` |
| membership（亦為 decision.id 的 `d:` 後綴） | `5f98b2b63b67f2c7ce756d31883ac1813b9d1cab6c5189027f4870890001af35` |
| 完整 shard（index.includes 的值） | `583a9737db97404eef0fa42cb90bccad6cc1574efd1f248bf4169e515715208b` |

此例的 old_record 是重建測試前件，不寫入新分片；舊決定與原 registry 仍需存在於被釘住的輸入。
下一次新增版次時 sequence=2，previous 釘第一筆 record hash／decision，update.before 指第一筆
transition_key、H(new) 與其 decision；after 再列**全部**觀測。不把第一次決定的 members 改成新集合。
reskin 續版用同封套、after 換完整 card_related record，evidence 同時釘 from／to 兩端全部版次；
即使新增版次與舊版文字相同也需要新核可。

### 7.1 修復後指名撤回的合成封套

接續上面程式的 canonical／digest 與合成 context（同樣不能作真實來源證據）。這是獨立的兩筆鏈：
A 原有 P1，B 原有另一 printing，F_A／F_B 是各自既有單面，兩張都沒有已分組 art。
第一筆錯誤 merge 將 P1 掛到 B 並退役 A；第二筆指名撤回第一筆，恢復 A／P1／F_A。
沒有改卡號或路由，故 routes=[]，撤回後網址仍解析同一 P1；沒有配發或刪除永久 ID。

```python
A, B = "c:" + "a" * 32, "c:" + "b" * 32
FA, FB, P = "f:" + "a" * 32, "f:" + "b" * 32, "p:" + "1" * 32
root_card = {"record_key": "card:" + A, "kind": "card", "owner": "EXAMPLE",
             "data": {"id": A, "layout": "single", "identity_state": "confirmed",
                      "home_set_id": "EXAMPLE"}}
root_printing = {
    "record_key": "printing:" + P, "kind": "printing", "owner": "EXAMPLE",
    "data": {"id": P, "card_id": A, "region": "jp", "card_no": "EXAMPLE-001",
             "variant_key": "standard", "home_set_id": "EXAMPLE",
             "source_face_map": [{"source_index": 0, "face_id": FA}],
             "observation": {**observation("EXAMPLE-001", "1"), "region": "jp"}},
}
roots = [root_card, root_printing]
root_members = [[r["record_key"], digest(r)] for r in roots]
root_decision = "d:" + digest(root_members).removeprefix("sha256:")
root_bytes = canonical(roots)
moved = copy.deepcopy(roots)
moved[0]["data"]["identity_state"] = "retired"
moved[1]["data"]["card_id"] = B
moved[1]["data"]["source_face_map"][0]["face_id"] = FB


def ref(record, decision_id):
    return {"record_key": record["record_key"], "record_hash": digest(record),
            "decision_id": decision_id}


def pack(record):
    members = [[record["record_key"], digest(record)]]
    d = {**decision, "members": members, "membership_hash": digest(members),
         "id": "d:" + digest(members).removeprefix("sha256:"),
         "sample_ids": [record["record_key"]]}
    return {"identity_transition_format": 1, "kind": "identity_transition_shard",
            "default_decision_id": d["id"], "records": [record], "decisions": [d]}


apply = {**copy.deepcopy(record), "updates": [
    {"target_key": before["record_key"],
     "before": {"transition_key": None, **ref(before, root_decision)},
     "after": after, "allocation_anchor": None}
    for before, after in zip(roots, moved, strict=True)],
    "repairs": [{"id": "repair:example-merge", "kind": "merge", "old_card_id": A,
                 "new_card_ids": [B], "printing_moves": [
                     {"printing_id": P, "from_card_id": A, "to_card_id": B,
                      "faces": [{"source_index": 0, "from_face_id": FA,
                                 "to_face_id": FB, "from_art_id": None, "to_art_id": None}]}],
                 "face_moves": [{"from_face_id": FA, "to_face_ids": [FB]}],
                 "art_moves": [], "retire_old": True, "reason": "合成例：錯誤合併"}],
    "reason": "合成例：錯誤合併"}
apply_shard = pack(apply)
apply_id = apply_shard["default_decision_id"]
apply_ref = ref(apply, apply_id)
revert = {**copy.deepcopy(apply), "record_key": '["identity_transition",2]',
          "sequence": 2, "action": "revert", "previous": apply_ref,
          "reverts": apply_ref, "repairs": [], "reason": "合成例：指名撤回錯誤合併",
          "updates": [
              {"target_key": after["record_key"],
               "before": {"transition_key": apply["record_key"], **ref(after, apply_id)},
               "after": before, "allocation_anchor": None}
              for before, after in zip(roots, moved, strict=True)]}
revert_shard = pack(revert)
revert_index = {"identity_transition_format": 1, "kind": "identity_transition_index",
                "includes": {"identity-transitions/001.yaml": digest(apply_shard),
                             "identity-transitions/002.yaml": digest(revert_shard)}}
assert canonical(roots) == root_bytes
assert [u["after"] for u in revert["updates"]] == roots
assert apply_id != revert_shard["default_decision_id"]
active_edges = {"repair:example-merge": (A, B)}
active_edges.pop(apply["repairs"][0]["id"])
assert active_edges == {}
print(json.dumps({"apply": apply_shard, "revert": revert_shard, "index": revert_index},
                 ensure_ascii=False, indent=2))
```

上述 context 的 evidence locator 須在真實輸入改為對應 JP 的精確觀測／修復證據，並更新全部 hash；
這裡沿用合成 pin 只驗封套結構、hash 與狀態反算，並非可通過來源閘門的 fixture。

| 輸入 | SHA-256（省略 `sha256:`） |
| --- | --- |
| apply record | `c9a7621eb744aea49879a1adcdad34604c2b59402d7d320982b4e3e7d54568f8` |
| apply shard | `f78e35625b10d522fe9c794f365e8dba8480f82b83ad9edbdb254b07eacd29b3` |
| revert record | `46d0732fb4ab9b6cf1235845e4cfc309fc2337d0b39055e364831b91ef22c40b` |
| revert membership（新 decision.id 後綴） | `66d78dfb50a9da9720b92215b75f49c73fa0b1c3b9bc89cbca1874c716df5b26` |
| revert shard | `3d559ba13da6714bc0d05df51b86f631f70a342e0759732cbd8df3c0e0e1f70b` |

路由有變動時的合成預期另列如下；O／N 均代表同一 printing 的已驗證合法入口，實作測試須提供
相應已採納路由證據，不能靠此示意修改 card_no。每列都以完整 routes.before／after 參與成員 hash。

| 狀態 | canonical | alias 與解析 |
| --- | --- | --- |
| 修復前 | O | 舊入口 K→O |
| 錯誤修復後 | N | O→N、K→N |
| 撤回後 | O | K→O；保留曾公開的 N→O；O 不再是 alias，不能留下 O→N→O |

## 8. 契約驗收

| 正例 | 必拒絕或隔離的反例 |
| --- | --- |
| 乾淨 checkout 依 pins 重建同一有效投影 | 靠現有 dist、latest raw、檔名字典序或最新日期選贏家 |
| 舊 bytes／IDs 不變，續版有新 hash／decision | 改舊 record、刪舊分片、只更新 index hash、重用 int_id、外層核對掩蓋漏看成員 |
| merge／split／雙面 reassign 完整列面與所有 art uses | 漏背面、漏 art、丟 null、重複移 printing、跨 face 共用 art、墓碑 FK 懸空 |
| after 與 moves 恰好一致，修復有 confirmed | 父 card 偷改、空修復、proposed／sampled 修復、自指與環 |
| 新版次經全集合重審後 confirmed_none／reskin 可投影 | 只簽新增差集、target coverage 缺失、任一端過期、新版次自動繼承批准 |
| 同父 card 的來源更新沒有 identity_change | 來源改字就建 split、以修復冒充等義、舊 correction／DSL 無條件搬到新面 |
| 連續改號後所有舊入口仍解析同 printing | alias 指另一 printing、鏈／環、canonical 搶舊路由、split 靜默改牌組 |
| 指名較早且無相依的修復可撤回；原邊移除，原卡／面／圖恢復 | 循環反向 apply、目標 hash 錯、重複／部分撤回、後續相依被覆蓋、撤回舊核可自動變 fresh |
| 無現行 use 的 art 留建置歷史；墓碑只供修復引用 | 歷史圖重複進插畫／繪師瀏覽、一般卡表列出墓碑、公開懸空 baseline |
| 全交易通過才發布，重跑不追加也不改寫 | 更新一個 index 後另一個失敗仍讀半套、發布後改事件 data_version、改舊快照 |

本文件記錄已核可的 docs 契約；匯入器、中斷拒讀／續寫、CLI、DB／快照與上述正反例測試須另行實作。
