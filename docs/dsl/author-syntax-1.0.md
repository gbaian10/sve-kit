# SVE 效果 DSL：撰寫語法 1.0

版本 1.0，2026-09-28 採用。配對 [IR 詞彙](ir-vocabulary-1.0.md)；總覽見 [README](README.md)。本文件定義撰寫語法，不是已發布的 JSON Schema，也不宣稱引擎已接受示例。日文卡文與 Q&A 以 2026-09-26 凍結資料為準，CR 1.27.0。以下新增欄位都是封閉型別；不能用 `custom_effect`、自由字串路徑或手寫 IR 逃避登錄。

## 0. 原則與記法

只有一種卡片 YAML。`A.*` 是撰寫構造的穩定 ID，動作為 `A.act.<鍵>`，`each_player / foreach / repeat / instead / simultaneous` 為 `A.comb.<鍵>`；`A.if / A.pay / A.optional / A.choice / A.either` 不加 comb 前綴。讀值與條件是 `A.expr / A.cond`，效果子鍵統一 `A.effect`。巨集在載入時展開成有型別 IR，不能從物件所在公開區域猜決定時點。

`Expr` 是純表達式，`Cond` 是布林表達式，`Sel` 是有型別集合或保序視圖，`Ref` 是物件／目標引用，`Steps` 是有序步驟列表。`n` 精確數量、`upto` 0..N、`any: true` 0..候選上限，三者互斥；只有チョイス的 `upto` 依 CR 5.18.2.1 為 1..N。`may` 表示可以拒絕整項，與選零個分開。

每個 `bind` 的型別由構造決定：選取為物件集、宣告為數值／卡名、骰子為骰值；`result` 永遠是 ReceiptId。作用域內不可重名、不可向前引用、分支局部綁定不可流出，除非每個分支輸出相同型別。未登錄或執行能力尚不具備的欄位在載入時附卡號、面、行、子句、預期／實際型別報錯。

## 1. 檔案、卡片與 meta

`A.file = {schema: sve-author/1.0, cards: Map<CardNumber,A.card>}`。卡號照官網保存。卡表快照提供印刷名稱、種類、數值、種族、圖示，這裡不維護另一套卡表。

`A.card = {meta, note?, abilities?, faces?, deck_rule?}`；faces 用穩定 FaceId，不依名稱猜日英對應。`A.ability = {line, section?, <一個能力頭>}`；line 從 1、section 從 0 起算，與分類 index 的零起算 line 轉換時加 1。同一行多能力分成多條，保留相同行號。token 說明段落必須掛 token 定義，不能掛母卡。

```yaml
schema: sve-author/1.0
cards:
  BP01-046:
    meta:
      dsl: "1.0"
      source: "sha256:<正規化來源內容雜湊>"
      written_by: { who: human, date: "2026-09-27" }
      reviewed_by: []
      status: draft
      verified_by_exam: []
      rulings: []
    abilities:
      - line: 1
        keywords: [ward]
```

這是容器示意；§13 的真實例子則省略工具自動填入的 meta，只列指定能力行或指定構造片段，並明示片段範圍。meta 的規範見 [ADR-0012](../adr/0012-version-meta.md)：`draft → reviewed → verified`，爭議為 `disputed`；模型不同的審核或合格巨集的三項機械檢查才可 reviewed，實跑才可 verified。`rulings` 指裁定 ID，不直接堆 Q&A；巨集產物另記 `generated_by: {macro, version, params}`。舊 `review / ruling` 不沿用為驗收狀態。

## 2. 能力頭與來源

降低成 `B.ability`、`B.source`。持有者、controller、文字提供者與印刷位置分開；授予不是複製來源身分。共通欄位：`targets? / cost? / optional_cost? / x? / define? / do / once_per_turn? / ub? / zone?`；只有適用的頭可填相應欄位。`zone` 接單值或集合（field、hand、ex、cemetery、equipment 等已登錄區域），預設依 CR 10.3：場上從者／護符 field、crest ex、equipment equipment。

| 構造                                                                                                                                                         | 內容與降低                                                                                                                                                                  |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `keywords: [Keyword…]`                                                                                                                                       | `B.keyword`，保留規則展開來源；ward、storm、rush、assail、bane、drain、intimidate、aura、stack、single_drive、twin_drive、drive、start_amulet。參數型關鍵字依 CR 12–14 登錄 |
| `quick: true`                                                                                                                                                | 卡的 Quick 許可；不控制引擎是否開窗口                                                                                                                                       |
| `fanfare / lastword / evolve_time / super_evolve_time / attack_time / race_time / drive_time`                                                                | `B.trigger` 巨集；分別匹配入場／場到墓地／進化／超進化／攻擊／出走／首次獲 Drive，均以 this 為主體                                                                          |
| `auto: {on, do, …}`                                                                                                                                          | 一般事件能力；`on: {state: Cond}` 是 CR 10.7.6 狀態誘發，保留待機 latch                                                                                                     |
| `activate: {cost, do, usable_if?, quick?, advance?, fusion?, …}`                                                                                             | 起動；usable_if 在打出時查；fusion 的 sel/result 見 §13                                                                                                                     |
| `evolve: {cost, into?, usable_if?}`、`ride: {cost}`、`feed: {times?, cost}`                                                                                  | 進化相當能力入口、共用額度與 EP 代付規則；Meal／Ride 不捏造進化事件                                                                                                         |
| `spell: {targets?, optional_cost?, x?, define?, do}`                                                                                                         | 法術本文；空 do 合法不代表無需打出程序                                                                                                                                      |
| `static: {while?, zone?, effects}`                                                                                                                           | 宣告動態效果，while 每次求值；來源有效區限制只施於此類                                                                                                                      |
| `play: {cost_delta?, cost_set?, additional_cost?, optional_cost?, optional_cost_delta?, optional_cost_set?, alt_cost?, cannot_if?, playable_from?, define?}` | 改打出程序；必要追加、可選追加、替代費用與增減／set 分開                                                                                                                    |

**裁定修訂**：長寫「〜とき、…なら」也在事件發生時誘發，本文條件寫 `do: [{if: …}]`，不寫 `when`（R-0009 取代舊裁定 #1；R-0002 保留結算時條件範圍；R-0003 同樣適用關鍵字頭）。`when` 從 1.0 撰寫層移除，真正事件限定放 `on` 的型別化 filter，狀態誘發放 state。事件凍結資料可以在本文 if 讀，不代表條件曾在誘發時判一次。

`usable_if` 可查有型別歷史，在打出時依目前 this 的世代比對事件 subject。例如 **BP21-096／PR-579**：本回合這個從者曾有體力增加事實才可使用進化；進化基礎替換不算，超進化的明示 +1 算（Q2758／Q2759，IR S-008）。

```yaml
evolve:
  cost: { pp: 1 }
  usable_if:
    history:
      {
        event: stat_changed,
        where: { ref: this, stat: hp, direction: up },
        window: this_turn,
        at_least: 1,
      }
```

## 3. 費用、目標與打出程序

| 位置                                                                          | 表達                            | 時點與依據                                                                                        |
| ----------------------------------------------------------------------------- | ------------------------------- | ------------------------------------------------------------------------------------------------- |
| 起動／進化／憑依／食事、fanfare                                               | 頭的 `cost`                     | PlayParameter，CR 10.6.2.5、契約 36                                                               |
| evolve_time、super_evolve_time、attack_time、race_time、drive_time 後直接冒號 | 頭的 `cost`                     | PlayParameter，R-0001（generalized）；先選目標再付款，拒付或不能付不產生適正 play                 |
| lastword、一般長寫 auto 的冒號                                                | 本文 `pay: {cost, do, result?}` | ResolutionChoice，CR 10.4.7.3、契約 36                                                            |
| spell 本文／チョイス選項內冒號                                                | 本文 pay                        | ResolutionChoice，R-0006（inferred），選模式／目標／宣告 X 仍在打出時；不因付費誘發而在選項中插隊 |
| 明寫「プレイする際」、独立前綴土の秘術                                        | optional_cost                   | PlayParameter，CR 10.4.7.2、13.3.3.2；R-0005、R-0006                                              |
| 明寫必要追加費用                                                              | additional_cost                 | PlayParameter，optional=false（BP17-116）                                                         |

刪除頭的 `timing: pending|play|resolution` 與任填 `ruling` 就改時點的入口。例外需先登錄新裁定、更新 typed lowering；R-0001 不覆蓋選項內本文費用。`A.cost-timing-override` 僅是遷移診斷，不是可執行構造。

`A.target`：`targets: [{bind, from: Sel, n|upto|any, groups?, cost_sum_le?, distinct_by?, distinct_names?, option?, roles?}]`。選ぶ依語義規則收集為 D.play-param Targets，順序按卡文；本文普通 choose 永遠結算時。公開與否不能獨自決定選擇時點。`option` 僅在被選的チョイス模式有效。**刪除 `targets[].if`**：R-0004 普通條件不阻止選目標，R-0005 未支付土の秘術也不能免除目標要求。必要目標不足不能打出／選該模式；一般條件在本文查，SC／NC 另按捕捉規則。

`A.cost` 保留 v0 的 pp、ep、act_self、self_to_cemetery、banish_self、discard、banish、to_cemetery、return_to_hand、reveal、counter、flip_down、flip_up、leader_life、lesson、earth_rite、stack_counter_from；增加 §13 的 act、to_ex、to_deck_bottom、to_deck、pp_max、skip_turn、one_of。**標準長形為 `steps: [{<費用鍵>: payload}, …]`**，依文字順序付款；v0 短形鍵順序需由保序解析器保存，重複同鍵用 steps，不能默默覆寫。

選材料格式 `{sel, n|upto|any, groups?, distinct_by?, result?}`，top 切片不額外選身份。材料組 groups 共用 disjoint=true 與整體限制；不是逐組貪婪選。K.pay 全付或全不付；合法零費用、合法取代依 CR 10.6.2.5.2–.3 可算完整付款，但不製造原動作事件。

`optional_cost / additional_cost` 可帶 `result`，是已付款 receipt；付款前不得讀 paid。`x: {min?, max?}` 為 D.play-param X。可變材料數直接由材料選擇決定，不另問一次數字；`x: {from_payment: receipt}` 是付款量別名，與自由宣告 X 互斥，詳 §13 GAP-A-041。

## 4. 決定與控制流程

| 撰寫構造       | 欄位及 IR                                                                                                                                           |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `choose`       | `{bind, from, n\|upto\|any, by?, groups?, cost_sum_le?, distinct_by?, reveal?, order?, position?, may?}` → D.resolve-choice                         |
| `optional`     | `{who?, do, then?, result?}` → K.may；result 包含 Declined 或 body 最後指示 receipt，then 用該指示 typed did                                        |
| `pay`          | `{who?, cost, do, then?, result?}` → K.pay ResolutionChoice；付款與材料一次回答，then 若有亦受完整付款保護                                          |
| `choice`       | `{n\|upto, options: [Steps…], instead?, who?, exclude_chosen?}` → K.mode PlayParameter；模式按印刷順序執行，不走玩家輸入順序                        |
| `either`       | `{who?, options: [Steps…], bind?}` → K.mode ResolutionChoice；恰選一個，不是チョイス、不帶 mode 的 1..N 規則；可選整項在外層 optional               |
| `declare`      | `{bind, kind: number\|name, min?, max?, who?}` → I.declare＋D.resolve-choice；未設 max 的非負整數為符號域，不枚舉                                   |
| `if`           | `{cond, then, else?}` → K.if；條件管到段落／模式邊界，後續新的獨立條件開新 if（R-0007、Q1245／Q1246）                                               |
| `repeat`       | `{n\|until\|while, check?: before\|after, do, collect?: {receipts: name}}` → K.repeat；每輪作用域獨立，collect 為保序 receipt 列表                  |
| `foreach`      | `{in, as, do}` → K.each-object；只用於明示逐物件，不把「全體」改逐一                                                                                |
| `each_player`  | `{who?: each, stages: [Steps或{do,mode,evidence?}…]}` → K.each-player；mode=choice/nonchoice/explicit，階段橫跨玩家，選擇階段回合玩家先、非選擇同時 |
| `simultaneous` | `{do: Steps, result?}` → T.batch；只在卡文／CR 指定同時時使用，不允許步驟互讀尚未完成 receipt                                                       |
| `by_kind`      | `{ref, as?, leader: Steps, follower: Steps}` → K.match RecipientKind；各分支有窄化的 item 型別                                                      |

