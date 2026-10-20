# 作者 1.0 構造與鍵清冊

本表整理[作者語法](author-syntax-1.0.md) §2–9、§13–14 與[承載契約](author-contract-1.0.md)的上下文。它是已定語法的導覽，**不是第二份機器語法，也不代表整棵作者樹已實作**；可用資源及入口見 [dsl/README](../../dsl/README.md)。固定物件只接受所屬變體的欄位。相同拼字出現在不同上下文，不表示型別或時點相同。

若清冊與作者語法／承載契約衝突，以後者為準；原文形狀有差異時列明出處，不以清冊增添限制。

以下 `?` 表選用、`|` 表互斥，`[]` 表列表元素；只有標明的 map 可用動態鍵。`Expr` 保留有號讀值，動作要求量不先轉 Nat；Nat 葉參數仍不能是負值。bool 不作 Int，數字字串不作整數。關鍵字、職業、種類及卡名的語法型別不等於登錄已存在，後者由來源／registry 驗證。

## 容器與共用葉

| 上下文 | 欄位／形狀 | 分開驗證的責任 |
| --- | --- | --- |
| A.file | format=1、kind=author_effect_set、scope、cards | cards 為原樣 CardNumber → A.card；逐卡 meta.dsl 不能另選 schema |
| A.card | meta、note?、abilities?、faces?、deck_rule? | faces 以穩定 FaceId 定位；不附另一份卡表 |
| A.ability | line、section?、sources、imports?、一個能力頭或 generated_by | inline 另必填 ports；generated_by 不覆寫 ports |
| sources[] | binding_id、occurrence、ability_scope | 恰一筆；occurrence 沿四層型別，segments 可多段 |
| ports | imports、exports | Code → {path,bind}；path 元素為字串鍵／Nat 索引；imports path 必空 |
| imports | Code → {binding_id,ability_scope,port} | 引用可見上游 export；不是葉參數 |
| generated_by | macro、version、params | macro={frame_id,ability_scope,applicability_scope}；version 為完整 body_version |
| effect_body_shard | format=1、kind、scope=shared、records | companion 不進逐卡索引 |
| records[]（companion） | frame_id、ability_scope、body_version、dsl、params、body、ports、written_by、registrations、note? | body 只有能力頭／payload，不含位置或 generated_by |
| registrations[] | id、domain、reviews | domain={regions}；body review 與 use review 分開 |
| effects_index | format=1、kind=effects_index、records | (card_id,scope) 唯一 |
| records[]（index） | card_id、scope、file、kind、record_key | file 原樣 POSIX 相對路徑；effect_set 只准 shared；候選閉包另驗 |
| A.param | {param:ParamName} | 只准承載 §3 白名單的葉位置 |
| ParamDecl | {type,source:{slot,component}} | 八種 type；component=value/constant/negated_magnitude；來源角色由接線驗證 |
| ParamValue | {type,value} | Int／Nat／CardName／Concept／CardKind／Player／Zone／Phase 的封閉和型別 |
| meta | dsl、source、written_by、reviewed_by、status、verified_by_exam、rulings | 宣告不授予資格；不以空 do 代替來源判定 |
| written_by[]／Actor | {actor:{who,model},date} | model 可 null；真實身分由作業填入 |
| Review | kind=body/use、reviewer、date、verdict、reason、dependencies、rekey? | disputed 的 reason 非空；依賴型別按 kind 分開 |
| macro_mechanical | kind、body、use_dependencies | 沒有虛構 reviewer |

CardNumber 沿現有身分登錄的非空字串，保留小寫與 `Ⓢ` 等原字；存在性及卡號／永久 ID 對應另查登錄，不靠新正規式推算。ParamName 為 `[A-Za-z][A-Za-z0-9_]*`，來源 Code 為 `[a-z][a-z0-9_.-]*`，兩者不可合併。

## 能力、費用與控制流程

| 上下文／構造 | 鍵與變體 | 必須區分 |
| --- | --- | --- |
| keywords／quick | Keyword 列表／true | 不是動作授予；參數型 Keyword 須按登錄形狀 |
| fanfare、lastword、evolve_time、super_evolve_time、attack_time、race_time、drive_time | 適用的 targets、cost、optional_cost、x、define、do、once_per_turn、ub、zone | 頭的 cost 只按 §3 允許處出現；lastword／auto 冒號用本文 pay |
| auto | on、do 及適用共通欄；aggregation?=batch | state 誘發與事件誘發；不用 when |
| activate | cost、do、usable_if?、quick?、advance?、fusion? 及適用共通欄 | fusion={sel,n,result?}；hand 不改 cost 的時點 |
| evolve／ride／feed（能力頭） | {cost,into?,usable_if?}／{cost}／{times?,cost} | 與 A.act.evolve／feed 分開 |
| spell | targets?、optional_cost?、x?、define?、do | 空 do 合法；仍須打出程序 |
| static | while?、zone?、effects | 動態 selector、來源有效區；需範圍的 on 不省略 |
| play | cost_delta?、cost_set?、additional_cost?、optional_cost?、optional_cost_delta?、optional_cost_set?、alt_cost?、cannot_if?、playable_from?、define? | set／delta／替代費用不等價；alt_cost={if,may?,cost} |
| A.cost 短形 | pp、ep、act_self、self_to_cemetery、banish_self、discard、banish、to_cemetery、return_to_hand、reveal、counter、flip_down、flip_up、leader_life、lesson、earth_rite、stack_counter_from、act、to_ex、to_deck_bottom、to_deck、pp_max、skip_turn、one_of | §3 要求保留付款鍵序；與承載 §4.1／§6 的排序及鍵序不改版存在差異，本表不裁決；to_deck 可含 shuffle:true（§13 GAP-A-012） |
| A.cost 長形 | steps:[{單一費用鍵:payload}…] | 同鍵重複用 steps；§3 未明定長短形混填規則 |
| 費用材料 | sel、n\|upto\|any、groups?、distinct_by?、result?；banish 另 top:{who,n}（§13 GAP-A-010） | groups 共用 disjoint／整體限制；discard 的 ref/include_self/random 見 §7、§13 GAP-A-042 |
| 費用 counter | on、name、n\|upto\|any、holders?、result? | on:this 固定唯一 holder；selector 須明示 holders |
| optional_cost／additional_cost | 費用＋result? | result 是付款 receipt；付款前不可讀 paid |
| x | {min?,max?} 或 {from_payment} | 自由宣告與付款量別名互斥 |
| A.target | bind、from、n\|upto\|any、groups?、cost_sum_le?、distinct_by?、distinct_names?、option?、roles? | 打出時選；沒有 targets[].if |
| A.act.choose | bind、from、n\|upto\|any、by?、groups?、cost_sum_le?、distinct_by?、reveal?、order?、position?、may? | 結算時選；may 與選零個分開 |
| A.optional／A.pay | who?、do、then?、result?；pay 另 cost | then 仍受拒絕／付款責任保護 |
| A.choice | n\|upto、options、instead?、who?、exclude_chosen? | 打出參數；upto 是 1..N |
| A.either | who?、options、bind? | 結算時恰選一項；不接受 choice 的數量欄 |
| A.declare | bind、kind=number/name、min?、max?、who? | §4 未逐 kind 明定 min/max 組合，不能據此新增拒絕規則 |
| A.if | cond、then、else? | 不讀事件前置 when |
| A.comb.repeat | n\|until\|while、check?、do、collect? | collect={receipts:name}；check=before/after |
| repeat_ability | {while} | 本文重複，非 play_ability |
| A.comb.foreach | in、as、do | 明示逐物件；不把全體改逐一 |
| A.comb.each_player | who?=each、stages | stage 為 Steps 或 {do,mode,evidence?}；mode=choice/nonchoice/explicit |
| A.comb.simultaneous | do、result? | 尚未完成的 receipt 不可互讀 |
| A.by-kind | ref、as?、leader、follower | leader 分支為 PlayerRef、follower 為 ObjectRef |
| roles／role | Map<RoleName,{if_nonempty:1}\|remaining>／{of,name} | first/rest 角色非無序集合的任取順序；形狀與數量位置見[選取矩陣](author-selection-shapes-1.0.md) |

