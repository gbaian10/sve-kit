# 數位官方卡名與同名瀏覽連結政策

本契約落實維護者分別明示同意的卡名與連結規則。兩份規則各有政策檔、核可收據及初始排除清單，不能互相代簽。
政策核可不等於逐卡人工核對；入口格式與能力實作也不等於已發布。現行 `digital-links` format 1 的真人門檻保持，
新政策必須有完整 loader、來源重播、逐 owner 驗證與正式投影才可套用。本文不修改現行 DDL／公開枚舉權威表列。

## 1. 名稱採用與關係分開

卡名政策只處理 JP 實體卡面自己的 `face_revision.name`／實際已知 `printing_face.name`，取繁中名字。
SVE 與數位日文完整字串須逐字相同，不 trim、正規化、casefold 或刪字，hash 命中後仍比完整字串。
只查政策釘住的同遊戲完整凍結目錄；所有同名 ID／實際 phase 的繁中名稱須非 null、非空、非純空白、
不含政策明定的假名佔位字元，且完整字串恰只有一種。缺任一語言／phase、同名異譯不能只挑合格項。
JA 與繁中相同但全漢字仍可用；Unicode 最低檢查表由政策明列，不因新的診斷自行擴張核可 matcher。

名稱取詞唯一順序為 **sv1 → svwb**，獨立名字政策與真正人審同卡證據共用同一 resolver。
自動名字規則只有一代完全沒有該 exact 名稱才直接用二代；一代有名但缺譯／異譯不能借二代目錄跳過。
合法完整目錄的歧義／缺譯，可由 owner 自己有效的真人同卡精確面連結及必要語義指派供名：
confirmed 本筆在 checked，或 sampled 本筆在實際 sample_ids；無合格一代人審候選才用合格二代。
來源損壞、錯父卡／面、壞 hash 仍整次拒絕；第三張無自己的有效依據不得借另外兩張的名字。
真人選詞 > 合格政策官方名 > 必要的自己真人同卡供名 > 其他合法選詞 > 原文。
順位第一只收真正看過的 confirmed checked／sampled sample 成員；sampled 非樣本仍有效，但按真實 origin 放其他合法選詞順位。
純譯名差異選勝出名稱並報差異，不因此退回原文；真正語義歧義仍須消歧，機器來源審過仍 machine。

每個 owner 驗自己的永久身分、registry／transition／source_face_map、來源語言及完整名稱。
背面不借正面，unknown printed 不借 current，印刷舊名依自己的來源取名。直接名字不要求先配 glossary 概念，
也不供 `glossary_choice.digital_name` 的同概念證據；效果引用與概念／語義指派仍各自驗，整段效果仍 unofficial。
畫面標「數位版官方卡名」，不表示官方實體繁中版或逐筆真人確認同卡。不改約 960 筆名字的既有委託範圍。

## 2. 同名規則瀏覽連結

獨立核可的連結政策以非空 exact 日文名先視為同卡，新增待能力實作的 `relation=same_name`。
不沿用 `same_card+confirmed`；既有真人 same_card／same_character／name_only 的定義與門檻不變。
所有同名數位 ID 都列，兩代全列，UI 先 sv1 再 svwb、各代按原始 ASCII official_id 排序；wire 仍依既有 link.id。
同一實體卡到同 game／official_id 恰一筆；subject 恰為 `{card_id,face_id:null,game,official_id,digital_phase:null}`。
不同面／印刷名稱可提供各自證據，但不展開面×phase，也不推測前後面機制對應；保留所有實際匹配來源。
完整目錄可能命中多張，不表示已確認異畫；繁中缺譯、職業／卡種／圖不同不阻擋這個瀏覽規則。

same_name 只授權瀏覽，永遠不供官方名稱、同概念、效果等義、DSL、圖或語音。
畫面用「同名規則視為同卡，未逐筆確認」。公開四種 review_level 不增值，此類固定 unreviewed；
typed 政策投影分支不能呼叫 `Source.review()` 把內部機械 confirmed 投成真人 confirmed。
application 可沿 decision FK 記 approved_rules／完整 checked_by_rules，但 sample_ids=[]、human_checked=[]，
核可者／核可時間與工具套用者／時間分開。沒有完整能力前不產生這種 application 或正式 link 分片。