`each_player.mode` 省略時看降低後是否含玩家選擇（含原子內部容量／守護／捨牌／排列），無法證明則載入拒絕，不猜 nonchoice。`repeat_ability: {while}` 保留 v0 已有證據的本文重複（BP05-054），不創新 E.play；有目標的重複／每輪 E.resolve 與永久循環仍列未決，不任意截斷當成功。

輸入點唯一判準（契約 33）：先檢查明示 `precondition`／原定強制零單位；打出參數屬原 play（他玩家為 Handoff）；Quick 一律問；開始的結算選擇即使唯一、空集或只有 decline 仍問。卡文前提保護該指示，不向前吞掉別的指定；目的區滿本身不是前提。`precondition` → P.start，與普通 if 分開。

## 5. 表達式、捕捉與選擇器

沿用 `count: Sel / count_of / objects_of / bound / power_of / hp_of / cost_of / hand / leader_life / counter / distinct_names / distinct_orig_costs / dice / paid / x_paid / history_count / lki_of`；算術為 `plus / minus / times / half_up / per: {of, unit}`，二元運算用二元素陣列。增加 `resource / agg / min / max / event / turn_number` 與 `define`，見 §13。`cmp: {left, op: eq|ne|lt|le|gt|ge, right}` 比較同型值；`all / any / not` 組合條件。保留 `exists / on_field / in_zone / leader_life / hand / zone_free / my_turn / opp_turn / construction / standing / acted / evolved / chosen / did / entered_from / played_from` 與 combo、spellchain、necro、awakened、crimson。

`define: {X: Expr, …}` 是同一能力內的**純表達式別名**，在每個使用點展開、依該 read 的時點讀；不是 entry 時偷偷 freeze。一般卡文定義 X 為 Live（已同意第 18 族），事件／receipt 值本身固定。自定義 X 與頭的自由 x 不可同名；無限循環定義、型別不明定義載入拒絕。同一步使用 X 多次從同一指示開始視圖讀，跨步驟則重新求值。

| 捕捉         | 規則                                                                                                            |
| ------------ | --------------------------------------------------------------------------------------------------------------- |
| Live         | 在使用該值的步驟讀，普通條件及卡文定義 X；「それのリーダー」當下 controller，離場依法 LKI（第 19 族、CR 10.11） |
| AtPlay       | 明示打出參數／SC、NC 決定模式數／費用；不是未付款 receipt                                                       |
| AtResolution | SC、NC 作結算門檻時在該 execution 入口固定；一般 if 不套這個預設                                                |
| AtSelection  | 「選んでいたなら」讀指定目標選定時特徵                                                                          |
| AtEvent      | event.field、事件材料／來源快照；R-0009 的本文判斷仍能讀這些值                                                  |

`at: live|play|resolution|selection|event` 只能用登錄允許且有語義依據的組合，不是自由改規則。條件求值階段與值的捕捉時間分開；一個複合式可有多種 read，但分類資料不必因已廢止的「長寫條件查兩次」增設 capture 清單（第 20 族結案；遷移見配對文件）。

`item` 是保留的局部名稱：`Sel.where` 求值時綁定目前候選元素，型別為該 Sel 的元素型別，只在該 predicate 有效；`by_kind` 分支則綁定窄化後的接受者。禁止巢狀 where／by_kind 隱式遮蔽 item；需要巢狀時以 `Sel.as: name` 或 `by_kind.as: name` 明確命名，該層不再引入 item。別名與一般 bind 同樣不得重名，離開 predicate／分支即失效，外層 item 仍可明確讀取。

`Sel` 欄位沿用 `who`、`zone`、`kind`、`class/not_class`、`trait/traits_any/not_trait`、`name/names/name_contains/not_same_name_as_this`、`other`、`cost_*/orig_cost_*`、`hp_le`、`power_max`、`acted`、`evolved`、`keyword`、`has_trigger/has_evolve`、`not_token`、`face_up`、`leader/leader_or/leader_of`、`in/rest`、`entered_this_turn`、`any_of`、`except`、`where`；增加 as（局部元素別名）、advance_follower、boxed、racing、同型／同名引用與頂底切片。`in` 僅接物件 binding，receipt 必須先用 objects_of（或 count_of.where 在材料快照篩選）。未知 filter 不當 no-op。

## 6. 玩家、引用、結果

`self / turn / non_turn / current / owner_of / master_of` 為 PlayerRef；`opp` 在單人位置以 RequireOne(Opponents) 取得唯一席位；`opponents / each / other_players` 保留 PlayerSet 與原文基數。執行目前僅二席；「相手すべて」不因此抹去集合。`leader: self|opp`、`leaders: opponents|all` 是領袖 recipient；集合動作保留批次。

`this / it / bind / first:bind / second:bind / rest:bind / attack_target / host / cost1` 保留 R.object 世代或事件快照。first/second 必須來自有角色或明示順序的 binding，無序集合不可任取第一張。目標引用 R.target 在結算驗合法；LKI 可讀不代表能對離場物件執行。`follow: receipt` 只接具合法移動授權的實際 after refs（R.follow-move），失敗或消去成員不回傳可操作物件。合法 follow 若沒有成功成員，後續對該集合的動作依 CR 1.3.2 不做，不是載入錯誤；未綁定或無授權的 follow 仍應拒絕。

`result` 可掛所有動作、費用、simultaneous。`count_of` 是指定單位 actual；`objects_of` 預設成功成員的快照，`follow` 才取得可執行的新區引用。`count_of: {receipt, where, view: before|after}` 用有型別材料快照，無資訊按 CR 1.3.6。`did`：卡片移動／建立／公開／抽／找須至少一個實際成功成員；傷害須實際正量；姿態／翻面／counter 需所要求全數實變；pay 是 PaymentComplete；evolve 是 receipt.evolved=true。非預設語意必須使用明示 predicate 並附依據，不通用地用 actual>0。

## 7. 動作原子

共通欄位 `result? / precondition?`；`by` 為執行者（預設 self），`who` 為接受資源／抽捨牌的玩家，`to` 的區域持有者是目的地角色。不可互相取代。具體新增欄位見 §13；下表保留 v0 全部動作族。

| 鍵                                     | payload                                                                              | 降低成                                                                               |
| -------------------------------------- | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------ |
| draw / discard                         | `{who?, n}`；discard 另有 all/down_to/any/sel/random                                 | I.draw / I.discard；逐張抽不是批次搬牌                                               |
| damage / destroy / banish              | `{to,n,distribute?,distribute_at?,instead?}`；後兩者 Ref 或 `{ref}`，banish 另接 top | I.damage / I.destroy / I.banish；distribute 預設打出時                               |
| move                                   | `{ref或top,to,position?,order?,reveal?,by?,acted?,entry_effects?,bind?,result?}`     | I.move；位置決定與順序 I.order，洗指定集 I.shuffle                                   |
| create                                 | `{token,n,to,counters?,by?,acted?,entry_effects?,bind?,result?}`                     | I.token；token 列表為每名各 n，不是擇一；不寫 n 時每名 1                             |
| act / stand                            | Ref 或 `{ref}`                                                                       | I.posture；已在該姿態不產生變化                                                      |
| evolve / transform                     | `{ref,into?,result?}`；transform 另接 `entry_effects?`                               | I.evolve Effect / I.transform；變身不是改卡名，into 分配見 §14 A-016j                |
| life / set_life                        | `{who,delta或n}`                                                                     | I.life Add / Set；生命改變不等於傷害                                                 |
| buff / set_stat                        | `{on,power?,hp?,until?,instead?}`                                                    | I.modify＋L.continuous Numeric                                                       |
| grant / lose_abilities                 | `{on,keywords?,abilities?,until?}`                                                   | I.grant / I.remove-ability；刪除舊 grant.effects，以 apply 安裝效果                  |
| counter / move_counters                | `{on,name,delta}` / `{from,to,name,all?}`                                            | I.counter Add/Remove/Move                                                            |
| pp / pp_max / ep / sep                 | `{who,delta}`；pp 另有 `{who,to_max:true}`                                           | I.resource；不更改印刷費用                                                           |
| reveal / look                          | `{ref或top,bind,result?,by?,random?}`                                                | I.reveal / I.look；random 先以 I.random 抽樣，不讓玩家挑                             |
| reveal_until                           | `{sel,n,bind,matched}`                                                               | I.reveal-until；頂端逐張、達標或耗盡停止                                             |
| search                                 | `{sel,n或upto,to,bind?,groups?,cost_sum_le?,distinct_by?,entry_effects?,result?}`    | I.search；找牌／證明條件／搬牌／洗牌，分量結果保存                                   |
| shuffle / order                        | `{who}` / `{ref,to,by?}`                                                             | I.shuffle FullZone / I.order                                                         |
| dice / declare                         | `{bind,who?}` / §4                                                                   | I.dice / I.declare；骰值非玩家宣告                                                   |
| flip_down / flip_up                    | Ref 或 `{ref}`                                                                       | I.flip-down / I.flip-up；進化牌庫表裏，不是 EP 資源                                  |
| equip                                  | `{on,token}`                                                                         | I.equip；裝備物不占普通場格                                                          |
| steal / give                           | `{ref,to?,precondition?}`                                                            | I.transfer；場間轉移同世代、不生入場事件                                             |
| stack_plus                             | Expr                                                                                 | K.if＋D.rule-choice StackRecipient＋I.counter 或 I.token 初始 N 個 counter（非 N+1） |
| box / pilot                            | `{ref}` / `{ref,on}`                                                                 | I.box-pilot；依 CR 5.31/5.32 的能力及關聯展開                                        |
| race / feed / gain_drive / drive_check | `{ref?,n?,who?}`                                                                     | I.meal-race / I.ride-drive / I.drive-check 的對應規則程序                            |
| play_card / play_ability               | `{ref,cost?}` / `{of,trigger?,ability?,cost_policy?}`                                | I.play-card / I.play-ability；內層獨立 execution                                     |
| delayed                                | `{on,do,next_only?,until?,line}`                                                     | I.delay；無期限預設只誘發一次                                                        |
| player_effect / apply                  | `{who,effect,until?}` / `{on,effect,until?}`                                         | I.install-effect，分玩家與物件範圍；不是 I.grant                                     |
| win / skip_turn / extra_turn           | `{who}`                                                                              | I.game-result / I.schedule-turn                                                      |
| make_type                              | `{on,type,until?}`                                                                   | I.modify SetKind；不隱式失去能力                                                     |

## 8. 事件與歷史

`A.event` 保留 enter_field、left_field、placed_ex、discarded、banished、destroyed、damaged、dealt_damage、attacked、attack_target、evolved、super_evolved、phase、played、ability_played、life_changed、stat_changed、counter_changed、acted、stood、raced、drive_gained、ub_activated、fused、targeted、state。新增 dice_rolled、drive_checked、drive_trigger、moved、drew、equipped、fused_as_material；any_of 合併多種事件但仍是一條能力／限次。payload 必須按事件型別窄化，不可讀不存在欄位。

