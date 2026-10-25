# 第二批自動能力：超進化、main phase、離場與受傷

本文件補充[第一批觸發契約](trigger-lki-semantics.md)，只定義內部 typed IR 的執行義務。
以下四個有限入口取代第一批 §1 對應的暫時拒絕；未列出的形狀維持拒絕。
lowering／checker／executor 已接上以下有限入口；DSL 的 `whose=each` 仍受現有 A.player-ref
schema 限制而拒絕。這不改 DSL schema、公開 program、capability 或最低版本；
本文、選項、成本及所有介入效果仍須整棵准入。執行與資訊邊界見
[內部執行契約](internal-execution.md#第二批自動能力入口)。

規則依據為[日文 CR 1.28.0（2026-09-28）](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf)，
PDF SHA-256 `c44b8f2ff7e1019bc2b4ec59df894d6a067179ff05eac2542b7723648a22cd47`。
以下條號均指此版；事件表示與准入限制是專案設計，不是官方新增裁定。

## 1. 盤點與准入範圍

盤點使用 `card-data/index.json` 的 JP 分片，每張卡取 `observations` 中
`(source.fetched_at, source.id)` 最大者所指文件的 `record.faces`，只讀非空 `text`，
以原樣 `(name, text)` 去重；不合併異文、不同名稱或不同進化面，不計 sections、Q&A、flavor。
基準為 commit `a3b2d67484e390d4aae24ba4a5cb489707fd86a6`：7,383 個印刷面，
3,689 個非空去重面。下列數字是**面數**，不是能力數、整卡可執行數或正式 coverage 分母。

| 候選族 | 字樣命中面數 | 本片入口與排除 |
| --- | --- | --- |
| 超進化時 | 51 | 行首 `【超進化時】` 50 面；排除 PR-404 SEP 的規則說明 |
| メインフェイズ | 36 | 自分／相手 main 到來單一行首 32 面；扣除 EX 有效的 BP13-071，得到 31 面入口候選。各プレイヤー主語、授予、AnyOf 與階段禁止文字不納入 |
| 場を離れ | 32 | 只收自身「これが場を離れたとき」及自分回合限定，共 12 面入口候選；其他物件監看、移區歷史及取代提醒文不納入 |
| ダメージを受けた | 30 | 26 面為自身受傷入口；另 1 面主戰者受傷、3 面受傷歷史條件不納入 |

入口候選仍可能有未支援本文，例如離場後讀祈りカウンター；不能由上述數字宣稱整卡可執行。
其他第一批以外的候選以全文子字串或正則命中計，一面可跨列；授予、延遲及本文中的引文
也可能命中，數字只用來排序，不能當成 Printed 訂閱清冊：

| 字樣／族 | 面數 | 本片處置 |
| --- | --- | --- |
| プレイしたとき／捨てたとき／進化したとき | 45／34／26 | 拒絕長寫打出、棄牌與監看其他進化；原有自身進化時不受影響 |
| 場から墓場に置かれたとき／場から手札に戻ったとき | 26／8 | 拒絕一般 Move 訂閱及移動後跟隨 |
| 攻撃したとき或攻撃するとき／ダメージを与えたとき | 19／10 | 拒絕長寫攻擊與造成傷害訂閱 |
| リーダーの{体力}を+したとき／リーダーの体力が減少したとき | 12／6 | 拒絕 LifeChange |
| これの{攻撃力}或{体力}を+したとき | 7 | 拒絕 StatChange；不是 Damage 的別名 |
| ドライブチェックによってトリガーしたとき／ドライブチェックしたとき | 7／3 | 拒絕聯動機制 |
| EXエリアに…置かれたとき／アクトしたとき | 6／6 | 拒絕 EnterEx、Posture |
| 融合したとき或融合されたとき／サイコロをふったとき | 5／5 | 拒絕融合與骰子訂閱 |
| 破壊されたとき／消滅したとき／引いたとき | 3／2／2 | 拒絕獨立 Destroy、Banish、Draw 訂閱 |
| 出走したとき／{UB}能力が発動したとき | 2／2 | 拒絕聯動機制 |
| 装備したとき／選ばれたとき／カウンターが0になったとき | 1／1／1 | 拒絕 Equip、Targeted、State |

這是依卡文字樣的候選盤點，不聲稱窮盡所有改寫句型。沒有命中的事件也不因此准入。
所有入口只收 Printed（或以 Printed 為 origin 的關鍵字）、`AbilityKind::Triggered`、
能力與 trigger 的 `active_zones=[Field]`、`PerFact`，以及第一批 §3 的無限次／每回合一次。
Grant、Delay、非 Field 有效區、AnyOf、PerBatch 與其他限次均拒絕。

## 2. 共用交易與資訊邊界

直接沿用第一批 §2–5 的 pending occurrence、TurnId 額度、controller 選序、
SourceInformation／LKI、prepared play 與 rollback。新事件在實際動作完成的同一交易
入待機；檢查時點仍先規則處理，再由回合玩家選一個待機，不能以事件種類硬排順序。
不新增 queue、receipt 家族、歷史儲存或公開追蹤身分。

每個 producer 保留既有 EventHeader：EventId、OriginFactId、batch、cause、actor、
source、規則時間與 before／after snapshot。predicate 與 captures 讀 AtEvent；
本文「なら」留在結算時的 K.if。來源 controller 固定於待機時；事件 source、受事件影響者
與能力 holder 是不同角色。合法事件仍可因本文或介入效果未受驗而使整次操作預檢拒絕。

下文的 `AfterEntry` 沿用第一批表示「該事件完成後」，不是讓事件另發 EnterField。
內部 ObjectRef、SnapshotId 與 LKI handle 不直接送 wire；公開選序題只顯示事件當時
可見的卡名、場位／階段與當次 occurrence 選項。不得用相同實體牌之後的身分補讀事件。

## 3. 超進化時

### 3.1 程序與同時觸發

依 CR 12.2.2–.4、3.3、5.16、12.17：合法進化能力的打出成本包含公開適正進化卡；
原有 PP 成本照付，至多 1 點可用 EP 替代。先攻的**自身經過回合數**至少 7、後攻至少 6
時可額外支付 1 SEP；不是把 SEP 折抵 PP／EP，也不是以全局 TurnId 判門檻。
直接效果「進化する」不因剩餘 SEP 自動升級成超進化，食事／憑依亦不冒充本事件。

關聯進化卡並替換印刷資訊後（5.16.1.2），超進化再加攻擊力、體力各 1（12.2.4.1），
然後在 5.16.1.3 同一時點形成進化與超進化事實面（12.2.4.2）。
場上 holder 世代不換；進化資源離開 evolve deck 的世代及關聯照既有進化程序處理。
以**進化後有效能力**匹配；進化前能力消失，新印刷定義依第一批 §3 取得自己的能力實例。
數值保留與還原依[目標與進化契約 §3](target-evolution-semantics.md#3-純-add-跨進化與到期)，
不把 +1 混成另一個進化，不在兩個事實面之間插檢查或結算。

單一 `EvolveEvent {subject, with, kind, before, after}` 足夠：普通進化 `kind=Evolved`，
超進化 `kind=SuperEvolved`，after 包含 +1。自身進化時匹配兩者，自身超進化時只匹配
後者；同一次超進化共用 header／OriginFactId。兩個不同 AbilityInstance 各待機一次，
不能把進化時觸發兩次，也不能因共用事實而把兩個能力合併成一個。

兩種能力同時待機後，由其 controller 依第一批選序任選先後（10.7.3.1、10.5.2；Q2056）；
沒有「進化時一定先於超進化時」。兩者要先全部入待機，才交出選序題。
`view=AfterEntry`；source/cause 由真實付款與規則程序建立，不接受 caller 的 `kind` 字串。
來源與進化卡在此都是已公開資訊，但不公開內部關聯 ID。

### 3.2 引擎接點

下列為檔案所見的現況，非本片新增的 runtime 保證：

| 已有接點 | D1 必須補的差異 |
| --- | --- |
| `game/progression.rs` 的 `can_super_evolve`、`evolve_with_code`、`apply_evolution` 已檢查經過回合、PP／EP／SEP、扣費、關聯與加 1 | 沿用該程序，不增加第二條超進化付款路徑；失敗依外層 origin 回復 |
| `game/internal/card/evolution.rs` 以受驗 intrinsic 投影進上述程序；`tests/internal_ir/card/evolution.rs` 已有正常／超進化代表局面 | 不把規則動作支援等同於超進化時訂閱支援 |
| `game/internal/card/triggers/evolution.rs` 已生成含 SuperEvolved 的 EvolveEvent，現只掃 Evolve pattern 並逐能力建 pending | 加入超進化關鍵字入口、真正按 kind 匹配，將兩種候選原子交給既有待機機制；普通進化不能觸發超進化時 |
| `ir/events/timing.rs` 與 `game/internal/card/triggers/timing.rs` 只准自身 OnEvolve | 擴充正規 predicate、來源證據與准入；保留同事實不同能力的重數 |

舊字串路徑依序 emit「進化」「超進化」的 log／group，不是 typed IR 可以採用的
選序依據。其先後順序不得形成兩次檢查時點或兩筆 typed 進化事實。

## 4. main phase 到來

`PhaseEvent {turn, player, phase=Main, edge=Begin}`，`view=AfterEntry`，
header cause 指 CR 7.3.1、source=None。player 是回合玩家；holder 是當時場上能力來源，
兩者不必同一玩家。收 `whose=self/opp/each` 的 Printed phase 訂閱；each 只是兩方回合
均匹配，**不把能力 controller 改成當前回合玩家**。

CR 7.2.5 的 start 檢查完整結束、對局尚未終局，才在 7.3.1 發一次事件；
7.3.2 處理待機後才能進 7.3.3 自由行動。普通行動結束後回到 main、Quick 返回、
繼續回答 pending 或重入檢查，不重發同一個 Begin。首回合的 main 也要經相同入口。
開始階段不是 main；「次の…」延遲訂閱、phase=Start、一般 turn begin/end 均未解鎖。

`game/opening.rs` 的首回合入口與 `game/rules.rs` 的正常回合入口現在只收集
舊 `main_start`；D1 要接到既有 typed Phase／pending 程序。不能只擴充
`internal_end_event` 的比對，卻漏掉 producer 或首回合。
BP01-083「各プレイヤーは、自身の…」涉及每位玩家執行者，不降為本片 each 的同義式。

## 5. 自身離場

只收 `auto.on.left_field.ref=this`，可附 `during.turn=self`；不收空 payload、
sel、who、by、監看他物或指定目的區。以 `LeaveFieldObjectBefore == SourceHolder`
匹配，`view=BeforeExit`；與 Last Words 共用 LeaveField producer，但**不加 to=Cemetery**。
CR 10.7.4.1.1–.2、10.7.4.2 要求離場前有效能力與資訊；破壞、成本、回手、消滅等
只要實際 Field→非 Field 都形成事件。Field→Field 控制轉移（被奪取不誘發，Q1305）、
同區重排及被完全取代而留場不形成事件。未受驗移區 producer 仍拒絕，不能因「離場」文字跳過取代／禁止檢查。

沿用 `LeaveFieldEvent {object_before, controller, lki, to}`；to 是實際目的玩家與區域，
object_before 為離場前世代，lki 在清除數值、進化關聯或換世代前封存。
同批移動共用 before 狀態；實際成功才提交 pending。來源 holder 與 object_before
相同；之後原物件回場的新世代不能被該待機能力操作。資訊讀值仍只有第一批 §5
SourceInformation 已准入的欄位，不開放離場 counter 讀值或泛用 MoveAfter。

本片對第一批 §4 隱藏區拒絕作一個窄例外：**公開場上物件離場到手牌／牌堆**時，
可以由公開 before 資訊建立待機，公開事件只含已知離場卡及目的區種類；
不得帶移動後 ObjectRef、手牌索引、牌堆位置或可連回隱藏牌的識別。
`lki` 不含隱藏目的區 after 視圖，也不能讓下一個新世代重新可追蹤。
裏向消滅、原本非公開來源及無法驗證此投影的路徑維持拒絕。

`game/internal/card/triggers.rs` 的 `internal_before_departures` 目前只為自身
Field→Cemetery 的 Last Words 建候選；D1 須把本片自身離場加入同一個 departure plan，
保留其他目的區及正確 LKI。不能再建一套離場 queue，或讓同一 Move 的多個 facets 重複入待機。

## 6. 自身受到傷害

收 `auto.on.damaged.recipient=this`，kind 省略或 ability，during 省略或
`{turn:self/opp}`；不收空 recipient、主戰者、其他物件、by、attack/combat 專屬訂閱。
kind 省略表示所有傷害，**包含戰鬥傷害**，不能靜默限縮為能力傷害。
`view=AfterEntry`；事件 recipient 的精確 ObjectRef 必須等於 holder。

依 CR 5.14、1.3.2.2，先處理取代／禁止與實際正傷害，完成扣體力才發
`DamageEvent {source, recipient, amount:PositiveInt, flags, controller}`。
amount 是真正給出的傷害，不是原提案或被剩餘體力截斷的值；體力可變成負數。
source 為傷害來源的 RuntimeObjectView，controller 是該傷害來源的 controller，
不是受傷者；pending controller 另外由受傷 holder 固定。cause／source 仍依動作來源保存。
全防止或 0 傷害沒有事件；直接扣體力、支付體力與降低數值不是 Damage。

`flags=Ability` 對應非 CR 8.4.9 的能力傷害；戰鬥用既有 Attack／Combat／AttackCombat，
單一 hit 即使帶兩種 facet 也只有一筆 DamageEvent。kind=ability 只匹配 Ability。
同時多個 hit 全部提交後才可能到檢查時點，仍依 PerFact 與第一批額度規則保留重數。
不把「受到傷害後仍存活」暗加成觸發條件：致命傷也先待機，檢查時依 10.5.2.1、11.3
先破壞，之後待機仍可解決（Q1090）；不能先讓自身加體力的受傷能力搶在規則破壞前救回自己。

場上來源／受傷者與傷害量可公開，source 的內部 snapshot 不可公開。
造成傷害的能力來源已離場時，事件 source 沿第一批 §5 LKI；不能用同張實體牌新世代替代。
事件 amount 可透過既有 `IntFrozen::DamageAmount` 在該 Damage binding 作用域讀取；
「そのダメージが5以上なら」仍是本文 if，不在觸發端篩掉小傷害與限次消耗。

`game/internal.rs` 已有能力 DamageEvent facts，但尚未使本片訂閱入待機；
`game/damage.rs::apply_damage` 及戰鬥傷害入口須在實際扣體力的共同提交點接上。
不得同時從 facts 回放與舊 damage log 再掃一次。載入無 kind 限制的訂閱前，
必須具備戰鬥與能力兩條 producer；只完成能力路徑時只能准入 kind=ability。
任何仍未受驗的傷害取代／禁止，沿整棵預檢拒絕，不能從 requested 值捏造 actual。

## 7. lowering、良構與最小驗收

沿[DSL 事件矩陣](author-event-matrix-1.0.md)與 schema 既有寫法，不新增別名。
lowering 的 `Ability` 補 `super_evolve_time`，phase 解碼接受 main，Auto 補本片的
left_field／damaged 有限 payload；現有 `During` 只收 self，damaged 須另收 opp。
未列欄位與 shape 靜態拒絕，不忽略。
body 與限次沿既有 lowering；`auto.on.super_evolved`、長寫 evolved、一般事件聯集不在本片。

現有 `KeywordId::OnSuperEvolve`、`EvolutionStateFields::EvolveKind`、
`EventPattern::{Evolve,Phase,LeaveField,Damage}` 與 payload 足以表示本片，毋須新事件型別。
checker／runtime admission 必須同時擴充正規形與 frozen 欄位求值，不能只放寬 walker 白名單：

- 超進化 predicate 為自身 Evolve subject **且** kind=SuperEvolved；
  進化時仍是自身 Evolve subject，不加排除 Super 的條件。
- main 為 Main、Begin 與 whose 條件；離場為自身 before；
  受傷為自身 recipient 加已列的 flags／turn 條件。事件 binding 支配所有讀值。
- 每棵使用的 producer、來源 origin、有效區、view、aggregation 與 limit 必須受驗；
  closed synthetic Program 不得自填 source／event 就取得能力入口權限。

兩組型別／良構代表對照（語義摘要，不是另一套可序列化語法）：

| 正例 | 反例與拒絕理由 |
| --- | --- |
| Printed→OnSuperEvolve，Evolve subject=self 且 kind=SuperEvolved，AfterEntry | 同關鍵字只比 subject，會在普通進化誤觸發，拒絕整棵 |
| Printed 自身 LeaveField，BeforeExit，本文讀 SourceInformation | 改成 AfterEntry 或追蹤隱藏目的區新世代，缺合法來源視圖／跟隨權限，拒絕 |

D1 每類只新增一個代表局面，搭配下列小變體；沿用第一批選序、回復與額度驗收，
不重建矩陣，不把以下待實作案例當作本片已跑的測試：

1. 合成進化後同時有「進化時抽 1」「超進化時抽 2」：合法 SEP 付款與 +1 後可任選
   其中一個先解決，另一個仍待機；普通進化只待機前者。沿用現有付款失敗測試。
2. main 到來待機一次，處理完成前不能自由行動；之後一次普通行動返回 main 不重發。
   首回合入口以同一情境的小變體驗證，不另建階段矩陣。
3. 合成自身離場抽 1 的來源回手仍待機，投影無手牌新 ID；同批共用 before。
   改成移動被完全防止則不待機。Last Words 的回手不觸發沿用第一批反例。
4. 合成自身受傷抽 1 的來源受致命正傷害，先待機、檢查先破壞、再抽 1；
   改 0 傷害沒有 pending。無 kind 限制的同例覆用戰鬥 hit；不得另造完整傷害測試格。