fresh 真人同 card/game/ID 的任何合法關係優先，抑制同組重複規則連結。
真人負面、stale 或撤回不被一般規則復活。真人撤回後若要放行該對規則連結，僅維護者明示指定卡對，
追加輕量 rule_resume 收據，釘原 terminal 的 record_key／hash／decision_id、政策 hash、卡對、真實訊息 UUID／instant、
理由與前件／結果 hash；不恢復真人採納，不繞來源／排除，新 terminal 再變須重新放行。
政策卡層轉真人卡層且完整 subject 相同時沿原 ID；真人精確面是不同 subject，另 ID 並抑制原規則列。
ID 仍為 `dl:`＋H(`["digital-link-v1",subject]`)，relation、時間、政策／清單 hash 不參與。

首批 coverage 不採納，公開 `digital_link_coverage=[]` 是未知，不能表示查無。
未來完整 coverage 的 links 集合含有效規則與真人結果，但 same_name 不等於真人查完或 reviewed_none。
草稿 93／2,041 約 4.6% 是警訊而非誤連率：84 同角色、8 兩代判斷不一、1 只有同名，全部在 232 待確認內。
維護者選「照規則連」，初始排除空；不改草稿原關係、不把它們記成真人 same_card，錯連數仍未知。

## 3. 政策、核可文件與可攜入口

沿 authored-layout 的嚴格 YAML 1.2、安全路徑、全入口查重、canonical-json-v1 與單檔 <1 MiB。
未知欄／格式、Bool 當整數、symlink、缺檔、孤立／未索引檔、hash 不符、分叉或缺號均拒絕。
版本從 `001` 起只增，既有 policy／approval／exclusions 的 bytes 與 index entry 不改。

| 路徑（相對 authored） | 完整頂層欄位 |
| --- | --- |
| `digital-name-policies/index.yaml` | `digital_name_policy_index_format:1,kind:digital_name_policy_index,policies` |
| `digital-name-policies/<policy_id>/<version>.policy.yaml` | `digital_name_policy_format:1,kind:digital_name_policy,policy_id,version,purpose,approved_document_hash,projection_recipe,content` |
| 對應 `<version>.approval.yaml` | `digital_name_approval_format:1,kind:digital_name_policy_approval,policy_id,version,policy_hash,approved_document_hash,presented_text_hash,reviewed_by,reviewed_at,reviewed_precision,approval_events,evidence_hashes,initial_exclusions_hash,disclosed_changes,note` |
| `digital-name-exclusions/<policy_id>/<version>.yaml` | `digital_name_exclusion_format:1,kind:digital_name_initial_exclusions,policy_id,version,purpose,approved_list_hash,entries` |

policy_id 沿實際核可文件，不因名稱帶 draft 就當未核可；version 為正整數，路徑序號至少三位。
purpose=names/links。policies 是 policy_id → 非空版本陣列；每項恰為
`{version,path,hash,approval_receipt_hash,exclusions_path,exclusions_hash,predecessor}`。
path／exclusions_path 固定符合上表與 ID／版本；predecessor 首筆 null，後續指上一 entry 的完整 canonical hash。
所有 Hash 是完整 `sha256:<64 lowercase hex>`。消費端 authored_revision 釘已合併後 main 的索引及全部歷史 pair，
索引不引用自己的未來 commit；完整 bytes 另進 F1。

**核可文件 hash 與 authored 操作政策 hash 分開**：維護者核可的是私人保存的完整呈現文件，含歷史題目／本機追溯欄，
不得刪路徑、回填核可事件或換封套後宣稱他核可新 hash。`approved_document_hash` 始終釘該完整 parsed canonical hash；
`policy_hash`／index.hash 是本表操作政策完整解析值的 hash。收據同時釘兩者，不混用，不產生 hash 自我引用。

projection_recipe 恰為 `approved-digital-name-document-v1`，content **逐欄原值複製**下列集合，不能改字或改規則：

| purpose | content 恰含的完整核可文件欄位 |
| --- | --- |
| names | `policy_id,actual_answers,normative_rules,plain_language_rules,catalogue_pins,scope,game_priority,target_minimum_check,display_label,excluded_names,visible_notices,shared_answer_bindings,shared_future_scope_rules,final_exclusions_hash,proposed_clause_replacements` |
| links | `policy_id,actual_answers,rule,review,limits,precedence,exclusions,ids,catalogue_pins,registry_pins,warning_disclosure,coverage,publication,f1,versioning,plain_language_rules,visible_notices,fixed_receipt_disclosure,shared_answer_bindings,shared_future_scope_rules,proposed_clause_replacements` |