共同事件 filter `by: {ability_of: Sel}` 判直接來源；入場加 entered_by，UB 加 holder，傷害加 recipient，融合加 material，drew 加 not_phase，counter_reached 為 counter_changed 的 after 比較。`stat_changed: {ref,stat: power|hp或清單,direction:up|down}` 對應 Attack/Life，同一指示對同一主體的攻體增減保存為一個事實的多個 facet，不重複計（`strength: inferred`；Q2075 只證明超進化會誘發，未證明次數）。stat_changed 的 history.where 同樣只用 ref／stat／direction：ref 降為 IR subject，stat 的 power／hp 降為 StatDelta.property 的 Attack／Life；撰寫層不接受 subject／property 別名。從者事件只由明示數值指示或超進化 CR 12.2.4.1 的 +1 步驟產生；進化基礎替換不產生（Q2758），超進化產生（Q2759）。持續效果條件變化、期限到期、種類還原與快取重算不產生，標 `strength: inferred`；領袖的傷害／回復／生命支付仍保留獨立來源。

`history: {event,who?,zone?,from?,to?,sel?,where?,window:this_turn|game|since:<錨>,at_least?,distinct?:facts|subjects,exclude?}` 是條件；`history_count` 回數量；`history_sum: {… ,field}` 回 typed payload 總量。事件鍵共用上表（骰子統一 dice_rolled）；where 只接該事件登錄欄位。exclude=this_execution 排除本次 AbilityExecutionId，不排除同卡過去執行或外層執行。UB 在適正打出完成的 E.play 記一次發動，內層查詢已可見外層；另見 IR §13 GAP-S-015。所有歷史保存事件當時特徵，不查目前卡片；內部歷史身份不洩漏隱藏牌固定 ID。

## 9. 效果、取代與期限

`A.effect` 可用於 static.effects、player_effect.effect、apply.effect；apply.on 預設在指示開始捕捉集合，可依明確證據用 capture:play，static selector 隨狀態求值。保留 stat/set_stat、keywords/grant/lose_abilities、also_named/add_trait/remove_trait/make_type、cost_mod、damage_mod、banish_instead、cannot、must_attack、play_permission、no_trigger、unlimited_evolve、rule_override；補 prevent、damage_cap、evolve_cost_mod、enters_acted、reroll、ignore_ward、choice_any、extra_trigger、must_target。各鍵只接受配對 IR 封閉變體；無語義的 boxed 宣告改本文 box。

`cannot: {on或who, what, by?, target?, card?, ability?, during?}`；card 是 play_card 專用 CardPredicate，不能套到 draw；what=play_card/play_ability/attack/choose_target/draw/destroy/banish/stand/win/lose/deal_damage/pp_max_increase。deal_damage 降 Damage.Prevent source；stand 的 during 精確區分自己的規則起身／所有起身。no_trigger 抑制待機，cannot.play_ability 阻止打出，兩者不能互換；本次入場 fanfare 限制需在入場判定前安裝（§13 GAP-A-044）。

`cannot.attack.target` 的「相手を攻撃できない」標準為 `{who: opp, kind: follower, leader_or: true}`，同時涵蓋對手從者與領袖（BP04-SL08、Q581）；CSD03b-014 使用同一述詞。只有卡文明寫「相手のリーダー」才縮成 `{leaders: opponents}`。

`prevent: {on?,kind:any|combat|ability,source?}`；`damage_mod: {on?,source?,kind?,delta或multiply,next_only?}`；`damage_cap: {on?,threshold,set}` 在 amount>=threshold 時改為 set，不默認 min；`cost_mod: {when_playing,delta或set,next_only?}`；`evolve_cost_mod: {on,delta或set}`；`ignore_ward: {on}`；`reroll: {who,times}`；`choice_any: {who}`。期限與使用額度分開。`cost_mod.when_playing.in` 接 ObjectSet 或 `{follow: ReceiptId|ReceiptList}`；於效果安裝時保存獲授權的新世代集合，不因之後同卡號入場而新增成員。`player_effect` 的 next_only 共用一個 UsageId，整組下一次符合的適正 play 消耗一次；`apply` 對各物件安裝時則各有 UsageId。付款交易失敗須回復額度，普通費用預覽不消耗。BP14-046 使用前者，不能每張各免費一次（依日文語法與 CP04-003 同句型推論，無直接 Q&A；`strength: inferred`）。

`enters_acted:{on}` 是入場姿態取代。自身 static 在 ProspectiveEntry 以目的區資訊求值（CR 10.9.3），因此預設 active_zones=field 仍能在入場當下生效，毋須擴成手牌／墓地皆有效；不產生額外 act 事件。

`instead: [{if, <動作 payload 差異>, after?: Steps}]` 可掛 damage/move/buff/life/grant/choice/create/draw/search/banish/destroy。同種差異保留未改欄位，異種用 `replace_with: {<另一動作>: payload}`。降低為一次自我取代，不是先做原動作；after 是取代成功適用後同條件段落的後續句，R-0007 不允許把它放成無條件下一步。choice 只改模式數的形式在打出時計算，不裝全域攔截器。

Until 保留 turn、next_opp_turn、this_and_next_opp_turn、game；明確長形 `{next_phase: {who,phase:start|main|end,edge:begin|end}}`、`{next_turn: {who,edge:begin|end}}`；`{during_next_phase:{who,phase}}` 表示完整下次階段區間（BP05-006），不是到開始即到期。who 在安裝時捕捉（master_of 與 owner_of 不可混淆）。舊無參數 next_start_phase / next_main_phase / next_owner_turn_end 遷移時要求補錨點，不猜。

省略期限＝NoTemporalExpiry，但目標世代換區即依法失效；EX→解決→場僅 RuleCarry 白名單。本文效果不因來源離場自行失效；static 受來源有效區與 while 限制。依存與事件邊界見 [ADR-0008](../adr/0008-continuous.md)。

## 10. 巨集規範

保留 v0 dig（look→choose→move→餘牌排列）、discount（install cost_mod）、search_to（search）與誘發頭巨集。模式數／目標／費用參數由明示構造收集，不由巨集在文字中的位置猜測。巨集 ≥3 個不同卡名才建立，2 張換參數、1 張單寫；170 個候選仍需兩模型各審與測試，不能此稿一次宣布全部合格。

每個巨集記撰寫者、審核者、版本；測試含 0 張、無合法目標、大數值、時點／資訊邊界。每張套用卡必須句型完全相符、數字／卡名參數相符、反向翻譯相符，才能 reviewed；實跑前不 verified。修巨集後依版本反向索引重檢所有套用卡。

## 11. 載入與錯誤

輸入固定為 **YAML 1.2 core schema**，單一文件；省略版本指示也按 1.2，明示 `%YAML 1.1` 必須拒絕。plain `on`／`off`／`yes`／`no`／`n` 是字串，只有 core schema 定義的 true／false 大小寫形式是布林；日期不是隱式 timestamp。所有 mapping 鍵必須是字串，布林、數值、null、集合鍵均拒絕；不可先轉字串再驗，亦不可容許 true／false 字串鍵冒充構造。重複鍵須在資料被覆蓋前拒絕。

每個鍵必須在**目前構造上下文**的登錄表：例如 cannot 沒有 n，buff 有 on；不能以另一構造曾使用 n 就放行。cards、define、roles 的動態名稱須符合其專屬卡號／識別字規則，不是任意額外欄位。未知鍵附卡號、行與完整路徑報錯。event 與 history.where 共用該事件的欄位表。

Python 載入邊界必須明確限定 YAML 1.2 core scalar resolver／標籤，不能只依函式名稱推定符合 core。`carddb` 的 authored 讀取採 PyYAML `CSafeLoader`（libyaml C 擴充）單趟事件檢查，沿用既有 ruamel 的 1.2 core 純量規則；缺 C 擴充即失敗，寫出維持 ruamel。其他既有 Python DSL 工具維持 `ruamel.yaml` 的 `YAML(typ="safe", pure=True)`、版本 `(1, 2)`、`allow_duplicate_keys=False`。禁止 PyYAML 的預設 1.1 resolver；BaseLoader 全讀字串也不能替代 core 型別檢查。Rust 選 saphyr 的 YAML 1.2 路線，經節點／事件邊界先檢查鍵型別、重複鍵、版本與 core tag，再轉有型別資料；不能先轉成會覆蓋重複鍵的 map。解析器選型與文件見 ADR-0002。

兩端載入器必過同組金絲雀：含 on、n、yes／no 值的最小卡片保留字串與數字；true 值仍為布林；布林鍵、字串 true 鍵、重複 on、未知鍵、cannot.draw.n、stat_changed 歷史中的 subject／property 舊別名均拒絕。設計階段只以 Python 端跑過這組檢查；Rust 接入時必須重跑，不能沿用 Python 結果宣告 Rust 已驗收。

依序做上述 YAML 版本／core 型別／鍵型別／重複鍵／構造鍵名檢查、schema 版號、卡表／source hash、欄位／registry 型別、binding scope、捕捉時點、receipt 可用性、目標／費用時點、效果安裝型別、引擎 capability；任一失敗都不能進入自動結算。正向 YAML 語法檢查不能替代此流程。禁止一般字串 selector、缺值回 0、未支援欄位默默忽略。詳 [ADR-0002](../adr/0002-author-ir.md)、[ADR-0010](../adr/0010-tests.md)。

## 12. 狀態與證據

R-0001～R-0009 的效力、strength、取代關係以裁定登錄為準，完整對照在 IR §0。第 9～17 族本文件補語法；第 18、19 族統一 capture，第 20 族因 R-0009 不再需要兩次條件求值而結案，第 21 族以遷移稽核處理。沒有改寫 v0 標註。每族是否能提升「兩層通過」須聯合檢查該句型所有缺口，不可加總重疊族行數。

## 13. GAP-A 逐族補法與真實卡例

下列 YAML 是指定能力行的最小表達，非整卡驗收產物；`line`／meta 由上層容器補入。例子中的卡名、數字以所列來源行為準；同族其他變體由欄位型別覆蓋，不以一張例子代替全族引擎驗證。

### GAP-A-001

define 是表達式別名，在 draw 開始讀牌庫數（Live）；負數所要求的動作按 CR 1.3.2.2 不做，不先強轉 Nat。Q1076。

真實卡例：**BP08-038 無貌の魔女**；來源 `BP08-038#0/text/1`，句型 `T4acb7d7328`。

> {ファンファーレ}X枚引く。Xは「自分のデッキの枚数-5」である。

```yaml
fanfare:
  define: { X: { minus: [{ count: { who: self, zone: deck } }, 5] } }
  do:
    - draw: { n: X }
```

### GAP-A-002

列表表示各建一張，保留列表順序／重數；token 名決定型別為 CardName。I.token.tokens 是 multiset，不是模式選擇。

真實卡例：**BP09-SL05 絶望の使者・セリア**；來源 `BP09-SL05#1/text/1`，句型 `T32d588f527`。

> 【進化時】『スティールナイト』1体と『ナイト』1体を出す。

```yaml
evolve_time:
  do:
    - create: { token: [スティールナイト, ナイト], n: 1, to: field }
```

### GAP-A-003

act 材料接受 Sel／Cardinality；降低 D.play-param CostMaterials＋K.pay＋I.posture。

真實卡例：**BP11-001 開拓のロデオガイ・ロキサス**；來源 `BP11-001#0/text/2`，句型 `T064cbd6279`。

> {起動}場のアミュレット3つを{アクト}：1枚引く。

