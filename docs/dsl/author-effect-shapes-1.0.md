# 作者效果形狀 1.0

本文件定義 `A.effect`（EffectSpec）的封閉形狀及安裝位置契約。
語法唯一權威為 [`author-nodes.schema.json`](../../dsl/author-nodes.schema.json)；
Python 對應型別在 `contracts/author_effects.py`，與 selector／數值共用同一遞迴圖。
本片段不是完整能力、Steps、作者入口或 expanded profile，不能據此宣稱可執行。

## 1. 共通邊界

EffectSpec 是恰含一個效果鍵的物件；下面接受 26 鍵。§9 登錄的第 27 鍵
`grant` 尚未接入完整能力頭與 Steps，C0 目前拒絕它，包括 `{grant:{}}`
與 `{grant:{keywords:[storm]}}`；不以任意 JSON 代替授予能力。
所有 payload 封閉，未列欄位、null 與錯型別拒絕，不補預設欄位。

型別簡寫如下；未特別允許的位置不接受直接 `{param:…}`。

| 簡寫 | 型別／限制 |
| --- | --- |
| O | `Ref` 或 `ObjectSel`，不含領袖 |
| R | `Ref` 或 `RecipientSel`，可含領袖 |
| P | `PlayerRef` 或 `PlayerSet` |
| N | `NumericExpr`；整數常值可負，bool 不算整數 |
| Q | `Quantity`；整數常值非負，運算式的結果仍由 C1／D0 檢查 |
| Text | 非空文字；name／trait／keyword 共用 CardName／Concept 葉型別 |
| Code | 既有作者代碼；形狀不保證 registry 已登錄 |
| Phase | start／main／end |
| Zone | field／hand／deck／evolve_deck／cemetery／ex |
| TriggerKind | fanfare／lastword／evolve_time／super_evolve_time／attack_time／race_time／drive_time |