只省略既有文件的呈現題目／草案狀態／歷史流程／本機背景路徑等非操作欄；完整已核可文件留私人持久證據。
採納當下先驗其 canonical、真正呈現白話全文、頁面與按鍵，逐欄驗投影；不能藉新封套／recipe 添加新授權。
之後 build／CI 驗 authored 內已驗的操作政策、收據與來源閉包，不重讀未入庫的私人核可頁；
不聲稱重新驗了原頁，也不因私人頁不在 CI 而使真實採納失效。缺 supported projection recipe 即拒絕使用。
任何操作文字、matcher、枚舉、scope 或來源語意改變須另核可；來源／清單有限續版依 §5，不借純 metadata 投影改規則。

## 4. 真實核可與初始清單

reviewed_by 逐字等於明列維護者 `gbaian10`；reviewed_precision=instant，reviewed_at 為本政策真正按鍵時間。
approval_events 是按事件時間排序的非空陣列；每項恰為 `{kind,at,uuid,source_hash,locator,value}`。
kind=page_button/message；按鍵沒有訊息 UUID 時 uuid=null，value 是原始按鍵 JSON 完整值；
message 的 uuid 是真實訊息 ID，value=null，source_hash 釘不可變事件摘要，locator 精確定位該訊息。
按鍵必驗 policy_hash=approved_document_hash、text_sha256=presented_text_hash、value=agree；拒絕背景／作答紀錄代替明示核可。
evidence_hashes 是不可變來源檔名 → exact bytes hash 映射，只含 basename／安全相對路徑，不存私人絕對路徑、憑證或官方名稱。
政策／規則文本不是官方卡文，可入 git；完整官方目錄／卡名表與私人頁面不進 git。

卡名收據必須同時引用政策按鍵與真實口頭同意訊息；連結收據引用自己的按鍵。
disclosed_changes 是恰含 `{rule_id,removed,added,disclosed_at,accepted_message_uuid}` 的排序陣列：
卡名第8條少「不因報告另擋發布」、第13條少「未來另審格式」且加「本卡名政策」限定，
是已逐句揭露並接受的刻意差異，不還原、不重算原核可 hash。disclosed_at 指按鍵前的真正揭露時間；
兩筆 accepted_message_uuid 均指收據內口頭同意訊息。連結沒有此差異，陣列空。
note 明示政策核可、無逐筆／抽樣樣本；不引用其他政策來補假樣本。

初始排除 entries 恰等於已核可文件的完整最終清單；本批 names／link name／card_target 三類均空。
approved_list_hash 釘原最終清單 canonical；initial_exclusions_hash 釘本表可攜清單完整解析值的 canonical。
names entry 為 `{source_lang,source_name_hash,reason}`；links entry 為
`{kind:name,source_lang,source_name_hash,reason}` 或 `{kind:card_target,card_id,game,official_id,reason}`。
完整鍵排序唯一，理由非空；清單可以空但必明示，不為空清單另造操作事件。

## 5. 續版與排除

第三題 agree：新 JP 凍結批次自己的 owner 符合有效政策／指定目錄，就沿用，逐批報新增，不冒稱真人逐張看過。
第四題選 **乙，兩政策共用**：純新增、完整符合各自條件、舊名稱／資格／取名遊戲／既有連結與對應未變，
無新增兩代異譯、排除或真人決定衝突，才可有限機械續版。兩代異譯、譯名改變、失去資格、既有對應變動、
目標消失仍由維護者實際看過明示同意；較新建置批次只報差異，不使原政策 stale、不偷偷換目錄。
有限續版仍釘完整新批次、精確前件及完整檢查報告，不把原事件寫成他看過新批次；
**有限續版的封閉格式與 loader 尚未完成，不先啟用自動新增**。