```yaml
activate:
  cost: { act: { sel: { who: self, zone: field, kind: amulet }, n: 3 } }
  do: [{ draw: { n: 1 } }]
```

### GAP-A-004

either 明確在結算時選一路，選完才建立 token；不產生不存在 token 的 ObjectId。

真實卡例：**SD03-016 ゴーレムの錬成**；來源 `SD03-016#0/text/1`，句型 `Tacbc87e742`。

> 『防御型ゴーレム』1枚か『攻撃型ゴーレム』1枚をEXエリアに置く。

```yaml
spell:
  do:
    - either:
        options:
          - [{ create: { token: 防御型ゴーレム, n: 1, to: ex } }]
          - [{ create: { token: 攻撃型ゴーレム, n: 1, to: ex } }]
```

### GAP-A-005

resource.kind 封閉為 pp_max/pp/ep/sep，回 Int；一般條件 Live。pp.to_max 降 RecoverToMax。

真實卡例：**BP21-SL28 駆動の領域・グレティナ**；來源 `BP21-SL28#0/text/0`，句型 `Tca82225beb`。

> 自分のSEPが0である限り、これは【疾走】を持つ。

```yaml
static:
  while:
    { cmp: { left: { resource: { who: self, kind: sep } }, op: eq, right: 0 } }
  effects: [{ keywords: { on: this, list: [storm] } }]
```

### GAP-A-006

move.result → follow 只取實際成功 after refs，apply 捕捉它們；原例未寫期限，故不擅加 turn。下文另列 set 0 完整例。

真實卡例：**ETD01-005 騎竜兵**；來源 `ETD01-005#0/text/0`，句型 `T328d04a2a4`。

> {ファンファーレ}自分のデッキの上1枚をEXエリアに置く。それをプレイする際、コストを-2する。

```yaml
fanfare:
  do:
    - move: { top: { who: self, n: 1 }, to: ex, result: moved }
    - apply:
        on: { follow: moved }
        effect: { cost_mod: { when_playing: { in: affected }, delta: -2 } }
```

### GAP-A-007

新增 A.act.apply → I.install-effect(Objects)；affected 是安裝期間的捕捉對象。失去能力不移除此效果。

真實卡例：**BP17-018 不屈のブレイブフェアリー**；來源 `BP17-018#0/text/1`，句型 `T1919d5e509`。

> 【攻撃時】このターン、これはダメージを受けない。

```yaml
attack_time:
  do:
    - apply: { on: this, effect: { prevent: { kind: any } }, until: turn }
```

### GAP-A-008

cannot.what=stand；during 錨定 self 的 start phase 規則起身，非全時段禁止效果 stand。IR Stand(scope=RuleStart)。

真實卡例：**ECP01-053 〔月下の悪魔ちゃん♪〕メジロパーマー**；來源 `ECP01-053#0/text/2`，句型 `Tb77a58afaf`。

> これは自分のスタートフェイズにスタンドしない。

```yaml
static:
  effects:
    - cannot:
        {
          on: this,
          what: stand,
          during: { phase: start, who: self, cause: rule },
        }
```

### GAP-A-009

create 的 instead 保留自我取代優先權，不能改成先建立再補一張。

真實卡例：**ETD01-018 ドラゴニックアーマー**；來源 `ETD01-018#0/text/0`，句型 `T79a83fa8d0`。

> 『ドラゴウェポン』1つを出す。【覚醒】状態なら、代わりに2つ。

```yaml
spell:
  do:
    - create:
        token: ドラゴウェポン
        n: 1
        to: field
        instead: [{ if: { awakened: true }, n: 2 }]
```

### GAP-A-010

top:{who,n} 是保序牌庫切片，banish 與費用 banish 都可用，未公開身分由引擎處理。

真實卡例：**BP21-047 カースドソーサラー・リーズ**；來源 `BP21-047#0/text/0`，句型 `Tc00b28ac32`。

> {ファンファーレ}自分のデッキの上1枚を消滅させる。

```yaml
fanfare:
  do: [{ banish: { top: { who: self, n: 1 } } }]
```

### GAP-A-011

top 明定來源玩家；目的地依卡片 owner，by 預設執行能力者。look 同樣接受 who。

真實卡例：**BP06-028 鮮やかな奪取**；來源 `BP06-028#0/text/1`，句型 `T7c3b9072aa`。

> 相手のデッキの上3枚を墓場に置く。

```yaml
spell:
  do: [{ move: { top: { who: opp, n: 3 }, to: cemetery } }]
```

### GAP-A-012

補 to_ex、to_deck_bottom、to_deck:{shuffle:true}，均降低 I.move；shuffle 指整副牌庫，不是僅洗材料。

真實卡例：**BP14-008 仲居のエルフ**；來源 `BP14-008#0/text/1`，句型 `T450443faa5`。

> {ファンファーレ}手札1枚をデッキの下に置く：自分のリーダーは{体力}+2する。

```yaml
fanfare:
  cost: { to_deck_bottom: { sel: { who: self, zone: hand }, n: 1 } }
  do: [{ life: { who: self, delta: 2 } }]
```

### GAP-A-013

entered_by:{ref,by:ability|play} 讀當次入場事實；E.enter-field.cause 與 entry_method 分開，效果中 play 仍為 play。delayed.on 同 filter。

真實卡例：**BP17-SL25 実りの参謀・ムニャール**；來源 `BP17-SL25#0/text/2`，句型 `Tb81e00e86b`。

> {ファンファーレ}これが能力によって場に出ていたなら、1枚引く。

```yaml
fanfare:
  do:
    - if:
        cond: { entered_by: { ref: this, by: ability } }
        then: [{ draw: { n: 1 } }]
```

### GAP-A-014

ub_activated.holder 以發動時 holder／場上快照過濾，不用 controller 代替物件。

真實卡例：**CP04-SL15 ホマレ**；來源 `CP04-SL15#0/section:0/0`，句型 `T92e6f007a4`。

> 自分の場の他のフォロワーの{UB}能力が発動したとき、自分のPP最大値を+1する。

```yaml
auto:
  on:
    {
      ub_activated:
        {
          who: self,
          holder: { who: self, zone: field, kind: follower, other: true },
        },
    }
  do: [{ pp_max: { who: self, delta: 1 } }]
```

### GAP-A-015

move/create.acted:true → initial_posture=Acted；static enters_acted 是 Move.EntryPosture 取代，沒有站立→橫置的中途 E.posture。

真實卡例：**BP11-112 ジャイアントマッチ**；來源 `BP11-112#0/text/0`，句型 `T8872dde0e9`。

> これはアクト状態で場に出る。

```yaml
static:
  effects: [{ enters_acted: { on: this } }]
```

### GAP-A-016

逐項補法見 §14 的 11 個子項，不能只憑這個抽牌篩選例關閉整族。during.not_phase 只篩選 Draw，不改其他取得手牌動作。

真實卡例：**BP10-SL20 《節制》・ルーゼン**；來源 `BP10-SL20#0/text/1`，句型 `T7122153437`。

> これが場にいる限り、相手プレイヤーすべては、スタートフェイズ以外でカードを引けない。

```yaml
static:
  effects:
    - cannot: { who: opponents, what: draw, during: { not_phase: start } }
```

### GAP-A-017

create.to 接 {one_of:[field,ex],by,scope:each|batch}；每張分配與整批選一路分型，均 ResolutionChoice。

真實卡例：**BP17-SL06 光耀の標・ミストリナ＆ベイリオン**；來源 `BP17-SL06#0/text/1`，句型 `T0456012426`。

> {ファンファーレ}『ナテラの大樹』1枚を場かEXエリアに置いてよい。

```yaml
fanfare:
  do:
    - optional:
        do:
          - create:
              {
                token: ナテラの大樹,
                n: 1,
                to: { one_of: [field, ex], by: self, scope: each },
              }
```

### GAP-A-018

材料 groups 不重用，手牌 this 明確包括自身；不同區域不能偷把解決區中的自身當手牌。

真實卡例：**SD08-017 オーブキャンサー**；來源 `SD08-017#0/text/1`，句型 `T0ddcddd1de`。

> {起動}{コスト1}手札のこれと元のコスト7以上の{ドラゴン}カード1枚を捨てる：2枚引く。

```yaml
activate:
  zone: hand
  cost:
    steps:
      - pp: 1
      - discard:
          groups:
            - { sel: { in: this }, n: 1 }
            - {
                sel:
                  { who: self, zone: hand, class: ドラゴン, orig_cost_ge: 7 },
                n: 1,
              }
  do: [{ draw: { n: 2 } }]
```

### GAP-A-019

deck_rule 指定 deck:main|evolve 與 same_name_max；不把進化牌庫的 10 張套到主牌庫。

真實卡例：**ECP01-058 開催大成功！**；來源 `ECP01-058#0/text/0`，句型 `T783169f7b6`。

> これはエボルヴデッキに10枚まで入れることができる。

```yaml
deck_rule: { deck: evolve, same_name_max: 10 }
```

### GAP-A-020

leaders:all 是包括自身與所有對手的 DamageRecipient 集合，同一 I.damage 批次。

真實卡例：**BP06-P23 ベアーベルセルク**；來源 `BP06-P23#0/text/1`，句型 `Te49f7fdc4b`。

> 【攻撃時】リーダーすべてに1ダメージ。

```yaml
attack_time:
  do: [{ damage: { to: { leaders: all }, n: 1 } }]
```

### GAP-A-021

destroy/banish 的 by=ability 判直接 cause；體力歸零的規則破壞仍可發生。

真實卡例：**BP19-SL15 禁牙の執行者・ドラズエル**；來源 `BP19-SL15#0/text/3`，句型 `T2ad29027f2`。

> これは能力によって破壊されない。

```yaml
static:
  effects: [{ cannot: { on: this, what: destroy, by: ability } }]
```

### GAP-A-022

any_of 可列任意合法事件；stat 清單是同事件 payload 多選，合併能力身分／限次；同一次攻體俱增的事實去重。事件邊界仍見 ADR-0008。

真實卡例：**BP11-P27 レヴィールの無法者**；來源 `BP11-P27#0/text/1`，句型 `T65bea14e0b`。

> これの{攻撃力}か{体力}を+したとき、これは【疾走】を持つ。

```yaml
auto:
  on: { stat_changed: { ref: this, stat: [power, hp], direction: up } }
  do: [{ grant: { on: this, keywords: [storm] } }]
```

### GAP-A-023

cannot.attack.target 可為 leader/follower/Sel；領袖限制不擋攻擊從者。

真實卡例：**BP13-024 生還の突撃兵**；來源 `BP13-024#0/text/1`，句型 `T2cf6acc174`。

> これは相手のリーダーを攻撃できない。

```yaml
static:
  effects:
    [{ cannot: { on: this, what: attack, target: { leaders: opponents } } }]
```

### GAP-A-024

is:{ref,kind} 窄化事件引用；attack_target 為凍結事件目標，本文條件依 R-0003 判。

真實卡例：**BP01-SL16 ムーンアルミラージ**；來源 `BP01-SL16#0/text/1`，句型 `T474d229c66`。

> 【攻撃時】フォロワーへの攻撃なら、これは{攻撃力}+2する。

```yaml
attack_time:
  do:
    - if:
        cond: { is: { ref: attack_target, kind: follower } }
        then: [{ buff: { on: this, power: 2 } }]
```

### GAP-A-025

evolve_cost_mod 用 PlayAbility(Evolution).Cost set/delta；dynamic count 在費用計算時讀，非改印刷 cost。

真實卡例：**SP01-012 音速の機構・ララミア**；來源 `SP01-012#0/text/1`，句型 `T24cd9af7dd`。

