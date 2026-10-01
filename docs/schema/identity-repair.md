# 身分修復與決定續版

**提案，待使用者決定。** 本文件將既有永久身分與不可變決定原則具體化為
`identity-transition-v1` authored 契約；格式尚未核可，也不表示已有實作或真實資料採納。
已定語意仍以 [build-db §3.1](build-db.md#31-永久身分)、[§13](build-db.md#13-官網更正與身分修復)
及 [authored-layout §2–§3](authored-layout.md#2-分片批次決定與來源) 為準。

## 1. 範圍與不變條件

- 永久 card／face／printing／art ID 不重新計算；`int_id→printing`、allocated_at、region 及既有 owner 不變。
  舊 record、分片與 decision 的 bytes 保留，不以重排 YAML、更新 index hash 或 Git 歷史代替原檔保存。
- `identity_change` 只表達既有 printing 的父 card 真正改變或既有 card 合併／拆分；首次登錄、
  來源改字、重新確認同一對應、新增再錄、confirmed_none／reskin 續版均不憑空建立修復事件。
- 來源版本、歷史核可與本次投影有效性分開。歷史 confirmed 不降級、不覆寫；新來源沒有受審，
  就不能冒用舊核可。現行表記、來源更正、勘誤與 DSL 分別重驗，不因身分確認而繼承。
- 本格式僅處理 registry 的 card、face、printing、art、region_mapping_review、card_related。
  不改配號格式，不提供商品內容／商品身分對照的續版，亦不替代表記採納或來源更正生命週期。
  商品誤配仍依 [authored-layout §11.3](authored-layout.md#113-首次採納重建匹配與改址) 停止受影響匯入。

## 2. 獨立入口與不可變封套（提案）

| 路徑（相對 authored 根） | 完整頂層欄位 |
| --- | --- |
| `identity-transitions/index.yaml` | `identity_transition_format: 1, kind: identity_transition_index, includes` |
| `identity-transitions/<sequence>.yaml` | `identity_transition_format: 1, kind: identity_transition_shard, default_decision_id, records, decisions` |

sequence 是從 1 開始、至少三位十進位數字的連續序號；每片恰一筆 transition 與一個 confirmed batch decision。
空入口為 includes={}。尚未接入此能力的舊工具不能忽略非空入口後仍宣稱 registry 可發布。
路徑安全、YAML 限制、canonical recipe 與單檔小於 1 MiB 沿 authored-layout §1、§2、§10.2；
禁止絕對路徑、`..`、symlink 逃逸、缺檔、未索引分片、重複鍵與未知欄位。
includes 釘解析後完整分片的 canonical hash，歷史 entries 只增不改；不納入 ids/index.yaml。

每筆 transition 恰有 `{record_key,kind,sequence,previous,registry_basis,review_context,updates,repairs,evidence,reason}`：

| 欄位 | 完整定義 |
| --- | --- |
| record_key | `["identity_transition",sequence]` 的 canonical JSON 字串，全域唯一 |
| kind | 固定 `identity_transition` |
| sequence | 正整數，決定套用順序，不表示官方年代或發布版號 |
| previous | 首筆 null；其餘為 `{record_key,record_hash,decision_id}`，恰指 sequence−1 的完整 transition 與決定 |
| registry_basis | `{authored_revision,index_path,index_hash}`；完整 Git commit SHA、固定 `ids/index.yaml`、其 canonical hash，釘核對時原始 registry |
| review_context | 沿 authored-layout §9.2 的 `{context,source_batches}`；完整 F1 程式、依賴、設定與凍結批次，釘前件及核對來源，不引用尚未產生的自身分片 |
| updates | 非空陣列，按 target_key 排序且唯一；元素見下節，完整新內容參與本次 record hash |
| repairs | 按 id 排序唯一的修復群組陣列；純來源更新／決定續版為 []，見 §4 |
| evidence | 沿 authored-layout §10.3 的非空 `{store_id,batch_id,source_version_id,locator,role}` 陣列，按 canonical bytes 排序唯一；全部批次列入 review_context |
| reason | 不含官方卡文、私人路徑的非空說明；公開事件另用各群組的公開 reason |

decision 完整欄位沿 authored-layout §10.4；category=`identity_transition`、policy_id=`identity-transition-v1`、
scope=`batch`、state=`confirmed`。members 恰為本片 transition 的 `[record_key,semantic_hash]`；
sample_ids 恰含該 key。核對者必須實際核對 updates 所涵蓋的全部新舊版次、全部面與證據；
外層只有一個成員不表示只看一筆 printing。草稿留在 authored 外，不先寫 proposed 再原地改 confirmed。

`H(x)` 指 authored-layout §2 的 canonical JSON SHA-256（含 `sha256:` 前綴）。
semantic_hash=H(完整 transition)，membership_hash=H(排序後 members)，decision.id=`d:` 加 membership hash 的完整 64 hex。
完整 after、前件 refs、移轉清單與 evidence 都在 hash 內；不是只 hash 永久 ID 或新增 printing 的差集。
沒有 record 或 decision 自我引用。新的核可必須是新 transition、新 members、新 decision；
不沿用舊 reviewed_by／時間冒充這次核對。

## 3. 完整替代內容與決定續版（提案）

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
card_id／source_face_map（有對應 repair 時）、observation／cross_region_review（重新完整核對時）改變；
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

## 4. merge／split／reassign 的完整移轉（提案）

每個 repair 恰有 `{id,kind,old_card_id,new_card_ids,printing_moves,face_moves,art_moves,retire_old,reason}`。
id 是首次配發的永久 ID，不由目前排序重算；kind=`merge/split/reassign_printing`。
new_card_ids 排序唯一，不含 old_card_id。moves 陣列按各自完整 canonical bytes 排序唯一。

| kind | 精確條件 |
| --- | --- |
| merge | 恰一目的 card，移走 old 的全部有效 printing，retire_old=true；多個 old 合併各列一群組、同 transaction 原子套用 |
| split | 至少兩個目的 card，各分得非空 printing 集合；恰分割 old 的全部有效 printing，retire_old=true；若保留 old 作其中一組，改用逐 printing reassign，不以 old→old 假事件表示 |
| reassign_printing | 恰一目的 card、恰一 printing；retire_old 只在本 transaction 後 old 無有效 printing 時為 true，否則 false |

每個 printing_move 恰有 `{printing_id,from_card_id,to_card_id,faces}`，from 必須是修復前的 old，
to 屬 new_card_ids，from≠to。同 transaction 同 printing 只移一次；全域事件有向圖不得有環，
每個 old_card_id 在同 transaction 至多一個 repair 群組，避免各自計算 remaining_uses 互相衝突；
分次移走同 old 的數張 printing 使用連續 transaction，每次以前次結果核對。
retired ID 不復活或重用。回復曾經錯誤的修復若將成環，隔離並另提政策，不自動繞過無環規則。

faces 恰覆蓋該 printing 全部來源面，每項為 `{source_index,from_face_id,to_face_id,from_art_id,to_art_id}`；
source_index 沿原 source_face_map，含雙面背面，不以 ordinal、名稱或圖片相似度猜。
兩個 art 欄必須明示（未知時 null）；不能把已知 art 任意丟成 null。
printing after.source_face_map 必須與此映射完全相同，after.card_id 必須等於 to。

**face／art ID 政策提案**：舊 face／art 保留原父層，跨 card 不搬動同一 face ID；
目的 card 使用其既有且人工確認對應的 face，或首次配發新 face。
舊 art 跨 face 時也使用目的面已有、已確認同圖的 art，否則配新 art ID；
不能讓一個 art ID 同時屬於兩個 face。這避免覆寫舊 face_revision 與圖像歷史的歸屬，
代價是同圖因身分修復可能有新 art ID；它是需核可的取捨，不是已定政策。

- face_moves 每項恰為 `{from_face_id,to_face_ids}`；逐 old face（含無 printing use 的面）列一次，
  to_face_ids 為非空排序唯一目的面。與 printing moves 一致；split 可一對多，merge 可多對一。
  沒有 use 的映射仍須證據。原卡保留部分 printing 時舊 face 繼續供它們使用，移轉清單只描述目的面。
- art_moves 每項恰為 `{from_art_id,targets,remaining_uses}`；逐 old card 的所有 art（含未使用候選）列一次。
  targets 元素恰為 `{to_art_id,to_face_id,uses}`，uses 為排序唯一 `{printing_id,face_id}` 陣列；
  remaining_uses 與全部 targets.uses 恰分割修復前 uses（目的 face 以 faces 映射反算比對）。
  無 use 的 art 明列 targets=[]、remaining_uses=[]，原列保留；沒有 art 時整體 art_moves=[]。
- 所有改變 use 集合的 art 都要有完整 after（舊 art 留 remaining_uses，目的 art 保留原有 uses 加新 uses）；
  非空 uses 的 observation 必須指該 art 的一個實際 use。歷史 art 的 uses=[] 是本提案新增容許，
  僅限移轉後無現行 use 的保留列，其 observation 留歷史證據，不拿來通過現行來源檢查。
- 新 card／face／art 必須隨 updates 完整建立。為避免拆分時原代表 printing 的 anchor 已被舊 card 佔用，
  新實體以既有固定 namespace、UUIDv5 名稱 `kind + NUL + "identity-transition-v1" + NUL + allocation_anchor`
  首次配發，保留 anchor 且檢查全域碰撞；只有修復新增實體使用這個提案 recipe，不改任何舊 ID。
  存活 card 仍驗 single／double_faced 數量、side、ordinal 唯一；墓碑亦保留原有完整 faces。

完整移轉不限於 printing FK：printing_face、art uses、圖像／標誌、printed observation 等以具體版次來源
重建至新面；face_revision、current、表記採納、規則同名、baseline、語義、翻譯、DSL 與換皮等
不可只換 face_id 繼承核可。舊歷史留原面，新面缺 fresh 採納時依該能力顯示未知／未實作，
不能造 confirmed 或已驗 DSL。來源更正若仍綁舊 face，須由其獨立契約取得新有效採納，否則阻止受影響結果冒充正確。
建置逐一列出所有實際啟用的面／插畫依賴之重建、保留或失效結果，漏引用為錯誤。

墓碑保留 card.id、owner、layout、原 faces，identity_state=retired；不帶有效 printing／current／自動能力，
原採納與歷史 revisions 可追溯。原 card 未退役時只移指定版次，不能把其餘版次或 defaults 一併丟掉。
公開 support 與每 card 必有列等限制仍照既有 schema，墓碑不得藉空 FK 或刪父列通過驗證。

### 4.1 三種修復的合成對照

下表符號代表已登錄實體，並非真卡資料；每格的面／插畫清單都須完整展開為上述物件。

| 情境 | 修復前 | 必要 updates／moves | 結果 |
| --- | --- | --- | --- |
| merge A→B | A 有 P1/F1/X1，B 有 P2/F2/X2 | P1 改 B/F2；F1→F2；X1 的 P1 use 改到新 X3/F2，X1 uses=[]；X3 新列；A 改 retired | P1、P2 與各自 int_id 不變；A/F1/X1 留歷史；B 保留 P2 的原有圖 |
| split A→B,C | A 有 P1、P2，共 F1/X1 | P1→B/F2/X2，P2→C/F3/X3；F1→[F2,F3]；X1 全部 uses 分別列入 X2/X3；新增 B/C/F2/F3/X2/X3；A retired | 兩個 split 公開事件，玩家選目的，不自動改牌組 |
| reassign P1 A→B | A 有雙面 P1、P2；B 已有 F3/F4 | P1 前／背 F1/F2→F3/F4，兩面 art 的移出與留下 uses 均列；P2 留 A；retire_old=false | P1 父層改 B；A 與 P2 不退役；P1 URL、int_id 不變 |

同圖同框不能省 art 移轉；雙面不能只移正面。純來源 effect 改字而父 card 未變，
只續版觀測與受影響決定，禁止用空 moves 的 reassign 混入公開修復。

## 5. 有效投影順序與原子性（提案）

1. 釘住本次 authored revision、ids 與 transitions 兩個完整入口。先驗全區所有 index／分片 hash、
   原 registry 決定、配號及歷史 bytes；不是先濾 JP 再重算 members。
2. 依 sequence 重建每次 registry_basis 及之前 transition 的有效狀態。basis 之間只容許原始登錄追加；
   驗 previous、全部 before refs 命中當時有效版本，禁止缺號、跳號、分叉、循環與過期前件。
   後來新增的 printing 不能倒灌進歷史決定的 checked 集合。
3. 從該次 review_context 重建原始觀測、target coverage 與 evidence，核對新決定完整集合。
   在暫存投影同時套用該 transaction 的所有 updates／moves；不逐列提交會短暫違反 FK 的半套修復。
   repairs 必須恰好解釋全部父 card 變更與退役，無多餘／遺漏事件；未列的永久欄位變更拒絕。
4. 每次驗 face／art 閉包、uses 分割、card layout、confirmed_none／reskin、跨區全部面與唯一性。
   純來源更新還須確認 card 關係未變；不能透過 after 偷改父層。任何失敗回滾整個候選建置。
5. 將歷史鏈套到本次完整 registry，重新比對本次凍結來源與新增版次，計算 freshness；
   歷史成功不代表本次仍適用。來源缺失、hash 不符與非法前件是輸入錯誤；新增未核對觀測列待審，
   不以舊 confirmed 投影受影響關係。再套各能力的來源更正／表記／勘誤政策。
6. 最後才做 JP／EN 地區投影、DB FK／完整性、公開引用閉包與發布閘門。決定與移轉清單留建置端，
   不擴充公開欄位白名單；無效閉包不可靠地區過濾藏起來。

沿 F1 保存本次兩個入口的 `{authored_revision,index_path,index_hash}` 與全部檔案 exact bytes hashes。
每片有 authored source_record（parser_version=`identity-transition-v1`），decision_source 連完整封套與
所有 raw evidence；歷史前件亦納輸入閉包，不只保留最終 after。這是可重建輸入，無須新增泛型 subject DB 表。

寫入沿 registry 全域鎖：先驗完整計畫，寫新 registry／配號分片及新 transition 分片，最後提交入口。
因 ids 與 transitions 是兩個檔案，不宣稱兩次 rename 等於原子交易：讀寫共用鎖，先持久化恢復紀錄，
任一入口尚未完成時拒絕所有讀取；恢復須驗全批新舊 hash 後完成或回復兩個入口，保留已配號證據且不重用 ID。
恢復紀錄不作採納真值；乾淨 checkout 以同一 Git commit 的完整兩入口重建。
跨入口交易機制與全域單鏈皆為提案；未實作前既有追加拒絕行為不放寬。

## 6. 公開事件、墓碑與路由

以下永久相容原則沿既有規格；本提案僅補明映射方式：

- repair 群組在 DB 展開：merge 一列，split 每個目的各一列，reassign 一列且 printing_id 必填；
  其他種類 printing_id=null。事件 ID 以既有 registry 固定 namespace 的 UUIDv5、canonical `[repair.id,new_card_id]`
  首次配發並查碰撞，後續不重算／重用。公開 reason 不能含決定、私密證據或本機路徑。
- data_version 是該事件**首次正式發布**版號。候選建置以目標版號暫填；發布成功後由永久版本索引
  與該版快照固定事件與 data_version，後續重建驗它並沿用，不每版改時間；preview 不登錄首次正式發布。
  無法取得歷史發布證據時拒絕發布，不把舊事件當首次發布。
  同事件 ID 已公開但內容不同即拒絕。公開修復歷史與舊墓碑保留，changes 只列本次新增事件。
- 卡片網址綁 printing；合併 card 不代表把該版次 URL 轉到另一個 printing。原卡號沒變就不新增 alias。
  確實改號時依 build-db §15 永久保留舊入口、展平 alias 到同 printing 的最新 canonical，
  禁止鏈／環、精確撞號、搶走舊入口與 provisional override。
- 舊分享碼保留 int_id、數量、區域、位置。split 必須讓玩家選擇，不能靜默換 printing／刪行；
  已知 int_id 仍解析原 printing 並呈現修復提示。對局／回放仍釘舊 data_version 與快照 hash，
  不把新父 card、面或文字覆寫進舊快照。

## 7. 完整封套合成例子與 hash 驗算

以下 Python 產生完整 **confirmed_none 決定續版**封套與 index（JSON 也是 YAML 1.2 合法輸入），
不讀資料目錄、不含官方卡文。輸出就是 proposed 格式的完整形狀，不是只列欄位差異。
`0/1/2` 等重複 hex、example store、核對者及空 dependencies 都是合成 pin，不能作真實採納；
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
evidence = [{"store_id": "example", "batch_id": "sha256:" + "7" * 64,
             "source_version_id": "src:v1:" + digit * 64,
             "locator": locator, "role": role}
            for digit, locator, role in [
                ("8", "EXAMPLE-001EN", "identity_observation"),
                ("9", "EXAMPLE-002EN", "identity_observation"),
                ("a", "jp:all", "mapping_coverage")]]
evidence.sort(key=canonical)
record = {
    "record_key": '["identity_transition",1]', "kind": "identity_transition",
    "sequence": 1, "previous": None, "registry_basis": basis,
    "review_context": {
        "context": {"program_revision": "b" * 40, "dependencies": [],
                    "configuration": {"registry": basis,
                                      "observation_recipe": "registry-observation-v1"}},
        "source_batches": [{"store_id": "example", "batch_id": "sha256:" + "7" * 64}]},
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
    "authored_by": "example-author", "authored_at": "2026-10-01T00:00:00Z",
    "reviewed_by": "example-reviewer", "reviewed_at": "2026-10-01T00:00:00Z",
    "reviewed_precision": "day", "note": "合成例，非真實核可",
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
| 完整 transition 成員 | `c957a179eb4b22a34c6bf4a1d5c809fe8ba46fb10c5e10da119954f81879366f` |
| membership（亦為 decision.id 的 `d:` 後綴） | `079081db5e0a2e82177c95f1f02f5179a50006a5add6c83ddbc3b7f925606473` |
| 完整 shard（index.includes 的值） | `3d85ba75a2ad775347bd941d05134f4ec9ed99169c799675c109503b12ad0816` |

此例的 old_record 是重建測試前件，不寫入新分片；舊決定與原 registry 仍需存在於被釘住的輸入。
下一次新增版次時 sequence=2，previous 釘第一筆 record hash／decision，update.before 指第一筆
transition_key、H(new) 與其 decision；after 再列**全部**觀測。不把第一次決定的 members 改成新集合。
reskin 續版用同封套、after 換完整 card_related record，evidence 同時釘 from／to 兩端全部版次；
即使新增版次與舊版文字相同也需要新核可。

## 8. 契約驗收

| 正例 | 必拒絕或隔離的反例 |
| --- | --- |
| 乾淨 checkout 依 pins 重建同一有效投影 | 靠現有 dist、latest raw、檔名字典序、reviewed_at 或最新日期選贏家 |
| 舊 bytes／IDs 不變，續版有新 hash／decision | 改舊 record、刪舊分片、只更新 index hash、重用 int_id、外層核對掩蓋漏看成員 |
| merge／split／雙面 reassign 完整列面與所有 art uses | 漏背面、漏 art、丟 null、重複移 printing、跨 face 共用 art、墓碑 FK 懸空 |
| after 與 moves 恰好一致，修復有 confirmed | 父 card 偷改、空修復、proposed／sampled 修復、自指與環 |
| 新版次經全集合重審後 confirmed_none／reskin 可投影 | 只簽新增差集、target coverage 缺失、任一端過期、新版次自動繼承批准 |
| 同父 card 的來源更新沒有 identity_change | 來源改字就建 split、以修復冒充等義、舊 correction／DSL 無條件搬到新面 |
| 連續改號後所有舊入口仍解析同 printing | alias 指另一 printing、鏈／環、canonical 搶舊路由、split 靜默改牌組 |
| 全交易通過才發布，重跑不追加也不改寫 | 更新一個 index 後另一個失敗仍讀半套、發布後改事件 data_version、改舊快照 |

本文件驗收的是可供審核的 docs 契約；匯入器、交易恢復、CLI、DB／快照與上述正反例測試須另行實作。