names 排除不允許借真人 B／規則 link 繞回自動官方名，但本人實際採納選詞仍可用；
它不撤真人關係或獨立規則連結。改 names 清單須核可完整新清單與差異，並證明其他規則／目錄未變。
links 排除只移除規則證據：同名排除不移除其他合法名字對同卡對的依據，整對排除才移除所有名依據。
真人有效關係照自己的決定顯示，重疊只報告不停出貨；要撤真人另走真人撤回。
兩份清單、hash／核可各自獨立。後續 links add/remove 只增 log 與輕量收據，釘實際維護者訊息 UUID／instant、
政策 hash、精確操作／理由、前件與結果集合 hash；remove 是新記錄，不刪歷史。
修改規則／遊戲範圍不能借輕量清單收據，未支持續版格式不得靜默套用。

## 6. 來源、F1、穩定 ID 與能力啟用

歷史背景由宣告的不可變 Git 版本驗，不與目前磁碟執行期比較；正式來源基準用 main 保留的 commit，
不能用會被 squash 掉的功能分支 commit。目前程式／parser／checker 的完整依賴另釘並重驗使用，
加註解或重構不讓歷史政策失效；raw／父層／閉包錯仍建置拒絕。
F1 包含政策索引／完整 pair／清單及鏈、核可摘要、所有 raw descriptor／來源／parser、逐 owner 與實際使用；
完整 `record.verify(...,complete=True)` 不能由子階段 partial verify 代替。

名稱 loader 回傳獨立 typed frozen NamePolicyResult，同一 resolver 明示接政策 result 與真正 B 結果，
不向 DB 塞假 digital_link／同概念。名字走既有 text_unit／translation/use／FieldTranslation，decision_id 可 null。
直接政策名字沿 render-v1，詞彙鍵含所選 game、exact JA／繁中 hash 與取名 recipe 語意，
不加入後補概念、owner、整批目錄、政策／收據／清單 hash、時間或資格 checker；完整證據留 F1。
真正效果／term 引用才依賴概念／choice；概念補配不讓普通名字整批換 ID。
原文、所選字串或取名語意實際改變仍按既有 recipe 重算，不能藉穩定 ID 隱藏差異。

純名字政策不要求發布數位卡／圖／link；同名瀏覽才啟用最小 digital_card／digital_face／digital_text／
digital_link／digital_endpoint 閉包，不因查整份目錄把所有數位內容公開。
same_name 啟用前由程式 PR 同步 DDL、公開 relation 白名單、typed projector、fixtures 與 reader，
新增枚舉以 minor＋`digital-same-name-links-v1` required capability／min_reader 拒舊讀者；改欄序或既有意義需 major。
review_level 原四值不變。正式檔案／表列不能在 docs-only PR 先改成已支援，舊快照不回寫。

## 7. 後續程式反例

以下是必備合成驗收，不宣稱已有這些政策 loader 測試。

| 情境 | 預期 |
| --- | --- |
| 用背景表態／作答紀錄、錯政策按鍵、缺卡名口頭同意或錯 UUID | 收據拒絕，不補造樣本 |
| 投影改一個規則、來源 pin 或答案，仍引用舊核可 hash | 採納拒絕；只容許固定非操作欄省略 |
| 同名目錄有異譯／缺譯；兩 owner 各有真正 B＋指派，第三張無 B | 前兩張各取自己名字，第三張不借；排除不能借 B |
| 完整目錄子集、phase／目標語遺漏、錯來源父卡、背面借正面、unknown printed 借 current | 各自拒絕或保持缺譯，不能當已知 |
| same_name 供名字／概念／語音，或由 Source.review 投成 confirmed | 各自拒絕；規則瀏覽固定卡層雙 null／unreviewed |
| 同名多 ID／兩代／多面與 phase | 所有 card/game/ID 去重列出，所有來源保留，不展開面×phase |
| 真人撤回／負面，被規則無事件復活；有新 terminal 沿舊 rule_resume | 拒絕；有效精確放行也不恢復真人結果 |
| Q4乙純新增帶舊名／資格／遊戲／對應變更，或格式未支援就續版 | 拒絕自動續版，舊指定目錄只出差異報告 |
| 只改現在程式註解，歷史原 pins 可驗；漏 F1 使用 | 前者歷史有效，後者完整建置拒絕 |
| 原政策／收據／清單改字、分叉、漏檔、未索引、symlink、錯 hash | 全入口拒絕，失敗不留半批資料 |

CLI 彩色輸出、無全域 git 設定、caller transaction rollback 與 runtime 閉包按既有規則驗。
政策數量與正式 owner 使用／link 出貨數分開報告，不拿候選2,041名／3,376配對當已發布量。