> これの{進化}コストは「自分の場の他のフォロワーの数」と同じだけ-1する。

```yaml
static:
  effects:
    - evolve_cost_mod:
        on: this
        delta:
          {
            minus:
              [
                0,
                {
                  count:
                    { who: self, zone: field, kind: follower, other: true },
                },
              ],
          }
```

### GAP-A-026

search 共用 SelectionConstraint，總原費用以找到的成員算，找不到仍按 search 規則收尾。

真實卡例：**BP18-P10 俯瞰の捜査員**；來源 `BP18-P10#0/text/1`，句型 `Tf6a43e66dd`。

> 【進化時】自分のデッキから透京・フォロワーを元のコストの合計が3以下になるように2枚まで探し、場に出す。

```yaml
evolve_time:
  do:
    - search:
        sel: { who: self, zone: deck, kind: follower, trait: 透京 }
        upto: 2
        cost_sum_le: 3
        to: field
```

### GAP-A-027

drive_trigger:{who} → E.drive-trigger，指該玩家真正執行 trigger 效果；不是每次揭牌。原文未限制 checked_card=this，不加此 filter。

真實卡例：**CP03-P81 オラクルガーディアン ニケ**；來源 `CP03-P81#0/text/1`，句型 `T9177744e95`。

> 自分のドライブチェックによってトリガーしたとき、これは【疾走】を持つ。

```yaml
auto:
  on: { drive_trigger: { who: self } }
  do: [{ grant: { on: this, keywords: [storm] } }]
```

### GAP-A-028

agg:{of,attr,fn:sum|max|min}，空 sum=0，空 min/max 為 AbsentByRules；可用 min/max:[Expr…]，空陣列拒絕。

真實卡例：**BP04-070 ケートス**；來源 `BP04-070#0/text/0`，句型 `T151ea50320`。

> 【進化時】相手の場のコスト最小のフォロワーすべてを破壊する。

```yaml
evolve_time:
  define:
    lowest:
      {
        agg:
          {
            of: { who: opp, zone: field, kind: follower },
            attr: cost,
            fn: min,
          },
      }
  do:
    - destroy:
        {
          ref:
            {
              who: opp,
              zone: field,
              kind: follower,
              where:
                {
                  cmp:
                    {
                      left: { cost_of: item },
                      op: eq,
                      right: { bound: lowest },
                    },
                },
            },
        }
```

### GAP-A-029

history.where 是 typed payload 條件；stat_changed 的 ref 相等比較包含世代（IR subject），不接受撰寫欄位 subject。history_sum.field 明列 amount/delta/removed 等可加總欄位，不能 count 代 sum。

真實卡例：**BP20-P37 吹雪のドラゴニュート**；來源 `BP20-P37#0/text/0`，句型 `T4fa538d54e`。

> 【進化時】「このターン中にダメージを受けた相手の場のフォロワー」すべてを破壊する。

```yaml
evolve_time:
  do:
    - destroy:
        ref:
          who: opp
          zone: field
          kind: follower
          where:
            history:
              {
                event: damaged,
                where: { recipient: item },
                window: this_turn,
                at_least: 1,
              }
```

### GAP-A-030

dealt_damage.recipient 接 DamageRecipient selector；kind=attack 與 combat 可重疊，不改成 ability。

真實卡例：**BP04-003 深き森の異形**；來源 `BP04-003#0/text/1`，句型 `T165d4ce33c`。

> これが相手のリーダーへの攻撃ダメージを与えたとき、自分はこのバトルに勝利する。

```yaml
auto:
  on:
    {
      dealt_damage:
        { ref: this, kind: attack, recipient: { leaders: opponents } },
    }
  do: [{ win: { who: self } }]
```

### GAP-A-031

counter.on 接 Sel，須明確選定一個 holder 再取 counter，不能默認 this；result 保留選到的 holder。

真實卡例：**BP14-063 竜山の鳴動**；來源 `BP14-063#0/text/0`，句型 `T9059c4c9cd`。

> これをプレイする際、場の『竜山温泉』1つの神湯カウンター2個を取る：コストを-2する。

```yaml
play:
  optional_cost:
    counter:
      {
        on: { who: self, zone: field, name: 竜山温泉 },
        name: 神湯,
        n: 2,
        holders: 1,
      }
  optional_cost_delta: -2
```

### GAP-A-032

move/create.by 接 PlayerRef；和目的地、owner 不同；master_of:t 於該步讀，離場用 LKI。

真實卡例：**CP02-010 相葉夕美**；來源 `CP02-010#0/text/0`，句型 `T83e340332e`。

> 【進化時】体力3以下の相手の場のフォロワー1体を選ぶ。それのプレイヤーはそれをデッキの下に置く。

```yaml
evolve_time:
  targets:
    [
      {
        bind: t,
        from: { who: opp, zone: field, kind: follower, hp_le: 3 },
        n: 1,
      },
    ]
  do: [{ move: { ref: t, to: deck, position: bottom, by: { master_of: t } } }]
```

### GAP-A-033

exclude:this_execution 只排本次呼叫，Q2390／Q2391；instead 條件 Live 查詢，外層已進入結算的 UB 資格可見。

真實卡例：**CP04-P60 レイ**；來源 `CP04-P60#0/text/0`，句型 `T731ab48948`。

> {UB}【攻撃時】これは{攻撃力}+1する。これを含めず、このターン中に自分の{UB}能力が2回以上発動していたなら、代わりに{攻撃力}+2する。

```yaml
attack_time:
  ub: true
  do:
    - buff:
        on: this
        power: 1
        instead:
          - if:
              {
                history:
                  {
                    event: ub_activated,
                    who: self,
                    window: this_turn,
                    exclude: this_execution,
                    at_least: 2,
                  },
              }
            power: 2
```

### GAP-A-034

exclude_chosen:this_turn 降模式 domain 過濾；key 為 AbilityInstanceId＋ModeId＋TurnId，適正 play 才提交選用紀錄。

真實卡例：**CP04-SL19 ランファ**；來源 `CP04-SL19#0/section:0/0`，句型 `T5d0334eba7`。

> 自分の場の他のフォロワーの{UB}能力が発動したとき、下記から1つチョイスする。このターン、この能力でチョイスした選択肢はチョイスできない。【1】相手のリーダーすべてに2ダメージ。【2】自分のリーダーは{体力}+2する。

```yaml
auto:
  on:
    {
      ub_activated:
        {
          who: self,
          holder: { who: self, zone: field, kind: follower, other: true },
        },
    }
  do:
    - choice:
        n: 1
        exclude_chosen: this_turn
        options:
          - [{ damage: { to: { leaders: opponents }, n: 2 } }]
          - [{ life: { who: self, delta: 2 } }]
```

### GAP-A-035

count_of.where 在成功公開材料快照過濾原費用；不使用後來的手牌集合。

真實卡例：**BP08-SL14 真紅のローズクイーン**；來源 `BP08-SL14#0/text/1`，句型 `Tacf76f3452`。

> 【進化時】自分の手札を公開する。自分のPPを「これによって公開した元のコスト2のカードの枚数」と同じだけ回復する。

```yaml
evolve_time:
  do:
    - reveal: { ref: { who: self, zone: hand }, bind: shown, result: revealed }
    - pp:
        {
          who: self,
          delta:
            {
              count_of:
                { receipt: revealed, where: { orig_cost_eq: 2 }, view: before },
            },
        }
```

### GAP-A-036

kind 增 advance_follower；boxed/racing 是 V.state 具名屬性。racing 表當前出走狀態，不是回合內歷史次數。

真實卡例：**CSD01-010 見習い魔女と長い夜**；來源 `CSD01-010#0/text/1`，句型 `T985b42c820`。

> 相手のフォロワー1体を選ぶ。それに2ダメージ。自分の場に出走したフォロワーがいるなら、代わりに3ダメージ。

```yaml
spell:
  targets: [{ bind: t, from: { who: opp, zone: field, kind: follower }, n: 1 }]
  do:
    - damage:
        to: t
        n: 2
        instead:
          [
            {
              if:
                {
                  exists:
                    { who: self, zone: field, kind: follower, racing: true },
                },
              n: 3,
            },
          ]
```

### GAP-A-037

進化頭 into 名稱清單／Sel 編為 D.play-param Face；效果進化的規則選面用 D.rule-choice EvolutionCard，兩者不同。

真實卡例：**BP09-004 愛の妖精・ポーラ**；來源 `BP09-004#0/text/0`，句型 `T3e96f90125`。

> {進化}{コスト1}：これは『深緑の純心・ポーラ』か『真紅の絆・ポーラ』に進化する。

```yaml
evolve:
  cost: { pp: 1 }
  into: [深緑の純心・ポーラ, 真紅の絆・ポーラ]
```

### GAP-A-038

dice_rolled 產生 E.dice；event.value 為該次最終接受骰值，條件在本文（R-0009），不把點數 6 篩掉誘發。

真實卡例：**BP21-SL19 エンペラーフィスト・ガロム**；來源 `BP21-SL19#0/text/1`，句型 `Ta33b0c016f`。

> 自分がサイコロをふったとき、出た目が6なら、相手のリーダーすべてに4ダメージ。

```yaml
auto:
  on: { dice_rolled: { who: self } }
  do:
    - if:
        cond: { cmp: { left: { event: value }, op: eq, right: 6 } }
        then: [{ damage: { to: { leaders: opponents }, n: 4 } }]
```

### GAP-A-039

create 數量可為 upto 或 any；在建立前 ResolutionChoice 選數，不讓容量吞掉原選擇。也可 declare→bound，但只有明示數字宣告才用 I.declare。

真實卡例：**BP07-P20 ワイルド・マナ**；來源 `BP07-P20#0/text/0`，句型 `T5c85164636`。

> 『ナテラの大樹』1つを出す。【覚醒】状態なら、代わりに3つまで。

```yaml
spell:
  do:
    - create:
        token: ナテラの大樹
        n: 1
        to: field
        instead: [{ if: { awakened: true }, upto: 3 }]
```

### GAP-A-040

optional_cost_set 是付款後的固定費用修改（先 set 後 delta）；alt_cost 是 CR 10.6.2.5.1.2 整份原費用替代，兩欄不能互換。CP03-083 另見 §14。

真實卡例：**EBD02-007 次元の超越**；來源 `EBD02-007#0/text/0`，句型 `T556fd02bdc`。

> これをプレイする際、墓場のスペル10枚を消滅：コストを7にする。

```yaml
play:
  optional_cost:
    { banish: { sel: { who: self, zone: cemetery, kind: spell }, n: 10 } }
  optional_cost_set: 7
```

### GAP-A-041

變動成本量來自實付 receipt；此卡無須問一個額外 X。若卡文明訂 X=實付量，可 x.from_payment，不允許未付款前讀實付值。

真實卡例：**CP01-003 スマートファルコン**；來源 `CP01-003#0/text/1`，句型 `T9e3e0112f3`。

> {ファンファーレ}場の他のウマ娘・カードを好きな枚数手札に戻す：相手のフォロワーすべてに「戻した枚数」の2倍のダメージ。

```yaml
fanfare:
  cost:
    return_to_hand:
      {
        sel: { who: self, zone: field, trait: ウマ娘, other: true },
        any: true,
        result: returned,
      }
  do:
    - damage:
        {
          to: { who: opp, zone: field, kind: follower },
          n: { times: [{ count_of: returned }, 2] },
        }
```

### GAP-A-042

新增 skip_turn: {who}、pp_max: N（減少 N）、discard 的 ref/include_self/random；下文列所有子項。skip_turn 付款建立排程承諾，不等待下一回合才算付完。