## 表達式、條件、集合與引用

| 上下文 | 構造／鍵 | 限制 |
| --- | --- | --- |
| A.define | binding → Expr | 純 Live 別名；不取代步驟、不在能力入口凍結 |
| A.expr 算術 | plus、minus、times、half_up、per、min、max | 二元運算恰兩元素；min/max 為非空 Expr 陣列；per={of,unit}（§5、§13 GAP-A-028） |
| A.expr 集合／物件讀值 | count、count_of、objects_of、bound、power_of、hp_of、cost_of、hand、leader_life、counter、distinct_names、distinct_orig_costs、lki_of | count_of 短形是 actual；objects_of 是明示 view 的快照，與 follow 不同；已定片段見[讀值矩陣](author-expression-shapes-1.0.md#快照與收據讀值矩陣) |
| A.expr 程序／歷史 | dice、paid、x_paid、history_count、history_sum、resource、agg、event、turn_number | resource={who,kind}；agg={of,attr,fn}；history_sum 先窄化欄位 |
| A.expr.zone_free | {who,zone} | 回 Nat；初版容量讀值為 field/ex |
| A.cond 組合 | cmp:{left,op,right}、all、any、not | cmp.op=eq/ne/lt/le/gt/ge；左右同型 |
| A.cond 謂詞 | exists、on_field、in_zone、leader_life、hand、zone_free、my_turn、opp_turn、construction、standing、acted、evolved、chosen、did、entered_from、played_from、combo、spellchain、necro、awakened、crimson、entered_by、is、history | zone_free 在 Cond 須 ge/eq；is={ref,kind}；entered_by={ref,by} |
| Sel 玩家／區域／種類 | who、zone、kind、class、not_class、trait、traits_any、traits_all、not_trait | zone 單值或集合；traits_all 與 traits_any 不合併；kind 可 same_as（§13 GAP-A-057） |
| Sel 名稱 | name、names、name_contains、not_same_name_as_this | name 可 {same_as:{objects_of,view}}，不是任意字串替換 |
| Sel 數值 | cost_*、orig_cost_*、hp_le、power_max | 比較 suffix 與數量 Expr 分開 |
| Sel 狀態 | other、acted、evolved、keyword、has_trigger、has_evolve、not_token、token、face_up、advance_follower、boxed、racing、entered_this_turn | token:true 是 TokenStatus 別名；advance_follower 在 §5 列為欄位、§13 GAP-A-036 列為 kind 值，本表保留差異 |
| Sel 組合／視圖 | leader、leader_or、leader_of、in、rest、any_of、except、where、as；同型／同名引用、頂底切片 | in 接 ObjectRef／物件 binding，receipt 須先 objects_of；where 的 item 局部 |
| PlayerRef／PlayerSet | self、turn、non_turn、current、owner_of、master_of、opp／opponents、each、other_players | owner 與 controller 不互換；集合不因二席而降單人 |
| Ref／Recipient | this、it、binding、first:binding、second:binding、rest:binding、attack_target、host、cost1；leader、leaders、role、follow、event | first/second 須有順序；by_kind 的 item 按分支窄化 |
| ReceiptRef | binding；{event:move_receipt}；{entry_payment:additional} | 只有有移動授權的 receipt 可 follow；不存在的合法 entry_payment 不等於拼錯引用 |
| count_of 長形 | receipt、where、view=before/after | 三鍵必填；成功材料快照篩選，不查現在卡片；receipt 材料型別另驗 |
| did 長形 | result、where | 兩鍵必填；預設成功判準且存在匹配的成功材料；已登錄 before 視角限付款／捨牌，型別另驗 |
| 捕捉 | at=live/play/resolution/selection/event | 只在已登錄位置；apply.capture=play 另為集合捕捉 |

物件來源、差集、where／as 與區域計數的已定片段見[物件查詢矩陣](author-object-query-shapes-1.0.md)。
其中計數 any_of 保留 counted 角色並先去重；它不表示上表的所有 Sel filter、
領袖／切片來源或選取位置 any_of 都已實作。

## 動作與內層選項

動作共通 `result? / precondition?` 不表示每個短形都可附物件欄位；短形須轉成該動作已登錄的長形。by 是執行者，who 是資源接受者，目的區持有者另判。

| 動作 | payload／額外鍵 | 互斥與上下文 |
| --- | --- | --- |
| draw／discard | who?、n；discard 另 all/down_to/any/sel/ref/include_self/random | 數量變體不混填；draw 不是一次批次 move |
| damage | to、n、distribute?、distribute_at?、instead? | distribute 時點獨立 |
| destroy／banish | Ref 或 {ref}；banish 另 top | top 與 ref 互斥 |
| move | ref\|top、to、position?、order?、reveal?、by?、acted?、entry_effects?、bind?、result? | to 可區域或目的地選項 |
| create | token、n?\|upto\|any、to、counters?、by?、acted?、entry_effects?、bind?、result? | 可 declare→bound 指定量（§13 GAP-A-039）；token 列表每名各 n；allocate 另有總量 |
| act／stand／flip_down／flip_up | Ref 或 {ref} | flip 與 EP 分開 |
| evolve／transform | ref、into?、result?；transform 另 entry_effects? | evolve 能力頭另型別；transform.into 支援 allocate |
| life／set_life | who、delta／n | delta 與 n 不互換 |
| buff／set_stat | on、power?、hp?、until?、instead? | effect.stat/set_stat 是持續效果，另型別 |
| grant／lose_abilities | on、keywords?、abilities?、until? | 不接受舊 grant.effects |
| counter／move_counters | {on,name,delta}／{from,to,name,all?} | cost.counter 使用材料數量，非 delta |
| pp／pp_max／ep／sep | {who,delta}；pp 另 {who,to_max:true} | delta 與 to_max 互斥 |
| reveal／look | ref\|top、bind、result?、by?、random? | random={n}，與 cost.discard.random=true 不同 |
| reveal_until | sel、n、bind、matched | 結果區分公開與匹配 |
| search | sel＋n\|upto 或 groups、to、bind?、cost_sum_le?、distinct_by?、entry_effects?、result? | search 特例與普通 choose 分開；不多次洗牌 |
| shuffle／order | {who}／{ref,to,by?} | A.order 為 canonical ID |
| dice／declare | {bind,who?}／控制流程表 | 骰子與宣告分開 |
| equip | on、token | 裝備不占普通場格 |
| steal／give | ref、to?、precondition? | 同世代控制轉移 |
| stack_plus | Expr | 規則選擇與建立 counter 語義 |
| box／pilot | {ref}／{ref,on} | 不以 boxed effect 代替 box |
| race／feed／gain_drive／drive_check | ref?、n?、who? | 按各規則程序，不是任意動作組合 |
| play_card／play_ability | {ref,cost?}／{of,trigger?,ability?,cost_policy?} | ability 例 {ub:true,n:1}；cost_policy=normal/waive_original |
| delayed | on、do、next_only?、until?、line | on 是事件；預設一次誘發 |
| player_effect／apply | {who,effect,until?}／{on,effect,until?,capture?} | on 與 who 分開；apply.capture=play |
| win／skip_turn／extra_turn | who | 遊戲結果與排程各自降低 |
| make_type | on、type、until? | 不隱式失去能力 |
| to.one_of | one_of、by、scope | create 的 scope=each/batch（§13 GAP-A-017） |
| to.partition | order=printed、partition:[{zone,n}…] | 順序與份額保留 |
| position | top/bottom 或 {nth} 或 {one_of,by} | nth 為 1 起算 |
| token.allocate | among、total、by? | among 為名稱列表；total 為總量 |
| into.allocate | among、each_exactly_once:true | 與 token 的總量分配不同 |

## 事件與歷史的封閉變體

33 個直接事件鍵如下。`any_of` 是事件組合，`counter_reached` 是 counter_changed.after 的比較，均不計成第 34 種。每一列須分開窄化，**不是可套在每個事件的總欄位池**。以下只列作者文件明示的 filter；事件當時可讀的 payload 另依配對 IR 型別，不自動成為 filter。

| 直接事件 | 作者明示的專屬 filter／責任 |
| --- | --- |
| enter_field | entered_by；此次入場的新世代 |
| left_field | 離場主體／move_receipt；不以現在卡號追蹤 |
| placed_ex | 移入 EX 的主體 |
| discarded | ref、from、who；by.ability_of |
| banished | 消滅主體 |
| destroyed | 破壞主體 |
| damaged | ref、recipient、kind；本文 event.amount |
| dealt_damage | ref、recipient、kind=attack/combat/ability；attack 與 combat 可重疊（§13 GAP-A-030）；aggregation=batch 屬 auto 層 |
| attacked | 攻擊主體 |
| attack_target | 攻擊目標 |
| evolved | 進化主體 |
| super_evolved | 超進化主體 |
| phase | which、whose |
| played | who、卡片篩選（如 trait） |
| ability_played | 能力打出，不以 played 代替 |
| life_changed | 生命變更；history_sum.delta 先窄化方向 |
| stat_changed | ref、stat=power/hp 或列表、direction=up/down；不收 subject/property |
| counter_changed | after 比較；作者別名 counter_reached:{ref,name,value} 降為 counter_changed.after 比較（§8、§13 GAP-A-043），不另算直接事件 |
| acted | 姿態實變 |
| stood | 姿態實變 |
| raced | 出走主體 |
| drive_gained | 首次取得 Drive |
| ub_activated | who、holder |
| fused | who、material |
| targeted | 目標事實 |
| state | Cond；狀態誘發 latch |
| dice_rolled | who；本文 event.value |
| drive_checked | holder |
| drive_trigger | who |
| moved | from、to、sel、during?；from 可 {who,zone} |
| drew | who、not_phase；與 played 等事件不同 |
| equipped | equipment；本文 event.holder |
| fused_as_material | ref、by.ability_of |

共同 `by:{ability_of:Sel}` 仍須該事件支持直接來源。history／history_count／history_sum 共用 `{event,who?,zone?,from?,to?,sel?,where?,window,distinct?,exclude?}`，history 可 at_least，history_sum 必有 field。window 為 this_turn/game/since 錨；distinct=facts/subjects，exclude=this_execution。where 僅接受所選事件的 filter。history_sum 的 amount/delta/removed_stack 分別需要相應事件／方向／EarthRite 付款窄化，不能由字串欄名就授予任意 payload 存取。§13 GAP-A-029 使用 removed，§14「欄位正規化補充」使用 removed_stack；本表不把兩者默認為已登錄別名。

## 效果、取代與期限

| 效果鍵／上下文 | 封閉形狀與區別 |
| --- | --- |
| stat／set_stat | 數值增減／設定分開 |
| keywords | {on,list}；不同於 A.ability.keywords 的列表 |
| grant／lose_abilities | 能力增減；不附任意回呼 |
| also_named／add_trait／remove_trait／make_type | 名稱／種族／種類變更，各自有型別 |
| cost_mod | when_playing、delta\|set、next_only?；delta Int、set Nat 葉參數 |
| damage_mod | on?、source?、kind?、delta\|multiply、next_only? |
| banish_instead | 取代移動，不等於直接 A.act.banish |
| cannot | on\|who、what、by?、target?、card?、ability?、during?；安裝上下文可提供範圍 |
| must_attack | 攻擊義務 |
| play_permission | 打出許可 |
| no_trigger | on、kinds；Ref/Binding；入場特例為 entering |
| unlimited_evolve | 進化額度效果 |
| rule_override | 配對 IR 的已登錄規則變體，不是自由規則名稱 |
| prevent | on?、kind=any/combat/ability、source? |
| damage_cap | on?、threshold、set；不是 min |
| evolve_cost_mod | on、delta\|set |
| enters_acted | on；ProspectiveEntry，非額外 act |
| reroll | who、times |
| ignore_ward | on；只略過守護目標限制 |
| choice_any | who；模式自我取代可選零 |
| extra_trigger | phase、who、n；附加次數 |
| must_target | on、by |
| instead[] | if、同種 payload 差異或 replace_with、after? |
| until | turn/next_opp_turn/this_and_next_opp_turn/game；{next_phase:{who,phase,edge}}／{next_turn:{who,edge}}／{during_next_phase:{who,phase}} |
| entry_effects[] | EffectSpec；move/search/create/transform 共用；entering 為本次新世代 |

cannot.what 的 12 種變體為 play_card/play_ability/attack/choose_target/draw/destroy/banish/stand/win/lose/deal_damage/pp_max_increase。card 只用於 play_card，ability 只用於能力篩選；target 與各 what 的精確組合未在 §9 逐項列定，不能從範例另造禁止規則。during 區分 phase/not_phase、who、cause，不能混成任意額外欄位。靜態範圍與 apply 的 affected 預設分開。cannot.ability.except 在 §13 GAP-A-045 的文字為列表、示意為純量，本表保留差異。

instead 僅掛 damage/move/buff/life/grant/choice/create/draw/search/banish/destroy；同種差異保留未改欄位，異種 replace_with 是完整另一動作。choice 數量取代在打出時計算。expiry 與 next_only 的使用額度分開；EntryOccurrence 也不是通用時間期限。

## Canonical ID 與移除語法

| 拼字／分類 | Canonical ID 或處理 |
| --- | --- |
| by_kind／A.by-kind | A.by-kind；不是第二個 A.comb.by_kind |
| define | A.define |
| either | A.either |
| declare | A.declare；保留 v0 ID |
| order | A.order；不另造 A.act.order |
| distribute／place-acted 分類 | A.distribute／A.place-acted；動作欄位展開不是新獨立 op |
| each_player/foreach/repeat/instead/simultaneous | A.comb.<鍵> |
| apply／sep 與其他原子 | A.act.<鍵> |
| dig／discount／search_to | §10 保留的 v0 巨集；按已登錄展開，不接任意模板 |
| when、targets[].if、cost timing、grant.effects、subject/property 舊別名 | 拒絕，不靜默遷移 |
| A.cost-timing-override | 僅遷移診斷，非可執行構造 |
| next_start_phase/next_main_phase/next_owner_turn_end | 不能猜錨點；改為明示期限後再驗 |

## 逐族與補充小節對照

以下路徑只記構造鍵，不保存卡文或真實卡名。`*` 表 define／roles 等動態 binding 名；每列列出範例內各物件的上下文與鍵。這份範例對照加上前述正文表才是清冊，單靠 85 個 YAML 區塊不能證明語法或引擎完整。

### §13：63 族

| 小節 | 範例上下文與鍵 |
| --- | --- |
| GAP-A-001 | `$` → `fanfare`；`fanfare` → `define, do`；`fanfare.define` → `*`；`fanfare.define.*` → `minus`；`fanfare.define.*.minus[]` → `count`；`fanfare.define.*.minus[].count` → `who, zone`；`fanfare.do[]` → `draw`；`fanfare.do[].draw` → `n` |
| GAP-A-002 | `$` → `evolve_time`；`evolve_time` → `do`；`evolve_time.do[]` → `create`；`evolve_time.do[].create` → `n, to, token` |
| GAP-A-003 | `$` → `activate`；`activate` → `cost, do`；`activate.cost` → `act`；`activate.cost.act` → `n, sel`；`activate.cost.act.sel` → `kind, who, zone`；`activate.do[]` → `draw`；`activate.do[].draw` → `n` |
| GAP-A-004 | `$` → `spell`；`spell` → `do`；`spell.do[]` → `either`；`spell.do[].either` → `options`；`spell.do[].either.options[][]` → `create`；`spell.do[].either.options[][].create` → `n, to, token` |
| GAP-A-005 | `$` → `static`；`static` → `effects, while`；`static.while` → `cmp`；`static.while.cmp` → `left, op, right`；`static.while.cmp.left` → `resource`；`static.while.cmp.left.resource` → `kind, who`；`static.effects[]` → `keywords`；`static.effects[].keywords` → `list, on` |
| GAP-A-006 | `$` → `fanfare`；`fanfare` → `do`；`fanfare.do[]` → `apply, move`；`fanfare.do[].move` → `result, to, top`；`fanfare.do[].move.top` → `n, who`；`fanfare.do[].apply` → `effect, on`；`fanfare.do[].apply.on` → `follow`；`fanfare.do[].apply.effect` → `cost_mod`；`fanfare.do[].apply.effect.cost_mod` → `delta, when_playing`；`fanfare.do[].apply.effect.cost_mod.when_playing` → `in` |
| GAP-A-007 | `$` → `attack_time`；`attack_time` → `do`；`attack_time.do[]` → `apply`；`attack_time.do[].apply` → `effect, on, until`；`attack_time.do[].apply.effect` → `prevent`；`attack_time.do[].apply.effect.prevent` → `kind` |
| GAP-A-008 | `$` → `static`；`static` → `effects`；`static.effects[]` → `cannot`；`static.effects[].cannot` → `during, on, what`；`static.effects[].cannot.during` → `cause, phase, who` |
| GAP-A-009 | `$` → `spell`；`spell` → `do`；`spell.do[]` → `create`；`spell.do[].create` → `instead, n, to, token`；`spell.do[].create.instead[]` → `if, n`；`spell.do[].create.instead[].if` → `awakened` |
| GAP-A-010 | `$` → `fanfare`；`fanfare` → `do`；`fanfare.do[]` → `banish`；`fanfare.do[].banish` → `top`；`fanfare.do[].banish.top` → `n, who` |
| GAP-A-011 | `$` → `spell`；`spell` → `do`；`spell.do[]` → `move`；`spell.do[].move` → `to, top`；`spell.do[].move.top` → `n, who` |
| GAP-A-012 | `$` → `fanfare`；`fanfare` → `cost, do`；`fanfare.cost` → `to_deck_bottom`；`fanfare.cost.to_deck_bottom` → `n, sel`；`fanfare.cost.to_deck_bottom.sel` → `who, zone`；`fanfare.do[]` → `life`；`fanfare.do[].life` → `delta, who` |
| GAP-A-013 | `$` → `fanfare`；`fanfare` → `do`；`fanfare.do[]` → `if`；`fanfare.do[].if` → `cond, then`；`fanfare.do[].if.cond` → `entered_by`；`fanfare.do[].if.cond.entered_by` → `by, ref`；`fanfare.do[].if.then[]` → `draw`；`fanfare.do[].if.then[].draw` → `n` |
| GAP-A-014 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `ub_activated`；`auto.on.ub_activated` → `holder, who`；`auto.on.ub_activated.holder` → `kind, other, who, zone`；`auto.do[]` → `pp_max`；`auto.do[].pp_max` → `delta, who` |
| GAP-A-015 | `$` → `static`；`static` → `effects`；`static.effects[]` → `enters_acted`；`static.effects[].enters_acted` → `on` |
| GAP-A-016 | `$` → `static`；`static` → `effects`；`static.effects[]` → `cannot`；`static.effects[].cannot` → `during, what, who`；`static.effects[].cannot.during` → `not_phase` |
| GAP-A-017 | `$` → `fanfare`；`fanfare` → `do`；`fanfare.do[]` → `optional`；`fanfare.do[].optional` → `do`；`fanfare.do[].optional.do[]` → `create`；`fanfare.do[].optional.do[].create` → `n, to, token`；`fanfare.do[].optional.do[].create.to` → `by, one_of, scope` |
| GAP-A-018 | `$` → `activate`；`activate` → `cost, do, zone`；`activate.cost` → `steps`；`activate.cost.steps[]` → `discard, pp`；`activate.cost.steps[].discard` → `groups`；`activate.cost.steps[].discard.groups[]` → `n, sel`；`activate.cost.steps[].discard.groups[].sel` → `class, in, orig_cost_ge, who, zone`；`activate.do[]` → `draw`；`activate.do[].draw` → `n` |
| GAP-A-019 | `$` → `deck_rule`；`deck_rule` → `deck, same_name_max` |
| GAP-A-020 | `$` → `attack_time`；`attack_time` → `do`；`attack_time.do[]` → `damage`；`attack_time.do[].damage` → `n, to`；`attack_time.do[].damage.to` → `leaders` |
| GAP-A-021 | `$` → `static`；`static` → `effects`；`static.effects[]` → `cannot`；`static.effects[].cannot` → `by, on, what` |
| GAP-A-022 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `stat_changed`；`auto.on.stat_changed` → `direction, ref, stat`；`auto.do[]` → `grant`；`auto.do[].grant` → `keywords, on` |
| GAP-A-023 | `$` → `static`；`static` → `effects`；`static.effects[]` → `cannot`；`static.effects[].cannot` → `on, target, what`；`static.effects[].cannot.target` → `leaders` |
| GAP-A-024 | `$` → `attack_time`；`attack_time` → `do`；`attack_time.do[]` → `if`；`attack_time.do[].if` → `cond, then`；`attack_time.do[].if.cond` → `is`；`attack_time.do[].if.cond.is` → `kind, ref`；`attack_time.do[].if.then[]` → `buff`；`attack_time.do[].if.then[].buff` → `on, power` |
| GAP-A-025 | `$` → `static`；`static` → `effects`；`static.effects[]` → `evolve_cost_mod`；`static.effects[].evolve_cost_mod` → `delta, on`；`static.effects[].evolve_cost_mod.delta` → `minus`；`static.effects[].evolve_cost_mod.delta.minus[]` → `count`；`static.effects[].evolve_cost_mod.delta.minus[].count` → `kind, other, who, zone` |
| GAP-A-026 | `$` → `evolve_time`；`evolve_time` → `do`；`evolve_time.do[]` → `search`；`evolve_time.do[].search` → `cost_sum_le, sel, to, upto`；`evolve_time.do[].search.sel` → `kind, trait, who, zone` |
| GAP-A-027 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `drive_trigger`；`auto.on.drive_trigger` → `who`；`auto.do[]` → `grant`；`auto.do[].grant` → `keywords, on` |
| GAP-A-028 | `$` → `evolve_time`；`evolve_time` → `define, do`；`evolve_time.define` → `*`；`evolve_time.define.*` → `agg`；`evolve_time.define.*.agg` → `attr, fn, of`；`evolve_time.define.*.agg.of` → `kind, who, zone`；`evolve_time.do[]` → `destroy`；`evolve_time.do[].destroy` → `ref`；`evolve_time.do[].destroy.ref` → `kind, where, who, zone`；`evolve_time.do[].destroy.ref.where` → `cmp`；`evolve_time.do[].destroy.ref.where.cmp` → `left, op, right`；`evolve_time.do[].destroy.ref.where.cmp.left` → `cost_of`；`evolve_time.do[].destroy.ref.where.cmp.right` → `bound` |
| GAP-A-029 | `$` → `evolve_time`；`evolve_time` → `do`；`evolve_time.do[]` → `destroy`；`evolve_time.do[].destroy` → `ref`；`evolve_time.do[].destroy.ref` → `kind, where, who, zone`；`evolve_time.do[].destroy.ref.where` → `history`；`evolve_time.do[].destroy.ref.where.history` → `at_least, event, where, window`；`evolve_time.do[].destroy.ref.where.history.where` → `recipient` |
| GAP-A-030 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `dealt_damage`；`auto.on.dealt_damage` → `kind, recipient, ref`；`auto.on.dealt_damage.recipient` → `leaders`；`auto.do[]` → `win`；`auto.do[].win` → `who` |
| GAP-A-031 | `$` → `play`；`play` → `optional_cost, optional_cost_delta`；`play.optional_cost` → `counter`；`play.optional_cost.counter` → `holders, n, name, on`；`play.optional_cost.counter.on` → `name, who, zone` |
| GAP-A-032 | `$` → `evolve_time`；`evolve_time` → `do, targets`；`evolve_time.targets[]` → `bind, from, n`；`evolve_time.targets[].from` → `hp_le, kind, who, zone`；`evolve_time.do[]` → `move`；`evolve_time.do[].move` → `by, position, ref, to`；`evolve_time.do[].move.by` → `master_of` |
| GAP-A-033 | `$` → `attack_time`；`attack_time` → `do, ub`；`attack_time.do[]` → `buff`；`attack_time.do[].buff` → `instead, on, power`；`attack_time.do[].buff.instead[]` → `if, power`；`attack_time.do[].buff.instead[].if` → `history`；`attack_time.do[].buff.instead[].if.history` → `at_least, event, exclude, who, window` |
| GAP-A-034 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `ub_activated`；`auto.on.ub_activated` → `holder, who`；`auto.on.ub_activated.holder` → `kind, other, who, zone`；`auto.do[]` → `choice`；`auto.do[].choice` → `exclude_chosen, n, options`；`auto.do[].choice.options[][]` → `damage, life`；`auto.do[].choice.options[][].damage` → `n, to`；`auto.do[].choice.options[][].damage.to` → `leaders`；`auto.do[].choice.options[][].life` → `delta, who` |
| GAP-A-035 | `$` → `evolve_time`；`evolve_time` → `do`；`evolve_time.do[]` → `pp, reveal`；`evolve_time.do[].reveal` → `bind, ref, result`；`evolve_time.do[].reveal.ref` → `who, zone`；`evolve_time.do[].pp` → `delta, who`；`evolve_time.do[].pp.delta` → `count_of`；`evolve_time.do[].pp.delta.count_of` → `receipt, view, where`；`evolve_time.do[].pp.delta.count_of.where` → `orig_cost_eq` |
| GAP-A-036 | `$` → `spell`；`spell` → `do, targets`；`spell.targets[]` → `bind, from, n`；`spell.targets[].from` → `kind, who, zone`；`spell.do[]` → `damage`；`spell.do[].damage` → `instead, n, to`；`spell.do[].damage.instead[]` → `if, n`；`spell.do[].damage.instead[].if` → `exists`；`spell.do[].damage.instead[].if.exists` → `kind, racing, who, zone` |
| GAP-A-037 | `$` → `evolve`；`evolve` → `cost, into`；`evolve.cost` → `pp` |
| GAP-A-038 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `dice_rolled`；`auto.on.dice_rolled` → `who`；`auto.do[]` → `if`；`auto.do[].if` → `cond, then`；`auto.do[].if.cond` → `cmp`；`auto.do[].if.cond.cmp` → `left, op, right`；`auto.do[].if.cond.cmp.left` → `event`；`auto.do[].if.then[]` → `damage`；`auto.do[].if.then[].damage` → `n, to`；`auto.do[].if.then[].damage.to` → `leaders` |
| GAP-A-039 | `$` → `spell`；`spell` → `do`；`spell.do[]` → `create`；`spell.do[].create` → `instead, n, to, token`；`spell.do[].create.instead[]` → `if, upto`；`spell.do[].create.instead[].if` → `awakened` |
| GAP-A-040 | `$` → `play`；`play` → `optional_cost, optional_cost_set`；`play.optional_cost` → `banish`；`play.optional_cost.banish` → `n, sel`；`play.optional_cost.banish.sel` → `kind, who, zone` |
| GAP-A-041 | `$` → `fanfare`；`fanfare` → `cost, do`；`fanfare.cost` → `return_to_hand`；`fanfare.cost.return_to_hand` → `any, result, sel`；`fanfare.cost.return_to_hand.sel` → `other, trait, who, zone`；`fanfare.do[]` → `damage`；`fanfare.do[].damage` → `n, to`；`fanfare.do[].damage.to` → `kind, who, zone`；`fanfare.do[].damage.n` → `times`；`fanfare.do[].damage.n.times[]` → `count_of` |
| GAP-A-042 | `$` → `evolve`；`evolve` → `cost`；`evolve.cost` → `skip_turn`；`evolve.cost.skip_turn` → `who` |
| GAP-A-043 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `any_of`；`auto.on.any_of[]` → `fused, played`；`auto.on.any_of[].fused` → `material, who`；`auto.on.any_of[].fused.material` → `trait`；`auto.on.any_of[].played` → `trait, who`；`auto.do[]` → `grant`；`auto.do[].grant` → `keywords, on` |
| GAP-A-044 | `$` → `evolve_time`；`evolve_time` → `do`；`evolve_time.do[]` → `apply, choose, move`；`evolve_time.do[].choose` → `any, bind, from, may`；`evolve_time.do[].choose.from` → `kind, who, zone`；`evolve_time.do[].move` → `entry_effects, ref, result, to`；`evolve_time.do[].move.entry_effects[]` → `cannot`；`evolve_time.do[].move.entry_effects[].cannot` → `ability, what`；`evolve_time.do[].move.entry_effects[].cannot.ability` → `keyword`；`evolve_time.do[].apply` → `effect, on, until`；`evolve_time.do[].apply.on` → `follow`；`evolve_time.do[].apply.effect` → `cannot`；`evolve_time.do[].apply.effect.cannot` → `target, what`；`evolve_time.do[].apply.effect.cannot.target` → `kind, leader_or, who` |
| GAP-A-045 | `$` → `activate`；`activate` → `cost, do, targets`；`activate.cost` → `lesson`；`activate.targets[]` → `bind, from, n`；`activate.targets[].from` → `orig_cost_le, other, who, zone`；`activate.do[]` → `apply, move`；`activate.do[].move` → `ref, to`；`activate.do[].apply` → `effect, on, until`；`activate.do[].apply.effect` → `cannot`；`activate.do[].apply.effect.cannot` → `ability, what`；`activate.do[].apply.effect.cannot.ability` → `kind` |
| GAP-A-046 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `discarded`；`auto.on.discarded` → `by, from, ref, who`；`auto.on.discarded.by` → `ability_of`；`auto.on.discarded.by.ability_of` → `trait, who`；`auto.do[]` → `draw`；`auto.do[].draw` → `n` |
| GAP-A-047 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `drive_checked`；`auto.on.drive_checked` → `holder`；`auto.on.drive_checked.holder` → `kind, who, zone`；`auto.do[]` → `buff`；`auto.do[].buff` → `on, power` |
| GAP-A-048 | `$` → `activate`；`activate` → `do, fusion, zone`；`activate.fusion` → `n, result, sel`；`activate.fusion.sel` → `orig_cost_ge, trait, who`；`activate.do[]` → `counter`；`activate.do[].counter` → `delta, name, on` |
| GAP-A-049 | `$` → `static`；`static` → `effects`；`static.effects[]` → `must_target`；`static.effects[].must_target` → `by, on` |
| GAP-A-050 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `fused_as_material`；`auto.on.fused_as_material` → `by, ref`；`auto.on.fused_as_material.by` → `ability_of`；`auto.on.fused_as_material.by.ability_of` → `kind, trait, who`；`auto.do[]` → `discard, draw`；`auto.do[].draw` → `n`；`auto.do[].discard` → `n, who` |
| GAP-A-051 | `$` → `evolve_time`；`evolve_time` → `do, targets`；`evolve_time.targets[]` → `bind, from, n`；`evolve_time.targets[].from` → `kind, leader_or, who, zone`；`evolve_time.do[]` → `by_kind`；`evolve_time.do[].by_kind` → `follower, leader, ref`；`evolve_time.do[].by_kind.leader[]` → `life`；`evolve_time.do[].by_kind.leader[].life` → `delta, who`；`evolve_time.do[].by_kind.follower[]` → `buff`；`evolve_time.do[].by_kind.follower[].buff` → `hp, on` |
| GAP-A-052 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `moved`；`auto.on.moved` → `during, from, to`；`auto.on.moved.from` → `who, zone`；`auto.on.moved.during` → `turn`；`auto.do[]` → `buff`；`auto.do[].buff` → `on, power` |
| GAP-A-053 | `$` → `activate`；`activate` → `cost, do`；`activate.cost` → `steps`；`activate.cost.steps[]` → `act_self, banish`；`activate.cost.steps[].banish` → `distinct_by, n, sel`；`activate.cost.steps[].banish.sel` → `who, zone`；`activate.do[]` → `choose, move`；`activate.do[].choose` → `bind, from, may, n`；`activate.do[].choose.from` → `name, who, zone`；`activate.do[].move` → `ref, to` |
| GAP-A-054 | `$` → `fanfare`；`fanfare` → `do`；`fanfare.do[]` → `if`；`fanfare.do[].if` → `cond, then`；`fanfare.do[].if.cond` → `did`；`fanfare.do[].if.cond.did` → `result, where`；`fanfare.do[].if.cond.did.result` → `entry_payment`；`fanfare.do[].if.cond.did.where` → `name, zone`；`fanfare.do[].if.then[]` → `evolve`；`fanfare.do[].if.then[].evolve` → `ref` |
| GAP-A-055 | `$` → `fanfare`；`fanfare` → `do`；`fanfare.do[]` → `search`；`fanfare.do[].search` → `instead, n, sel, to`；`fanfare.do[].search.sel` → `kind, trait, who, zone`；`fanfare.do[].search.instead[]` → `if, may, to`；`fanfare.do[].search.instead[].if` → `cmp`；`fanfare.do[].search.instead[].if.cmp` → `left, op, right`；`fanfare.do[].search.instead[].if.cmp.left` → `cost_of` |
| GAP-A-056 | `$` → `play`；`play` → `cannot_if`；`play.cannot_if` → `cmp`；`play.cannot_if.cmp` → `left, op, right`；`play.cannot_if.cmp.left` → `turn_number`；`play.cannot_if.cmp.left.turn_number` → `who` |
| GAP-A-057 | `$` → `evolve_time`；`evolve_time` → `cost, do`；`evolve_time.cost` → `banish`；`evolve_time.cost.banish` → `n, result, sel`；`evolve_time.cost.banish.sel` → `kind, who, zone`；`evolve_time.do[]` → `search`；`evolve_time.do[].search` → `sel, to, upto`；`evolve_time.do[].search.sel` → `kind, name, who, zone`；`evolve_time.do[].search.sel.name` → `same_as`；`evolve_time.do[].search.sel.name.same_as` → `objects_of, view` |
| GAP-A-058 | `$` → `auto`；`auto` → `do, on, targets`；`auto.on` → `equipped`；`auto.on.equipped` → `equipment`；`auto.targets[]` → `bind, from, n`；`auto.targets[].from` → `kind, who, zone`；`auto.do[]` → `act, buff`；`auto.do[].buff` → `hp, on, power`；`auto.do[].buff.on` → `event` |
| GAP-A-059 | `$` → `static`；`static` → `effects, zone`；`static.effects[]` → `keywords`；`static.effects[].keywords` → `list, on`；`static.effects[].keywords.on` → `kind, token, trait, who, zone` |
| GAP-A-060 | `$` → `lastword`；`lastword` → `do`；`lastword.do[]` → `pay`；`lastword.do[].pay` → `cost, do`；`lastword.do[].pay.cost` → `banish`；`lastword.do[].pay.cost.banish` → `n, sel`；`lastword.do[].pay.cost.banish.sel` → `name, who, zone`；`lastword.do[].pay.do[]` → `evolve, if, move`；`lastword.do[].pay.do[].move` → `acted, ref, result, to`；`lastword.do[].pay.do[].move.ref` → `follow`；`lastword.do[].pay.do[].move.ref.follow` → `event`；`lastword.do[].pay.do[].evolve` → `ref, result`；`lastword.do[].pay.do[].evolve.ref` → `follow`；`lastword.do[].pay.do[].if` → `cond, then`；`lastword.do[].pay.do[].if.cond` → `not`；`lastword.do[].pay.do[].if.cond.not` → `did`；`lastword.do[].pay.do[].if.then[]` → `banish`；`lastword.do[].pay.do[].if.then[].banish` → `ref`；`lastword.do[].pay.do[].if.then[].banish.ref` → `follow` |
| GAP-A-061 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `damaged`；`auto.on.damaged` → `ref`；`auto.do[]` → `if`；`auto.do[].if` → `cond, then`；`auto.do[].if.cond` → `cmp`；`auto.do[].if.cond.cmp` → `left, op, right`；`auto.do[].if.cond.cmp.left` → `event`；`auto.do[].if.then[]` → `pp_max`；`auto.do[].if.then[].pp_max` → `delta, who` |
| GAP-A-062 | `$` → `spell`；`spell` → `do, targets`；`spell.targets[]` → `bind, from, n`；`spell.targets[].from` → `kind, who, zone`；`spell.do[]` → `apply`；`spell.do[].apply` → `effect, on, until`；`spell.do[].apply.effect` → `cannot`；`spell.do[].apply.effect.cannot` → `what` |
| GAP-A-063 | `$` → `auto`；`auto` → `do, on`；`auto.on` → `phase`；`auto.on.phase` → `which, whose`；`auto.do[]` → `choice`；`auto.do[].choice` → `n, options, who`；`auto.do[].choice.options[][]` → `player_effect`；`auto.do[].choice.options[][].player_effect` → `effect, until, who`；`auto.do[].choice.options[][].player_effect.effect` → `cannot`；`auto.do[].choice.options[][].player_effect.effect.cannot` → `card, during, what`；`auto.do[].choice.options[][].player_effect.effect.cannot.during` → `cause, phase`；`auto.do[].choice.options[][].player_effect.until` → `during_next_phase`；`auto.do[].choice.options[][].player_effect.until.during_next_phase` → `phase, who`；`auto.do[].choice.options[][].player_effect.effect.cannot.card` → `kind` |

### §14：22 補充小節

| 小節 | 範例上下文與鍵 |
| --- | --- |
| A-016a 隨機公開手牌 | `$` → `activate`；`activate` → `cost, do`；`activate.cost` → `steps`；`activate.cost.steps[]` → `act_self, self_to_cemetery`；`activate.do[]` → `move, reveal`；`activate.do[].reveal` → `bind, random, ref`；`activate.do[].reveal.ref` → `who, zone`；`activate.do[].reveal.random` → `n`；`activate.do[].move` → `ref, to`；`activate.do[].move.ref` → `in, kind` |
| A-016b 從頂第二張或底 | `$` → `lastword`；`lastword` → `do`；`lastword.do[]` → `damage, move`；`lastword.do[].damage` → `n, to`；`lastword.do[].damage.to` → `leaders`；`lastword.do[].move` → `position, ref, to`；`lastword.do[].move.ref` → `follow`；`lastword.do[].move.ref.follow` → `event`；`lastword.do[].move.position` → `by, one_of`；`lastword.do[].move.position.one_of[]` → `nth` |
| A-016c repeat 累積結果 | `$` → `spell`；`spell` → `do`；`spell.do[]` → `player_effect, repeat`；`spell.do[].repeat` → `check, collect, do, until`；`spell.do[].repeat.until` → `cmp`；`spell.do[].repeat.until.cmp` → `left, op, right`；`spell.do[].repeat.until.cmp.left` → `zone_free`；`spell.do[].repeat.until.cmp.left.zone_free` → `who, zone`；`spell.do[].repeat.collect` → `receipts`；`spell.do[].repeat.do[]` → `move`；`spell.do[].repeat.do[].move` → `to, top`；`spell.do[].repeat.do[].move.top` → `n, who`；`spell.do[].player_effect` → `effect, until, who`；`spell.do[].player_effect.effect` → `cost_mod`；`spell.do[].player_effect.effect.cost_mod` → `next_only, set, when_playing`；`spell.do[].player_effect.effect.cost_mod.when_playing` → `in`；`spell.do[].player_effect.effect.cost_mod.when_playing.in` → `follow` |
| A-016d 兩種付款擇一 | `$` → `play`；`play` → `optional_cost, optional_cost_delta`；`play.optional_cost` → `one_of`；`play.optional_cost.one_of[]` → `banish, to_cemetery`；`play.optional_cost.one_of[].to_cemetery` → `n, sel`；`play.optional_cost.one_of[].to_cemetery.sel` → `acted, class, who, zone`；`play.optional_cost.one_of[].banish` → `n, sel`；`play.optional_cost.one_of[].banish.sel` → `class, who, zone` |
| A-016e 免原費用打出能力 | `$` → `play_ability`；`play_ability` → `ability, cost_policy, of`；`play_ability.ability` → `n, ub` |
| A-016f create 綁定 | `$` → `evolve_time`；`evolve_time` → `do`；`evolve_time.do[]` → `apply, create, damage`；`evolve_time.do[].damage` → `n, to`；`evolve_time.do[].damage.to` → `kind, who, zone`；`evolve_time.do[].create` → `bind, n, to, token`；`evolve_time.do[].apply` → `effect, on, until`；`evolve_time.do[].apply.effect` → `cost_mod`；`evolve_time.do[].apply.effect.cost_mod` → `delta, when_playing`；`evolve_time.do[].apply.effect.cost_mod.when_playing` → `in` |
| A-016g 分配 token 張數 | `$` → `activate`；`activate` → `cost, do`；`activate.cost` → `steps`；`activate.cost.steps[]` → `earth_rite, pp`；`activate.do[]` → `create`；`activate.do[].create` → `to, token`；`activate.do[].create.token` → `allocate`；`activate.do[].create.token.allocate` → `among, by, total` |
| A-016h first/rest 目標角色 | `$` → `fanfare`；`fanfare` → `do, targets`；`fanfare.targets[]` → `bind, from, roles, upto`；`fanfare.targets[].from` → `kind, who, zone`；`fanfare.targets[].roles` → `*`；`fanfare.targets[].roles.*` → `if_nonempty`；`fanfare.do[]` → `damage`；`fanfare.do[].damage` → `n, to`；`fanfare.do[].damage.to` → `role`；`fanfare.do[].damage.to.role` → `name, of` |
| A-016i 同時放置現有物件與 token | `$` → `lastword`；`lastword` → `do`；`lastword.do[]` → `counter, simultaneous`；`lastword.do[].simultaneous` → `do`；`lastword.do[].simultaneous.do[]` → `create, move`；`lastword.do[].simultaneous.do[].move` → `ref, result, to`；`lastword.do[].simultaneous.do[].move.ref` → `follow`；`lastword.do[].simultaneous.do[].move.ref.follow` → `event`；`lastword.do[].simultaneous.do[].create` → `n, to, token`；`lastword.do[].counter` → `delta, name, on`；`lastword.do[].counter.on` → `follow` |
| A-016j 變身後分配 token 張數 | `$` → `spell`；`spell` → `do, targets`；`spell.targets[]` → `bind, from, n`；`spell.targets[].from` → `name, who, zone`；`spell.do[]` → `transform`；`spell.do[].transform` → `into, ref`；`spell.do[].transform.into` → `allocate`；`spell.do[].transform.into.allocate` → `among, each_exactly_once` |
| A-016k 抽牌階段篩選 | `$` → `static`；`static` → `effects`；`static.effects[]` → `cannot`；`static.effects[].cannot` → `during, what, who`；`static.effects[].cannot.during` → `not_phase` |
| A-042a PP 上限費用 | `$` → `lastword`；`lastword` → `do`；`lastword.do[]` → `pay`；`lastword.do[].pay` → `cost, do`；`lastword.do[].pay.cost` → `pp_max`；`lastword.do[].pay.do[]` → `move`；`lastword.do[].pay.do[].move` → `acted, ref, to`；`lastword.do[].pay.do[].move.ref` → `follow`；`lastword.do[].pay.do[].move.ref.follow` → `event` |
| A-042b 捨手牌自身 | `$` → `activate`；`activate` → `cost, do, zone`；`activate.cost` → `discard`；`activate.cost.discard` → `groups`；`activate.cost.discard.groups[]` → `n, sel`；`activate.cost.discard.groups[].sel` → `in, kind, traits_all, who, zone`；`activate.do[]` → `create`；`activate.do[].create` → `n, to, token` |
| A-042c 隨機捨棄成本 | `$` → `fanfare`；`fanfare` → `cost, do`；`fanfare.cost` → `discard`；`fanfare.cost.discard` → `n, random, sel`；`fanfare.cost.discard.sel` → `who, zone`；`fanfare.do[]` → `buff, grant`；`fanfare.do[].buff` → `on, power`；`fanfare.do[].grant` → `keywords, on` |
| GAP-A-006：移動後 set 0 與 delta -2 | 第一個 YAML 區塊 `$` → `fanfare`，第二個 YAML 區塊 `$` → `evolve_time`（兩個獨立能力）；`fanfare` → `do, targets`；`fanfare.targets[]` → `bind, cost_sum_le, from, upto`；`fanfare.targets[].from` → `trait, who, zone`；`fanfare.do[]` → `apply, move`；`fanfare.do[].move` → `ref, result, to`；`fanfare.do[].apply` → `effect, on, until`；`fanfare.do[].apply.on` → `follow`；`fanfare.do[].apply.effect` → `cost_mod`；`fanfare.do[].apply.effect.cost_mod` → `set, when_playing`；`fanfare.do[].apply.effect.cost_mod.when_playing` → `in`；`evolve_time` → `do, ub`；`evolve_time.do[]` → `apply, search`；`evolve_time.do[].search` → `n, result, sel, to`；`evolve_time.do[].search.sel` → `not_same_name_as_this, trait, who, zone`；`evolve_time.do[].apply` → `effect, on, until`；`evolve_time.do[].apply.on` → `follow`；`evolve_time.do[].apply.effect` → `cost_mod`；`evolve_time.do[].apply.effect.cost_mod` → `delta, when_playing`；`evolve_time.do[].apply.effect.cost_mod.when_playing` → `in` |
| GAP-A-040：替代費用不是 set 0 | `$` → `play`；`play` → `alt_cost`；`play.alt_cost` → `cost, if, may`；`play.alt_cost.if` → `cmp`；`play.alt_cost.if.cmp` → `left, op, right`；`play.alt_cost.if.cmp.left` → `count`；`play.alt_cost.if.cmp.left.count` → `trait, who, zone`；`play.alt_cost.cost` → `to_cemetery`；`play.alt_cost.cost.to_cemetery` → `n, sel`；`play.alt_cost.cost.to_cemetery.sel` → `name, who, zone` |
| GAP-A-041：X 取實付 counter | `$` → `activate`；`activate` → `cost, do, x`；`activate.x` → `from_payment`；`activate.cost` → `steps`；`activate.cost.steps[]` → `counter, pp`；`activate.cost.steps[].counter` → `any, name, on, result`；`activate.do[]` → `buff`；`activate.do[].buff` → `hp, on, power` |
| GAP-A-055：分配到手牌與墓地 | `$` → `search`；`search` → `sel, to, upto`；`search.sel` → `kind, name_contains, who, zone`；`search.to` → `order, partition`；`search.to.partition[]` → `n, zone` |
| 影響集合的捕捉不是能力授予 | `$` → `evolve_time`；`evolve_time` → `do`；`evolve_time.do[]` → `apply`；`evolve_time.do[].apply` → `capture, effect, on, until`；`evolve_time.do[].apply.on` → `kind, who, zone`；`evolve_time.do[].apply.effect` → `cannot`；`evolve_time.do[].apply.effect.cannot` → `what` |
| 欄位正規化補充 | 無 YAML；Sel.in、traits_all、token、zone、zone_free、cost.counter、effect.on、付款結果與 history_sum 見正文表。 |
| GAP-A-044：本次入場不誘發 fanfare | `$` → `fanfare`；`fanfare` → `do`；`fanfare.do[]` → `search`；`fanfare.do[].search` → `entry_effects, groups, to`；`fanfare.do[].search.groups[]` → `n, sel`；`fanfare.do[].search.groups[].sel` → `kind, orig_cost_eq, orig_cost_le, trait, who, zone`；`fanfare.do[].search.entry_effects[]` → `no_trigger`；`fanfare.do[].search.entry_effects[].no_trigger` → `kinds, on` |
| 批次誘發與封閉變體補記 | `$` → `auto`；`auto` → `aggregation, do, on`；`auto.on` → `dealt_damage`；`auto.on.dealt_damage` → `kind, recipient, ref`；`auto.on.dealt_damage.recipient` → `kind, who, zone`；`auto.do[]` → `damage`；`auto.do[].damage` → `n, to`；`auto.do[].damage.to` → `leaders` |