O／R 見[物件查詢](author-object-query-shapes-1.0.md)與[選取形狀](author-selection-shapes-1.0.md)，
P／Ref 見[參照形狀](author-reference-shapes-1.0.md)，N／Q 見[運算式形狀](author-expression-shapes-1.0.md)。
各葉原有的巢狀參數權限仍成立，例如 N 內 plus 的已登錄參數位置；效果不增加通用參數權限。
所有列表均保序、保留重數；不新增 uniqueItems。
canonical 只依[承載契約 §4.1](author-contract-1.0.md#41-body-recipe)處理，
不補語義預設、不做代數化簡；省略與明寫 `next_only:false`、`kind:any` 或
`on:affected` 不推定同 hash。

## 2. EffectSpec 矩陣

「二擇一」均為恰一，不可兩者同時出現。所有 bool 嚴格拒絕 0／1／字串。

| 鍵 | 必填 | 選填 | 額外約束／語義 |
| --- | --- | --- | --- |
| stat | power:N 或 hp:N 至少一個 | on:O | 同時改兩項合法 |
| set_stat | power:Q 或 hp:Q 至少一個 | on:O | 同時設兩項合法 |
| keywords | list:非空 Text[] | on:O | 與 Sel.keyword 共用 Concept；不是直接參數位置 |
| lose_abilities | 無 | on:O | AllAtTime；空 payload 可由父位置提供範圍 |
| also_named | name:Text | on:O | CardName 葉 |
| add_trait、remove_trait | trait:Text | on:O | Concept 葉 |
| make_type | type:Code | on:O | CardKind 葉 |
| cost_mod | when_playing:ObjectSel；delta:N 或 set:Q 二擇一 | next_only:bool | delta／set 另收直接 ParamRef；when_playing 不收裸 Ref |
| damage_mod | delta:N 或 multiply:Q 二擇一 | on:R、source:O、kind:any/attack/combat/ability、next_only:bool | 有 source 而無 on 的例外見 §4 |
| prevent | kind:any/combat/ability | on:R、source:O、next_only:bool | 不收 attack；next_only 是使用額度 |
| damage_cap | threshold:Q、set:Q | on:R | amount ≥ threshold 才改為 set，不能默認 min |
| evolve_cost_mod | on:O；delta:N 或 set:Q 二擇一 | 無 | 不繼承 cost_mod 的直接參數權限 |
| banish_instead | from:field、to:cemetery | on:O | 兩個值是常值，表示這段移動改為消滅；不是任意 Zone 配對 |
| cannot | what 與對應欄位見 §3 | 見 §3 | 封閉變體聯集 |
| must_attack、enters_acted、ignore_ward | on:O | 無 | 範圍必須明寫 |
| play_permission | on:O、from:非空 Zone[] | 無 | 不增加 banish／banished 區域；玩家許可仍待定 |
| no_trigger | kinds:非空 TriggerKind[]；on:Ref 或 who:P 二擇一 | 無 | who 指能力控制者；on 不接受 selector |
| unlimited_evolve、choice_any | who:P | 無 | 不提供 rule_override 同義入口 |
| reroll | who:P、times:Q | 無 | 次數計算語義交規則裁定 |
| extra_trigger | phase:Phase、who:P、n:Q | 無 | who 指能力控制者，n 是附加次數 |
| must_target | on:O、by:P | 無 | by 是受此選擇限制的玩家 |
| rule_override | 依下表 | 無 | 只收三個已登錄變體 |

| rule_override.rule | 其餘必填 | 選填 |
| --- | --- | --- |
| spellchain_population | who:P、of:CollectionSource | 無 |
| counter_cap | on:O、name:CounterName、max:Q | 無 |
| battle_damage_basis | on:O、basis:hp | 無 |

## 3. cannot 與 predicates

每個變體必填 `what`，其餘欄位如下。沒有列出的組合不能借用其他 what 的欄位。

| what | 選填欄位 | 約束 |
| --- | --- | --- |
| play_card | on:O、who:P、card:CardPredicate | on／who 互斥；均省略時依 §4 |
| play_ability | on:O、who:P、ability:AbilityPredicate | on／who 互斥；均省略時依 §4 |
| attack | on:O、target:RecipientSel | target 不收裸 Ref |
| choose_target | on:O、by:P | by 是玩家域；ability 字串僅可能作 binding，沒有特殊來源語義 |
| draw | who:P、during:DrawDuring | 不收 on／card／during.who |
| destroy、banish | on:O、by:ability | by 若提供只能是 ability |
| stand | on:O、during:StandDuring | 不收頂層 who |
| win、lose、pp_max_increase | who:P | 不收物件範圍 |
| deal_damage | on:O | 降為來源側傷害阻止 |

CardPredicate 是非空的 CountFilterFields 加 `zone:Zone 或非空 Zone[]`；
完整共用屬性見[物件查詢屬性表](author-object-query-shapes-1.0.md)。
沿用 name／names、token／not_token 互斥，不收 who／in／where／source，
也不繼承 Sel 的直接參數權限；zone 暫只有合成例。

AbilityPredicate 恰為 `{keyword:TriggerKind}` 或
`{kind:activated, except?:非空 [evolution]}`；except 必須是列表，不能寫純量。
它篩選能力種類，不是授予能力頭。

DrawDuring 恰為 `{not_phase:Phase}` 或 `{phase:Phase,cause:rule}`，
後者 cause 必填；不可同時含 phase 與 not_phase。
StandDuring 必填 `{phase:start,who:PlayerRef,cause:rule}`，沒有選填欄位。
這三型與事件的 `{turn:PlayerRef}` 不共用形狀。
no_trigger 抑制誘發；cannot.play_ability 阻止打出，不能互換。

## 4. 安裝位置與範圍

這張表是完整父節點接線時 C1／D0 必須實施的契約；目前 C0 只驗證效果片段，
還沒有 static／apply／player_effect／entry_effects 父節點驗證器。
表中「明寫」不是由片段驗證器依上下文推定；錯父位置的拒絕尚須完整入口。

| scope 角色 | static | apply | player_effect | entry_effects |
| --- | --- | --- | --- | --- |
| optional on:O | 明寫 | 省略＝affected | 明寫，不把玩家轉物件 | 省略＝entering |
| optional on:R | 明寫 | 省略＝affected | 省略＝父 who 的領袖集合 | 省略＝entering |
| optional who:P | 明寫 | 缺少則拒絕，不把物件轉玩家 | 省略＝父 who | 缺少則拒絕 |
| cannot.play_card／play_ability 的 optional on／who | 恰一明寫 | 均省略＝affected | 均省略＝父 who | 均省略＝entering |
| 必填 on／who／by | 全部明寫 | 全部明寫 | 全部明寫 | 全部明寫 |

**damage_mod 的跨上下文例外**：只要有 source 而沒有 on，在上面四種位置一律表示
所有符合的接受者，不套各欄預設。沒有 source 才回到表內規則。
例如 apply 中 `{damage_mod:{source:affected,delta:1}}` 增加 affected 對任何接受者的傷害，
不能誤變成 affected 對自己造成的傷害。prevent／damage_cap 沒有此例外。
cost_mod.when_playing 是獨立的打出篩選，不從這張表補範圍。

必填範圍在 apply 明寫 `on:affected`、入場明寫 `on:entering`；
`on:this` 始終指能力持有者，不偷偷改成接受者。玩家、物件與領袖域不可隱式互轉。
能力控制者、binding 可見性、capture、期限／UsageId、世代與 LKI、入場掃描前安裝及 capability
仍需 C1／D0；C0 形狀接受不證明實際上下文合法。

已有專用本文動作的加攻體／設攻體／失去能力／改種類，作者優先用對應動作；
自身打出許可優先用 play 頭。這是作者選用偏好，C0 不強制；
不推定與 apply.effect 的等義拼法同 hash，也不另造 C1 同義禁令。

## 5. variant 證據

來源為遞迴檢查 `card-data/jp` 凍結卡檔的 documents／faces／text／sections；
不把 Q&A 或本文動作命中冒充效果位置證據。以下是片段，不含完整父頭、條件或期限；
「合成」只驗形狀。測試用正例在 `carddb/tests/contracts/author_effect_cases.py`
記錄來源位置，並由雙端驗證測試 round-trip。卡號代表 frozen 記錄，不主張所有歷史版本相同。

| variant | 來源能力位置／代表卡 | 本片片段與缺口 |
| --- | --- | --- |
| stat、set_stat | 本文 buff／設值已有動作；BP01-011 是 fanfare buff | EffectSpec 正例合成；沒有直接 static 攻體真例 |
| keywords | static，BP06-054 | list:[ward]；條件／完整 static 頭未接 |
| lose_abilities、make_type | 本文已有專用動作 | EffectSpec 正例合成 |
| also_named | token section static，BP08-004 | name:操り人形 |
| add_trait | apply，BP02-T07 | trait:武装 |
| remove_trait | 未找到直接效果位置 | 合成 |
| cost_mod.delta／set | apply BP18-001；player_effect BP14-046 | delta:-2；set:0,next_only:true，集合型範圍 |
| damage_mod.delta | static BP01-134／BP01-137 | 收到 -1／輸出 +4；後者省略父回合條件 |
| damage_mod.multiply | static BP02-067 | 限領袖的 attack ×2 與 combat ×2 分兩片，避免攻擊從者乘兩次 |
| prevent.any／combat／ability | apply BP02-090／BP02-019；static BP21-001 | 三個 kind；BP21-001 用學院異名或獸聯集 |
| prevent.next_only | player_effect BP05-017 | 下一次所有種類傷害；消耗時點仍需裁定 |
| damage_cap | player_effect BP05-101 | threshold:5,set:4 |
| evolve_cost_mod.delta／set | static SP01-012；apply BP05-048 | 動態減費／set:0，後者明寫 affected |
| banish_instead | static BP18-061 | 對手場上從者的 field→cemetery |
| cannot.play_card | player_effect BP05-006 | card.kind:follower；card.zone 另為合成 |
| cannot.play_ability | apply CP02-005／BP03-039；entry_effects BP04-038 | activated／except:[evolution]／keyword:fanfare |
| cannot.attack | apply BP05-061 | 對手從者或領袖；BP06-113 的可攻擊判斷尚有缺口 |
| cannot.choose_target | Aura 提醒文，不是獨立能力 | 合成，不能算直接真例 |
| cannot.draw | player_effect BP05-006；static BP10-076 | 規則 start 抽牌／非 start；兩種 during |
| cannot.destroy／banish | static BP02-090；banish 的能力限定有疑義 | destroy 真例；banish:by:ability 合成 |
| cannot.stand | static ECP01-053 | 僅自己的規則 start 起身 |
| cannot.win／lose | static BP05-092 | win:opp／lose:self |
| cannot.pp_max_increase | player_effect BP05-006 | 省略 who 交父節點 |
| cannot.deal_damage | apply BP12-109 | 父 capture:play 與離場後 LKI 仍需接線 |
| must_attack | CP04-012 的授予內 static | 獨立 EffectSpec 合成，grant 尚未實作 |
| play_permission | BP18-007 優先用自身 play 頭 | EffectSpec 合成；玩家消滅區來源另待設計 |
| no_trigger.on／who | entry_effects BP19-074；static ECP01-010 | entering／對手能力控制者 |
| unlimited_evolve | static BP18-001 | who:self |
| enters_acted | static BP11-112 | 對未來入場的 this，完整入場接線待做 |
| reroll | static ECP02-065 | times:1 |
| ignore_ward | static BP04-006 | on:this |
| choice_any | static BP20-P65 | who:self |
| extra_trigger | static BP19-092 | phase:end,n:1 |
| must_target | static BP03-091 | on:this,by:opp |
| rule.spellchain_population | static BP02-035 | 墓場 spell 與巫師 follower 聯集 |
| rule.counter_cap | section static CP04-115 | バッテリー,max:7 |
| rule.battle_damage_basis | static BP01-129 | 己方從者 basis:hp |

直接事件與 history.where 的逐欄矩陣另見[事件矩陣](author-event-matrix-1.0.md)。
事件角色與本文件的安裝角色不可互借；兩者的正反例都不取代全卡與執行驗收。