真實卡例：**BP20-R13 安息の絶傑・マーウィン**；來源 `BP20-R13#0/text/1`，句型 `T3658b61e88`。

> {進化}次の自分のターンをスキップする：これは進化する。

```yaml
evolve:
  cost: { skip_turn: { who: self } }
```

### GAP-A-043

fused.material 過濾本次素材快照；counter_reached:{ref,name,value} 是變更後值而非持續輪詢。舊 Stack 提醒依 R-0008 不另造 destroy 誘發。

真實卡例：**BP20-SL05 空絶の顕現・オクトリス**；來源 `BP20-SL05#0/text/0`，句型 `Ta395908a27`。

> 自分の財宝・カードを融合したときか自分が財宝・カードをプレイしたとき、これは【疾走】を持つ。

```yaml
auto:
  on:
    any_of:
      - fused: { who: self, material: { trait: 財宝 } }
      - played: { who: self, trait: 財宝 }
  do: [{ grant: { on: this, keywords: [storm] } }]
```

### GAP-A-044

move.entry_effects 在該移動後物件建立時、判斷入場能力之前安裝；no_trigger 是抑制誘發，本文「使えない」此例用 cannot PlayAbility(Fanfare)，不任意變為不誘發。

真實卡例：**BP04-SL08 言霊遣い・ジンジャー**；來源 `BP04-SL08#0/text/0`，句型 `T10d53fbd99`。

> 【進化時】自分の手札のフォロワーを好きな枚数場に出してよい。それの{ファンファーレ}能力を使えない。このターン、それは相手を攻撃できない。

```yaml
evolve_time:
  do:
    - choose:
        {
          bind: picked,
          from: { who: self, zone: hand, kind: follower },
          any: true,
          may: true,
        }
    - move:
        ref: picked
        to: field
        result: entered
        entry_effects:
          - { cannot: { what: play_ability, ability: { keyword: fanfare } } }
    - apply:
        {
          on: { follow: entered },
          effect:
            {
              cannot:
                {
                  what: attack,
                  target: { who: opp, kind: follower, leader_or: true },
                },
            },
          until: turn,
        }
```

### GAP-A-045

`ability.kind: activated` 依 CR 12.2.1 包含進化能力；只有卡文明寫「進化以外」時才填 `except: [evolution]`。

ability:{kind:activated,except:evolution} 與 keyword:fanfare 是 AbilityPredicate；本例為所有 activated，不能擴成禁止全部能力。

真實卡例：**CP02-005 喜多見柚**；來源 `CP02-005#0/text/2`，句型 `T7887b26969`。

> {起動}【レッスン\_1】：元のコスト1以下の自分の場の他のカード1枚を選ぶ。それを手札に戻す。このターン、これの{起動}能力を使えない。

```yaml
activate:
  cost: { lesson: 1 }
  targets:
    [
      {
        bind: t,
        from: { who: self, zone: field, other: true, orig_cost_le: 1 },
        n: 1,
      },
    ]
  do:
    - move: { ref: t, to: hand }
    - apply:
        {
          on: this,
          effect:
            { cannot: { what: play_ability, ability: { kind: activated } } },
          until: turn,
        }
```

### GAP-A-046

by.ability_of 只判造成捨棄的直接能力 holder 與事件快照；一般捨棄的有效區視角依 CR 10.7.4。

真實卡例：**BP21-043 マナリアパーティー**；來源 `BP21-043#0/text/0`，句型 `Tf7295e0d27`。

> 自分の学院・カードの能力によってこれを自分の手札から捨てたとき、1枚引く。

```yaml
auto:
  on:
    {
      discarded:
        {
          ref: this,
          from: hand,
          who: self,
          by: { ability_of: { who: self, trait: 学院 } },
        },
    }
  do: [{ draw: { n: 1 } }]
```

### GAP-A-047

drive_checked 於一次檢查程序完成產生，和接受 trigger 不同，it 指發起檢查的從者。

真實卡例：**CP03-P27 スターライト・ユニコーン**；來源 `CP03-P27#0/text/0`，句型 `T125ed14f0d`。

> 自分の場のフォロワーがドライブチェックしたとき、それは{攻撃力}+1する。

```yaml
auto:
  on: { drive_checked: { holder: { who: self, zone: field, kind: follower } } }
  do: [{ buff: { on: it, power: 1 } }]
```

### GAP-A-048

fusion.sel 限合法素材區手牌／EX，加入種族與原費用過濾；不可把 n=3 硬寫成任意材料。

真實卡例：**BP20-030 簒奪のアジト**；來源 `BP20-030#0/text/1`，句型 `Tcd83ca58bc`。

> {起動}【融合】元のコスト1以上の財宝・カード3枚：これに融合カウンター1個を置く。

```yaml
activate:
  zone: hand
  fusion:
    {
      sel: { who: self, trait: 財宝, orig_cost_ge: 1 },
      n: 3,
      result: fusion_paid,
    }
  do: [{ counter: { on: this, name: 融合, delta: 1 } }]
```

### GAP-A-049

must_target:{on,by} → ChooseTarget Require；只在整體合法選擇可包含 this 時強制納入，保留其他條件與容量；不是讓不可選物件變合法。

真實卡例：**BP03-SL18 ダイヤモンドマスター**；來源 `BP03-SL18#0/text/1`，句型 `Td5914ae7be`。

> 相手プレイヤーは能力でこれを選べるとき、これを選ぶ。

```yaml
static:
  effects: [{ must_target: { on: this, by: opp } }]
```

### GAP-A-050

fusion.result 保存素材；fused_as_material 是 E.fused 的材料角色投影，來源為融合能力持有者，不是這張素材。

真實卡例：**BP19-048 没頭の実験体**；來源 `BP19-048#0/text/0`，句型 `Teb6968e3cc`。

> 自分の八獄・フォロワーの能力によってこれを融合したとき、1枚引く。自分の手札1枚を捨てる。

```yaml
auto:
  on:
    {
      fused_as_material:
        {
          ref: this,
          by: { ability_of: { who: self, kind: follower, trait: 八獄 } },
        },
    }
  do: [{ draw: { n: 1 } }, { discard: { who: self, n: 1 } }]
```

### GAP-A-051

新增 A.by-kind → K.match RecipientKind；能力的一個目標可為領袖或從者，各分支窄化，不能把 follower hp 當 player life。

真實卡例：**BP21-P54 浄火の令嬢**；來源 `BP21-P54#0/text/1`，句型 `Td992c08d2f`。

> 【進化時】自分のリーダーか自分の場のフォロワー1体を選ぶ。それは{体力}+2する。

```yaml
evolve_time:
  targets:
    [
      {
        bind: t,
        from: { who: self, zone: field, kind: follower, leader_or: true },
        n: 1,
      },
    ]
  do:
    - by_kind:
        ref: t
        leader: [{ life: { who: item, delta: 2 } }]
        follower: [{ buff: { on: item, hp: 2 } }]
```

### GAP-A-052

moved:{from,to,sel,during?} → E.move；drew:{who,not_phase} → E.draw，phase 過濾發生時刻，不是結算時 phase。

真實卡例：**BP05-029 簒奪の従者**；來源 `BP05-029#0/text/0`，句型 `T0d8f4d5397`。

> 自分のターン中、相手のデッキ1枚が墓場に置かれたとき、これは{攻撃力}+1する。

```yaml
auto:
  on:
    {
      moved:
        {
          from: { who: opp, zone: deck },
          to: cemetery,
          during: { turn: self },
        },
    }
  do: [{ buff: { on: this, power: 1 } }]
```

### GAP-A-053

distinct_by 接已登錄屬性 orig_cost/class/name；成本無資訊者依 CR 1.3.6，不把空值當一個費用種類。

真實卡例：**BP10-003 遺物の番人・ルチル**；來源 `BP10-003#0/text/1`，句型 `T57776b5e98`。

> {起動}これを{アクト}EXエリアのカードを元のコストがすべて異なるように5枚消滅：自分のエボルヴデッキの『神秘の遺物・スピネ＆ルチル』1枚を場に出してよい。

```yaml
activate:
  cost:
    steps:
      - act_self: true
      - banish: { sel: { who: self, zone: ex }, n: 5, distinct_by: orig_cost }
  do:
    - choose:
        {
          bind: form,
          from:
            { who: self, zone: evolve_deck, name: 神秘の遺物・スピネ＆ルチル },
          n: 1,
          may: true,
        }
    - move: { ref: form, to: field }
```

### GAP-A-054

additional_cost.result 的作用域是該次卡片 play；後續 fanfare 透過 entry.play_receipt 取得，不能直接借另一個 execution 的 local binding。本例為本文條件片段。

真實卡例：**CP03-SL16 ファントム・ブラスター・ドラゴン**；來源 `CP03-SL16#0/text/3`，句型 `T3b6f77cb64`。

> {ファンファーレ}これをプレイする際の追加コストとして自分の『ブラスター・ダーク』を場から墓場に置いていたなら、これは進化する。

```yaml
fanfare:
  do:
    - if:
        cond:
          {
            did:
              {
                result: { entry_payment: additional },
                where: { name: ブラスター・ダーク, zone: field },
              },
          }
        then: [{ evolve: { ref: this } }]
```

### GAP-A-055

search.to 可為分配表或自我取代：先找到才問替代目的地，may 拒絕時仍去原手牌，最後只洗一次。found 為 search 的內部唯讀 binding。

真實卡例：**BP03-002 コスモスファング**；來源 `BP03-002#0/text/1`，句型 `Te2eddc98cf`。

> {ファンファーレ}自分のデッキから獣・フォロワー1枚を探し、手札に加える。それがコスト2以下なら、代わりに場に出してよい。

```yaml
fanfare:
  do:
    - search:
        sel: { who: self, zone: deck, kind: follower, trait: 獣 }
        n: 1
        to: hand
        instead:
          - if: { cmp: { left: { cost_of: found }, op: le, right: 2 } }
            to: field
            may: true
```

### GAP-A-056

自身實際回合數依 CR 3.3.2–3.3.2.2：跳過不計、額外回合計入。

turn_number:{who} → V.turn.PlayerTurnOrdinal；只數實際開始的該玩家回合，跳過的不增加，額外回合增加。

真實卡例：**CP04-013 リマ**；來源 `CP04-013#0/text/0`，句型 `Td4234bcd75`。

> これは自分のターンが5ターン目かそれ以降でないなら、プレイできない。

```yaml
play:
  cannot_if: { cmp: { left: { turn_number: { who: self } }, op: lt, right: 5 } }
```

### GAP-A-057

kind/name 的 same_as 接型別化引用；名稱比較用凍結材料 CardNames 集合，精確比對按 CR 2.1，不能改查墓地新物件。

真實卡例：**BP10-112 星灯りの女神**；來源 `BP10-112#0/text/1`，句型 `Tb0ecfd3840`。

> 【進化時】墓場のフォロワー1枚を消滅：自分のデッキから「消滅させたフォロワーと同名のフォロワー」2枚まで探し、EXエリアに置く。

```yaml
evolve_time:
  cost:
    {
      banish:
        {
          sel: { who: self, zone: cemetery, kind: follower },
          n: 1,
          result: material,
        },
    }
  do:
    - search:
        sel:
          {
            who: self,
            zone: deck,
            kind: follower,
            name: { same_as: { objects_of: material, view: before } },
          }
        upto: 2
        to: ex
```

### GAP-A-058

equipped → E.equip{holder,equipment}；event.holder 是此次 host，和目前 host 查詢不同。保留裝備自身作來源。

真實卡例：**CP04-PR22 氷竜剣**；來源 `CP04-PR22#0/text/0`，句型 `T06c77a2077`。

> これを装備したとき、相手の場のフォロワー1体を選ぶ。それをアクトする。これを装備したフォロワーは{攻撃力}+2/{体力}+2する。

```yaml
auto:
  on: { equipped: { equipment: this } }
  targets: [{ bind: t, from: { who: opp, zone: field, kind: follower }, n: 1 }]
  do:
    - act: t
    - buff: { on: { event: holder }, power: 2, hp: 2 }
```

### GAP-A-059

zone 單值正規化為集合，static 依來源在任一合法區有效；不複製成兩份效果。

真實卡例：**EBD01-005 瘴気の妖精姫・アリア**；來源 `EBD01-005#0/text/2`，句型 `Tc84e6a03de`。

> これが場かEXエリアにある限り、自分の場の妖精・トークン・フォロワーすべては【突進】を持つ。

```yaml
static:
  zone: [field, ex]
  effects:
    - keywords:
        {
          on:
            {
              who: self,
              zone: field,
              trait: 妖精,
              kind: follower,
              token: true,
            },
          list: [rush],
        }
```

### GAP-A-060

did(evolve) 只讀該次 receipt.evolved；重新出場 this 需沿 move receipt 跟隨，不沿印刷卡號尋找。

真實卡例：**BP07-SL20 母なる君**；來源 `BP07-SL20#0/text/1`，句型 `T7ef0795334`。

> {ラストワード}場かEXエリアの『ナテラの大樹』1枚を消滅：これをアクト状態で場に出す。これは進化する。進化しなかったなら、これは消滅する。

```yaml
lastword:
  do:
    - pay:
        cost:
          {
            banish:
              {
                sel: { who: self, zone: [field, ex], name: ナテラの大樹 },
                n: 1,
              },
          }
        do:
          - move:
              {
                ref: { follow: { event: move_receipt } },
                to: field,
                acted: true,
                result: returned,
              }
          - evolve: { ref: { follow: returned }, result: evolved_now }
          - if:
              cond: { not: { did: evolved_now } }
              then: [{ banish: { ref: { follow: returned } } }]
```

### GAP-A-061

event.field 或 `event:<field>` 都須先由 on 型別推導欄位。固定傷害事件值在本文判斷；兩次 3、2 不合成 5（R-0009、Q940）。

真實卡例：**CP02-SL10 辻野あかり**；來源 `CP02-SL10#0/text/1`，句型 `T247492c8a1`。

> これがダメージを受けたとき、そのダメージが5以上なら、自分のPP最大値を+1する。

```yaml
auto:
  on: { damaged: { ref: this } }
  do:
    - if:
        cond: { cmp: { left: { event: amount }, op: ge, right: 5 } }
        then: [{ pp_max: { who: self, delta: 1 } }]
```

### GAP-A-062

cannot.deal_damage 降來源方向 Damage.Prevent；不生成 0 點 E.damage，不等於不能攻擊。

真實卡例：**BP01-024 精霊の呪い**；來源 `BP01-024#0/text/1`，句型 `Tf08d681221`。

> 相手のフォロワー1体を選ぶ。このターン、それはダメージを与えない。

```yaml
spell:
  targets: [{ bind: t, from: { who: opp, zone: field, kind: follower }, n: 1 }]
  do:
    [
      {
        apply:
          { on: t, effect: { cannot: { what: deal_damage } }, until: turn },
      },
    ]
```

### GAP-A-063

pp_max_increase 是獨立許可，duration 必須覆盖下次階段全程。mode 的 who=opp 為打出程序交接；效果接受者也是該玩家。

真實卡例：**BP05-006 マインドルーラー・モートン**；來源 `BP05-006#0/text/0`，句型 `T1aa285dfc8`。

> 自分のエンドフェイズが来たとき、相手プレイヤー1人は下記から1つチョイスする。【1】次のスタートフェイズに1枚引けない。【2】次のスタートフェイズにPP最大値を+1できない。【3】次のメインフェイズにフォロワーをプレイできない。

```yaml
auto:
  on: { phase: { which: end, whose: self } }
  do:
    - choice:
        who: opp
        n: 1
        options:
          - [
              {
                player_effect:
                  {
                    who: opp,
                    effect:
                      {
                        cannot:
                          { what: draw, during: { phase: start, cause: rule } },
                      },
                    until: { during_next_phase: { who: opp, phase: start } },
                  },
              },
            ]
          - [
              {
                player_effect:
                  {
                    who: opp,
                    effect: { cannot: { what: pp_max_increase } },
                    until: { during_next_phase: { who: opp, phase: start } },
                  },
              },
            ]
          - [
              {
                player_effect:
                  {
                    who: opp,
                    effect:
                      { cannot: { what: play_card, card: { kind: follower } } },
                    until: { during_next_phase: { who: opp, phase: main } },
                  },
              },
            ]
```

## 14. 複合缺口拆項與補充完整例

GAP-A-016 的 11 個子項逐項列出；GAP-A-042 的 skip_turn 已見 §13，其餘三項亦列出。小節不另外增加 gap-catalog 家族數。

### A-016a 隨機公開手牌

**BP11-112 ジャイアントマッチ**；`BP11-112#0/text/2`。reveal.random 是 I.random 的均勻不放回抽樣，無玩家選牌輸入。

> {起動}これを{アクト}墓場に置く：自分の手札をランダムに2枚公開する。その中から、フォロワーすべてを場に出す。

```yaml
activate:
  cost: { steps: [{ act_self: true }, { self_to_cemetery: true }] }
  do:
    - reveal: { ref: { who: self, zone: hand }, random: { n: 2 }, bind: shown }
    - move: { ref: { in: shown, kind: follower }, to: field }
```

### A-016b 從頂第二張或底

**BP10-062 エターナルホエール**；`BP10-062#0/text/2`。position.one_of 在結算時選位置；nth 從 1 起算，牌庫不足的邊界尚無直接 Q&A；不宣告「夾到合法位置」是既定規則，需另登錄判讀與局面。

> {ラストワード}相手のリーダーすべてに2ダメージ。これをデッキの上から2番目かデッキの下に置く。

```yaml
lastword:
  do:
    - damage: { to: { leaders: opponents }, n: 2 }
    - move:
        {
          ref: { follow: { event: move_receipt } },
          to: deck,
          position: { one_of: [{ nth: 2 }, bottom], by: self },
        }
```

### A-016c repeat 累積結果

**BP14-046 願望の実現**；`BP14-046#0/text/0`。collect.receipts 回保序 ReceiptList；follow 接列表取各輪成功 after 的聯集。必須在離開迴圈後使用；循環無進展不假裝完成。

> 自分のデッキの上1枚をEXエリアに置く。これをEXエリアが上限になるまでくり返す。このターン、次に自分が「これによってEXエリアに置いたカード」をプレイする際、コストを0にする。

```yaml
spell:
  do:
    - repeat:
        until:
          {
            cmp:
              {
                left: { zone_free: { who: self, zone: ex } },
                op: eq,
                right: 0,
              },
          }
        check: after
        collect: { receipts: moved_all }
        do: [{ move: { top: { who: self, n: 1 }, to: ex } }]
    - player_effect:
        who: self
        effect:
          {
            cost_mod:
              {
                when_playing: { in: { follow: moved_all } },
                set: 0,
                next_only: true,
              },
          }
        until: turn
```

### A-016d 兩種付款擇一

**BP01-SL15 骸の王**；`BP01-SL15#0/text/0`。one_of 在 optional_cost 所屬 PlayParameter 選，材料與是否付款合併；不是本文 either。

> これをプレイする際、場のスタンド状態の{ナイトメア}カード4枚を墓場に置く、または、EXエリアの{ナイトメア}カード4枚を消滅：コストを-9する。

```yaml
play:
  optional_cost:
    one_of:
      - to_cemetery:
          {
            sel: { who: self, zone: field, class: ナイトメア, acted: false },
            n: 4,
          }
      - banish: { sel: { who: self, zone: ex, class: ナイトメア }, n: 4 }
  optional_cost_delta: -9
```

### A-016e 免原費用打出能力

**CP04-P86 アメス**；`CP04-P86#0/text/0`。只列此卡選項 2 的最小片段，t 為該模式打出時所選、已滿足卡文條件的合法目標；完整頭與歷史條件沿 A-033／034。Normal 與 waive_original 對應 I.play-ability 的封閉 CostPolicy，UB 的任意 X 依 CR 14.5.1.4.2 為 0。

> {UB}【進化時】下記から1つチョイスする。【1】自分の場の他のフォロワー1体を選ぶ。それは【守護】を持つ。【2】自分の場の「これと同名を除くプリコネ・フォロワー」1体を選ぶ。これを含めず、このターン中に自分の{UB}能力が2回以上発動していたなら、それの{UB}能力1つを元のコストを支払わずに発動する。

```yaml
play_ability:
  of: t
  ability: { ub: true, n: 1 }
  cost_policy: waive_original
```

### A-016f create 綁定

**BP20-SL15 絶尽の顕現・ライオ**；`BP20-SL15#0/text/0`。create.bind 保存實際新物件，不是輸入模板；後文 cost_mod 捕捉這次建立物件。

> 【進化時】相手の場のフォロワーすべてに9ダメージ。『絶尽の偽証』2枚をEXエリアに置く。このターン、それをプレイする際、コストを-1する。

```yaml
evolve_time:
  do:
    - damage: { to: { who: opp, zone: field, kind: follower }, n: 9 }
    - create: { token: 絶尽の偽証, n: 2, to: ex, bind: created }
    - apply:
        {
          on: created,
          effect: { cost_mod: { when_playing: { in: affected }, delta: -1 } },
          until: turn,
        }
```

### A-016g 分配 token 張數

**BP19-052 景観の魔導師**；`BP19-052#0/text/1`。token.allocate.total=3，對兩模板分配非負整數總和；與 [A,B] 各一張不同。

> {起動}{コスト5}【土の秘術】：『防御型ゴーレム』や『攻撃型ゴーレム』合わせて3体を場に出す。

```yaml
activate:
  cost: { steps: [{ pp: 5 }, { earth_rite: 1 }] }
  do:
    - create:
        token:
          {
            allocate:
              { among: [防御型ゴーレム, 攻撃型ゴーレム], total: 3, by: self },
          }
        to: field
```

### A-016h first/rest 目標角色

**BP21-PR13 楽園の終焉・イツルギ**；`BP21-PR13#0/text/1`。所有目標與主傷害角色在同一打出參數中指定，選零張時 primary 也零；不任選集合首元素。roles primary 為 if_nonempty:1、rest 為餘者。

> {ファンファーレ}相手の場のフォロワー3体まで選ぶ。その中の1体に5ダメージ。残りの2体に2ダメージ。

```yaml
fanfare:
  targets:
    - bind: t
      from: { who: opp, zone: field, kind: follower }
      upto: 3
      roles: { primary: { if_nonempty: 1 }, rest: remaining }
  do:
    - damage: { to: { role: { of: t, name: primary } }, n: 5 }
    - damage: { to: { role: { of: t, name: rest } }, n: 2 }
```

### A-016i 同時放置現有物件與 token

**BP13-SL16 永劫の吸血鬼・アルザード**；`BP13-SL16#0/text/0`。simultaneous 共享容量程序及批次，不能先填滿再丟掉另一項。內部子 receipt 在整批完成後可讀。Q1776：EX 已有 4 張時可選放アルザード或紅の牙；若選後者，前者留墓地，self_moved 無成功成員，後句 counter 不做。

> {ラストワード}これと『紅の牙』1枚をEXエリアに置く。EXエリアのこれに休眠カウンター2個を置く。

```yaml
lastword:
  do:
    - simultaneous:
        do:
          - move:
              {
                ref: { follow: { event: move_receipt } },
                to: ex,
                result: self_moved,
              }
          - create: { token: 紅の牙, n: 1, to: ex }
    - counter: { on: { follow: self_moved }, name: 休眠, delta: 2 }
```

### A-016j 變身後分配 token 張數

**BP08-011 心無き決闘**；`BP08-011#0/text/0`。Q1060 要求打出時必選 2 張；Q1061 要求完整成功時各一種。依 CR 5.17.1 先消滅、再依實際消滅數新建；在後者以 ResolutionChoice 分配模板張數，不對原卡配 token。若只消滅成功 1 張，在兩模板中選 1 種、每模板最多 1 張（`strength: inferred`）；0 張則不建立也不問分配。容量限制另在建立時處理，不回寫消滅數。

> 自分のEXエリアの『操り人形』2枚を選ぶ。それはそれぞれ『ロイド』と『ヴィクトリア』に変身する。

```yaml
spell:
  targets: [{ bind: t, from: { who: self, zone: ex, name: 操り人形 }, n: 2 }]
  do:
    - transform:
        ref: t
        into:
          {
            allocate:
              { among: [ロイド, ヴィクトリア], each_exactly_once: true },
          }
```

### A-016k 抽牌階段篩選

**BP10-SL20 《節制》・ルーゼン**；`BP10-SL20#0/text/1`。during.not_phase 看 Draw 的真實發生階段；不是所有把卡放到手牌。

> これが場にいる限り、相手プレイヤーすべては、スタートフェイズ以外でカードを引けない。

```yaml
static:
  effects:
    [{ cannot: { who: opponents, what: draw, during: { not_phase: start } } }]
```

### A-042a PP 上限費用

**BP06-P16 不死鳥の女帝**；`BP06-P16#0/text/1`。pp_max:1 在成本表示減最大值 1，降低 I.resource ReduceMaximum；不能付不足。

> {ラストワード}PP最大値を-1：これをアクト状態で場に出す。

```yaml
lastword:
  do:
    - pay:
        cost: { pp_max: 1 }
        do:
          [
            {
              move:
                {
                  ref: { follow: { event: move_receipt } },
                  to: field,
                  acted: true,
                },
            },
          ]
```

### A-042b 捨手牌自身

**BP20-SL14 絶尽の顕現・ライオ**；`BP20-SL14#0/text/2`。兩個 group 跨組不重用；this 真正在手牌才可付，與手牌外文字來源不同。

> {起動}手札のこれと絶傑・魔法使い・スペル1枚を捨てる：『絶尽の偽証』1枚をEXエリアに置く。

```yaml
activate:
  zone: hand
  cost:
    discard:
      groups:
        - { sel: { in: this }, n: 1 }
        - {
            sel:
              {
                who: self,
                zone: hand,
                kind: spell,
                traits_all: [絶傑, 魔法使い],
              },
            n: 1,
          }
  do: [{ create: { token: 絶尽の偽証, n: 1, to: ex } }]
```

### A-042c 隨機捨棄成本

**BP03-088 悪魔の笛吹き**；`BP03-088#0/text/1`。費用預演不能先向玩家洩漏隨機結果再讓其反悔；付款 transaction 同步記 RNG 與 receipt。

> {ファンファーレ}手札をランダムに1枚捨てる：これは{攻撃力}+1して、【ドレイン】を持つ。

```yaml
fanfare:
  cost: { discard: { sel: { who: self, zone: hand }, n: 1, random: true } }
  do:
    [
      { buff: { on: this, power: 1 } },
      { grant: { on: this, keywords: [drain] } },
    ]
```

### GAP-A-006：移動後 set 0 與 delta -2

BP07-071 的第一行完整 YAML（Q&A／原卡文見凍結快照）；R.target 選到的舊世代不能直接當 EX 新世代。

```yaml
fanfare:
  targets:
    - bind: t
      from: { who: self, zone: cemetery, trait: 吸血鬼 }
      upto: 2
      cost_sum_le: 6
  do:
    - move: { ref: t, to: ex, result: moved }
    - apply:
        {
          on: { follow: moved },
          effect: { cost_mod: { when_playing: { in: affected }, set: 0 } },
          until: turn,
        }
```

CP04-060 的 UB 進化時第一行完整 YAML；search 的 follow 只取實際 moved 成員，不取 found 但未移動者。

```yaml
evolve_time:
  ub: true
  do:
    - search:
        sel:
          {
            who: self,
            zone: deck,
            trait: ドラゴンズネスト,
            not_same_name_as_this: true,
          }
        n: 1
        to: ex
        result: searched
    - apply:
        {
          on: { follow: searched },
          effect: { cost_mod: { when_playing: { in: affected }, delta: -2 } },
          until: turn,
        }
```

### GAP-A-040：替代費用不是 set 0

CP03-083 的第一行：alt_cost:{if,cost,may} 在打出時決定；選替代才不付原 9PP，其他追加與修正仍按 CR 10.6.2.5 管線。

```yaml
play:
  alt_cost:
    if:
      {
        cmp:
          {
            left:
              {
                count: { who: self, zone: cemetery, trait: シャドウパラディン },
              },
            op: ge,
            right: 15,
          },
      }
    may: true
    cost:
      {
        to_cemetery:
          {
            sel:
              {
                who: self,
                zone: field,
                name: ファントム・ブラスター・ドラゴン,
              },
            n: 1,
          },
      }
```

### GAP-A-041：X 取實付 counter

BP03-001 的起動行（Q443）：選取要取的童話 counter 數於打出參數；付款成功後 X 為 receipt 實際取走量。不是多問一個自由 X。

```yaml
activate:
  x: { from_payment: removed }
  cost:
    steps:
      - pp: 1
      - counter: { on: this, name: 童話, any: true, result: removed }
  do: [{ buff: { on: this, power: X, hp: X } }]
```

### GAP-A-055：分配到手牌與墓地

CP03-SL07 的找牌片段：完整句型見索引，to.partition 各份額 1，依文字先手牌再墓地處理；只找到 1 張時必須先加手牌，不能任選放墓地。每段所要求的選擇保留各自指示／輸入點，不因內部同屬 search 而合成錯誤決定；末尾只洗牌一次。

```yaml
search:
  sel: { who: self, zone: deck, kind: follower, name_contains: ブラスター }
  upto: 2
  to:
    order: printed
    partition:
      - { zone: hand, n: 1 }
      - { zone: cemetery, n: 1 }
```

### 影響集合的捕捉不是能力授予

BP12-109 第一行依 Q1708：集合是打出該能力時在對手場上的從者，新增 apply.capture:play 明示該快照；不是對對手玩家安裝未來所有從者的規則。Q1709 另要求離場來源的 lastword 仍不能造成傷害，故 prevent 的來源匹配保留該受影響世代的 LKI，不能僅看目前場上物件。

```yaml
evolve_time:
  do:
    - apply:
        on: { who: opp, zone: field, kind: follower }
        capture: play
        effect: { cannot: { what: deal_damage } }
        until: turn
```

### 欄位正規化補充

`Sel.in` 接 ObjectRef 或物件 binding，單一 ref 正規化為 singleton；`traits_all` 是集合包含全部指定種族。`token:true` 是 TokenStatus 的別名；`zone` 可為集合，降低 S.combine Union。`zone_free:{who,zone}` 在 Expr 位置回 Nat，在 Cond 位置須有 ge/eq 比較。cost.counter.n/upto/any 均接受，holders 預設 this 時唯一、有 Sel 時要求明示數量；on:this 不再問 holder。`effect.on` 省略時使用 apply 的 affected；static 不可省略需明確範圍的 on。

`play.optional_cost.result` 與 `additional_cost.result` 可以在同 execution 用 V.receipt-field；入場衍生的 fanfare 則用 entry_payment:additional 明確跨 execution provenance。`history_sum` 允許 field=amount/delta/removed_stack，必須先按事件與 cost_kind 窄化。`damage` 的來源篩選與 recipient 的世代、LKI 壽命見 IR §13 S-023。

### GAP-A-044：本次入場不誘發 fanfare

CP04-P68 的 sections[0] fanfare 找牌行；search.entry_effects 轉交給其內部 I.move，no_trigger 僅篩選這次新建立世代的 Fanfare。分組都各 1、可找不到的例外仍由 search 規則處理，整體 search 只洗一次。

```yaml
fanfare:
  do:
    - search:
        groups:
          - {
              sel:
                {
                  who: self,
                  zone: deck,
                  kind: follower,
                  trait: ルーセント学院,
                  orig_cost_le: 3,
                },
              n: 1,
            }
          - {
              sel:
                {
                  who: self,
                  zone: deck,
                  kind: follower,
                  trait: ルーセント学院,
                  orig_cost_eq: 1,
                },
              n: 1,
            }
        to: field
        entry_effects: [{ no_trigger: { on: entering, kinds: [fanfare] } }]
```

`move`、`search`、`create`、`transform` 均接受 `entry_effects: List<EffectSpec>`；後兩者轉交 I.token，在成功新建世代上安裝。這是共用型別能力，不表示 BP08-011 自帶禁止 fanfare。

entry_effects 中 entering 為此次入場新世代、其能力來源尚未掃描的物件，不是可由撰寫者猜測的 ID。若卡文只禁止「這一次入場」的誘發，作用域為 EntryOccurrence；若禁止這些物件的能力打出，依 cannot.play_ability 的 duration 處理，不混為一種期限。

### 批次誘發與封閉變體補記

`auto.aggregation: batch` 對 `B.trigger.aggregation=PerBatch(ExistsMatching)`，只限原文明示「1體以上」等聚合。CP04-PR25 的授予能力片段（Q2549）：

```yaml
auto:
  on:
    {
      dealt_damage:
        {
          ref: this,
          kind: ability,
          recipient: { who: opp, zone: field, kind: follower },
        },
    }
  aggregation: batch
  do: [{ damage: { to: { leaders: opponents }, n: 3 } }]
```

此能力掛在 grant.abilities 的接受者（裝備者），不是闇斧ナハトファング或裝備提供者自身。`no_trigger.on` 接 Ref/Binding；`search.entry_effects` 與 move 相同，在入場掃描前作用。`extra_trigger:{phase,who,n}` 降 TriggerMultiplicity，N 是附加次數。`ignore_ward` 只略過守護目標限制。`choice_any` 是 ChooseMode 的數量自我取代：原本至少一個才適用；BP20-P65 替代後「好きな数」可為 0（依 CR 1.3.5／5.18.2.2 推論，無此卡直接 Q&A；`strength: inferred`），不套「Nつまで」的 1 下限。

`A.by-kind` 為 by_kind 的 canonical ID；`A.define`、`A.either` 是新增組合／宣告 ID，其他新原子（apply、sep）依 A.act.<鍵>。v0 的 A.declare、A.order、A.distribute、A.place-acted 分類 ID 保留；動作表的 order 使用 A.order，不另建立第二個 canonical A.act.order。

`event.move_receipt` 是離場／移動事件的 header.receipt 型別化投影。lastword 本文依卡文明示引用離場的「これ」時，以 `follow:{event:move_receipt}` 找到該次實際移動 after；這是 R.follow-move 的 TriggerExplicitReference 授權，並非 LKI 可執行。若該世代又移動，follow 失效，不搜尋同卡號。`this` 作能力 source 仍指原持有者，不因效果召回而偷偷改變傷害來源。

`by_kind` 的 leader 分支將 item 窄化為 PlayerRef，所以 life.who:item；follower 分支 item 是 ObjectRef。`entry_payment` 合法不存在時 did 為 false；拼錯 receipt 名或跨 scope 引用仍是載入錯誤。以上兩者不能混作缺值兜底。
