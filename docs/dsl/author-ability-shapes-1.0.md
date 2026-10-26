# 能力頭、授予與延遲形狀矩陣

JSON Schema `dsl/author-nodes.schema.json` 是語法權威；Python 對照為 `contracts/author_programs.py`。本文只定義封閉節點；完整DSL 入口、展開與執行准入另驗。

## 單一遞迴圖與封閉邊界

A.ability-head 恰含一個頭，不含用途 line/section/sources/ports/imports/generated_by。inline 的能力／宣告內容使用它；companion.body 使用排除獨立 keywords 的 A.companion-head，不能經 generated_by 引入非本體宣告。完整容器由獨立契約承載。GrantedAbilityHead 為同一頭 payload 的子集合（見下表），不是另一套能力語法。遞迴路徑為 ability→Steps→grant/delayed→ability，以及 static→EffectSpec.grant→ability。

Python 的 author_programs 承載能力、效果、Steps 的強連通圖，不使用動態 namespace 注入。Action／Step／GrantedAbilityHead 使用透明 RootModel 名目入口，避免型別檢查器逐路徑展開遞迴別名；內部仍是相同封閉 Fields，序列化不新增 root 鍵。Python 消費者以 `.root` 取得這三種入口的 Fields；Steps 是 Step 的 tuple，授予清單是 GrantedAbilityHead 的 tuple，D0 遍歷時須在這些邊界解包。Cost／Event／Target／Until 保留原模組與公開型別。Schema 現有 ID 保留，根仍拒絕裸片段。名稱使用 Concept／CardName 具名葉（Python alias 與 schema 同步），目前均沿 Text 的非空值域；registry 不是 C0 字串形狀。

## 完整頭矩陣

T=既有非空、保序 Targets；O=OptionalCosts；X=XParameter；D=非空 Map<BindingName,NumericExpr>。do 是既有 Steps，可空；pending 不得用空 do 冒充完成。型別共用不推定執行准入。

| 頭 | 必填 | 選填 | 真例／合成欄 |
| --- | --- | --- | --- |
| keywords | 非空 Concept/ParamRef 列表 | 無 | BP01-159 守護、BP01-007 Quick；ParamRef 與 `[quick, ward]` 並列合成，沿既有白名單 |
| fanfare | do | T,cost,O,X,D,per_turn,ub | BP19-038 cost、BP01-004 D、BP01-011 本文；per_turn 合成 |
| evolve_time | do | 同 fanfare | BP02-047；T/cost/UB 沿既有 §3、§13 真例；per_turn 合成 |
| super_evolve_time | do | 同 fanfare | BP18-081；未有直接例的 O/X/D/per_turn/ub 標合成 |
| attack_time | do | 同 fanfare | BP02-019 per_turn=1；BP08-020 per_turn=2；其餘無直接例標合成 |
| race_time | do | 同 fanfare | CP01-007；未有直接例的 T以外共通選填標合成 |
| drive_time | do | 同 fanfare | CP03-008；未有直接例的共通選填標合成 |
| lastword | do | T,O,X,D,per_turn,ub | BP01-159；不收 cost，冒號用 pay；未有直接例的共通選填標合成 |
| auto | on:EventPattern,do | T,O,X,D,per_turn,ub,zone,aggregation:batch | BP20-P61 on、BP04-064 cemetery、CP04-PR20 巢狀 per_turn=2；O/X/D/ub 合成 |
| activate 一般 | cost,do | T,O,X,D,per_turn,ub,zone,usable_if,quick:true,advance:true | BP01-013 per_turn、BP02-027 quick、BP14-018 advance/usable_if、BP10-101 hand；O/X/D 無直接例標合成 |
| activate 融合 | fusion,do | T,O,X,D,per_turn,ub,zone,usable_if | BP19-038、BP20-020；不收 cost/quick/advance；其他無直接例欄標合成 |
| evolve | cost | into:EvolutionDestination,usable_if | BP21-096 usable_if、BP09-004 名稱列表、BP03-056 selector |
| ride | cost | 無 | CP03-008 |
| feed | cost | times:正安全整數 | CP01-042 三行省略/2/3；明寫1合成且不與省略同 hash |
| spell | do | T,O,X,D | BP01-007 T、BP15-046 O/X；不收 cost/zone/per_turn/ub |
| static | effects:非空 EffectSpec 列表 | while:Condition,zone | BP12-029、CP04-PR21；不收 do/T/cost/D |
| play | 至少一個打出修飾鍵 | PlayCosts 的欄＋cannot_if、playable_from、D | 三張條件減費、BP13-093、BP18-007、BP06-016 D、CP01-057 cannot_if；define 單獨拒絕 |

所有欄位封閉。7 個觸發巨集頭不接受 zone；事件定義與 LKI 已決定自身的觸發位置。auto／activate／static 的 zone 用 AbilityZone=既有六值＋equipment，允許單值或非空列表，保留重數，不接受 param；不擴張 Sel／參數的 Zone。auto/activate.zone 的合法值及對應卡文由 C1 驗證，C0 不把只標合成的組合當全卡准入。

獨立 keywords 是非本體宣告，完整 A.ability.keywords 用 CardKeywordSourceUse `{binding_id,occurrence}`，不含 ability_scope；ports 兩個 map 必空，imports／generated_by 不合法，清單元素不可重複。裸 A.ability-head 仍供片段型別與葉位驗證，不能把其結構可讀當 companion 准入；完整 inline 的 ParamRef 仍拒絕。含此宣告的卡目前只接受 draft、空 reviewed_by／verified_by_exam；正式 current 來源與作者宣告對帳、D0 的獨立義務精確消耗、Ward／Aura 具名權限宣告的 lowering 與精確整卡准入已實作。准入不代表對局規則已執行：守護的攻擊限制與可選橫置、光紋的目標限制及生命週期接線、用途資格仍未完成。

per_turn 是 ≥1 的安全整數；拒絕舊鍵與 bool/0/負數/字串。自動能力按 PendingAdmission 消耗、起動按 PlayCommitted 消耗，不提供 consume_at 自訂入口。起動 BP08-084 n=2、auto BP07-036 n=4、授予 CP04-PR20 n=2。per_turn 不表示「只在自己回合」；額外的回合限定需要獨立的規則准入。ub:true 為獨立標誌，不是 per_turn。

EvolutionDestination 直接抽出並共用現有動作 evolve.into 型別：CardName／非空 CardName[]／ObjectSel，不新增 {sel} 包裝或新選取程序。C1 驗 selector 指向合法進化候選、名單登錄與選取時點；evolve_follower 字串形狀不等於 registry 准入。fusion 使用三個互斥變體 {sel:ObjectSel,n:Quantity,result?}／{sel,upto:Quantity,result?}／{sel,any:true,result?}；n 真例 BP19-038、upto BP20-020、any 合成。activate.zone:hand 限制能力啟動位置；融合材料選取不寫 zone，以保留規則允許的手牌／EX。材料一次回答，BP20-020 用 upto:2,result:fused,x:{from_payment:fused}；融合 receipt 是否可投影實付數列 C1，不另宣告一次 X。下限與零選擇准入依 CR12.18，不由 C0 推定。

## 打出程序修飾

cost_delta/optional_cost_delta 為 NumericExpr 或 {if:Condition,delta:NumericExpr}；cost_set/optional_cost_set 為 Quantity 或 {if:Condition,set:Quantity}。各欄另接受至少兩項上述單項的列表，保留印刷序。BP13-093 同一 source ability scope 三條條件，放同一 play.cost_delta 列表，不拆成三次用途。條件於費用計算點求值，false 不安裝修飾，不能改成 set:0。多 set 的適用關係依費用規則由 C1/D0 檢查，列表不自行建立全域優先權。

條件式 cost_delta 真例 BP02-038、BP03-097、BP04-023、BP13-093。其餘條件式／列表交叉格皆合成。已有 Scalar delta/set 行為保留。直接 param 仍不接受，只有 NumericExpr 內原先准許的葉位置可接受；play.cost_delta 直接參數不在既有白名單。

playable_from=Zone／非空 Zone[]／{zone:Zone 或非空 Zone[],if:Condition}。BP18-007 用條件式 cemetery；條件式多區列表與無條件形標合成。條件只守護此許可，不禁止原本合法手牌打出。cannot_if 使用 Condition，禁特定區域的標準形是 in_zone(this)／其 not／布林組合；原所在區在打出程序承諾移區前查。BP12-095 與 BP06-059 為例。

play 的 optional_cost 沿既有 OptionalCosts 形狀（保留 C0/C1 已定邊界），C1 拒絕 option／多組，且同卡同次 play 至多一個有 optional_cost 的 play 頭。這個責任不因局部片段能 parse 而解除；optional modifier 原本要求唯一 optional_cost 的 C0 檢查保留。

## grant 的子集合與期限

GrantedAbilityHead 共用普通頭 payload，接受 7 個觸發巨集頭、auto、activate、static、play；排除 keywords、spell、evolve、ride、feed。grant.keywords 已承載關鍵字；其餘未證實能授予的規則入口不新增接受形狀。每項恰一個頭，沒有 line/sources/ports/generated_by。類別與接受者准入仍由 C1 驗。

- A.act.grant：on:EffectRecipient 必填（擴成可含領袖），keywords?/abilities? 至少一項且各非空，可並用；沿原 result/precondition/until/instead。keywords 元素 Concept|ParamRef；abilities 為 GrantedAbilityHead 列表。
- EffectSpec.grant：on:EffectRecipient 選填，keywords?/abilities? 至少一項；keywords 只有 Concept，不繼承動作直接 param。static 明寫 on 是 C1 的要求；C0 仍接受省略 on 的效果形狀。apply/player_effect/entry_effects 按既有 optional R scope 表，不推測物件↔玩家轉換。
- grant.instead 差異可改 keywords、abilities（可同時）；replace_with 與任何差異互斥；不改 on/until/result，合併後重驗。abilities 差異是合成，BP15-029 只佐證 keywords。
- 一個 grant 的 until 適用全部內容。BP20-036 的 keywords 無期限、abilities until:turn，寫兩個有序 grant 步驟。官方英文卡文將「本回合」只掛在後面的防傷能力；Q629 亦確認未限期的指定攻擊不在回合末失去，CR 10.2.1.2／7.4.8 分別規定無明示期限及回合末到期。同型卡名還有 BP05-112、BP16-103、BP18-017（突進）、BP18-026（オーラ），各用途仍核對其官方卡文；不以一個 grant 把期限擴到前面的關鍵字。此為文字範圍判讀，官方 Q&A 未直接回答 BP20-036 的掛接範圍，Q2647 只確認防傷能力不擋非傷害的攻擊力與體力減少；C1 仍須驗證用途來源與資格。
- 純關鍵字優先現有 keywords 形，混合例 BP12-029/BP13-033/CP03-023/ECP01-043；多能力 ECP02-061；授予 play BP17-079、lastword BP07-038；grant 頭子集合的其他無實例格標合成。

Quick：卡片本身持有時寫 `keywords: [quick]`，起動能力仍寫 `activate.quick: true`。授予子集合排除 keywords 頭，關鍵字由 grant.keywords 承載；既有 grant.keywords 與 EffectSpec.keywords 繼續接受 quick 字串，不新增同義拼法禁令。BP12-058 的DSL 標準形是 static.while＋EffectSpec.keywords(list:[quick])；不做語義正規化，不推定不同形狀有相同 body hash。

巢狀 this 指接受者、self 指能力控制者，文字提供者來源保留。host 已登錄：CP04-PR21 是外層 static(zone:equipment)→grant(on:host)→static；內層 this 是裝備者。host 在裝備不存在／不在有效位置時不可假造物件，接附改變須重算。CP04-PR18 是 UB 起動、PR20 是 per_turn=2、PR25 是 batch、CP04-058 sections/0 是混合授予。

CP04-045 授予 auto 明寫 zone:ex。巢狀能力預設依接受者種類，不能按提供者／接受當下位置任意推斷。領袖接受者的 static 不寫 zone；有效期間跟隨領袖接受者（非 field 物件），這是 C1 的驗證責任，不把型別例當可執行。

## delayed 與 Until

A.act.delayed={on:EventPattern,do:Steps,targets?:T,optional_cost?:O,x?:X,define?:D,aggregation?:batch,next_only?:bool,until?:Until,precondition?:Condition}。沒有 line/result/cost/zone/per_turn/ub/when。來源由外層用途＋結構路徑 sidecar 保存；不把印刷行號寫進 body。result 不接受：沒有本契約定義的延遲 receipt 投影，不能假借 ReceiptId；precondition 依一般動作規則，在安裝延遲前查（合成）。delayed 是 Action，replace_with 可使用它（合成），不另造排除延遲的平行 Action union。

on/do/T/O/X/D/aggregation 都用同一原型別，T/O/X/D 為合成欄。無 until 預設僅一次；無 until＋next_only:false 拒絕。有 until 時省略 next_only 或 false 表期限內各次，true 為期限內一次。前者 lowering Once，後者 Until；期限內一次用 Until＋TriggerSpec.limit(maximum=1,window=Game,scope=AbilityInstance,consume_at=PendingAdmission)，不新增 IR lifetime 變體。delayed runtime 的有效身分／清除時點須由 D0 驗證，C0 不執行。

新增 Until:{during_next_turn:{who:PlayerRef}}，對應現有 Interval::DuringNextTurn；不接受 edge/phase/param。CP04-012、SP01-026。完整次回合使用顯式長形；既有短值 next_opp_turn 仍按原形保留，不由 C0 推定與長形同義。

BP01-056 無期限一次；BP03-089 turn＋next_only:true；SP01-026 批次；BP17-P34／BP17-074 sections/0 有期限多次；BP15-001/008/012 捕捉母 binding。批次「その中の1体」的選取時點與選擇者由 C1 驗證，C0 只驗結構。

## C1／D0 與來源責任

- targets、Cost.steps、Steps、effects、abilities、修飾列表保序保重數；map 鍵序無語義。define 需依賴檢查、無循環、無非法前向引用；不新增預設或語義正規化。
- grant 不捕捉母 local binding；affected/entering/found/item/it/attack_target/cost1 不得原樣跨入。新頭若自身建立同名保留脈絡則是新 execution 的合法值。delayed 的 this/self 固定母來源／控制者；母 binding 經 CaptureEnv 以既有 Ref 世代／LKI／capture 規則捕捉；其 on 與本文 it 來自新事件，不混成母事件。未有投影策略拒絕。
- ports 不能讓巢狀 grant／delayed 本文 local binding 逃出；完整容器的 source sidecar 要追蹤所有遞迴節點。
- 巢狀型別位置沿同一 param 白名單（例如 draw.n），不准替換整頭／Steps／期限／binding／limit。EffectSpec.grant.keywords 明確不准直接 param。
- BP17-030 的 play 條件讀 spell 目標：C1 以同卡同次 play、ports／既定依賴精確定位；定位不到拒絕。不靠字串同名自動跨能力可見。
- 合併 Fanfare/Lastword 的卡，在 Projection 已提供兩個能力 scope 時，完整容器依契約產同 line 的兩個獨立頭。不得複製同一 source ability scope 兩次；來源尚只提供一個 scope 時必須先處理來源分解，由 C1 拒絕未分解的來源，不新增 combined 自由容器。BP04-055 兩頭冒號費用的不同時點須各有來源規則證據。
- C0 通過不代表來源、規則與 capability 已通過 C1 或可由引擎執行。

## 凍結卡文證據

掃描遞迴涵蓋 JP 302 檔、7383 faces、8192 個非空 text／sections，EN 297 檔、7437 faces、7834 列。未以 Q&A 命中充當卡例，日英也未由卡號推定配對。下表定位 JP 主文；同卡 sections 另列。`author_ability_cases.py` 的片段不是整卡完成聲明：`partial`、`fragment`、`mode-fragment` 省略了其他指示／外層 choice，局部 binding 由外層提供；SP01-026 的選取解讀仍需 C1 規則准入。

所有檔案相對 `card-data/jp/`；document 是文件鍵，face 是 0 起算索引。

| 卡號 | 檔案 | document／face／欄位 |
| --- | --- | --- |
| BP01-007 | `BP01/000-099.json` | `documents-b0a48baa2af03c69`／0／`text` |
| BP01-011 | `BP01/000-099.json` | `documents-43a4237a00c53844`／0／`text` |
| BP01-056 | `BP01/000-099.json` | `documents-4be5e842d12919ad`／0／`text` |
| BP01-159 | `BP01/100-199.json` | `documents-39e053390673272f`／0／`text` |
| BP02-019 | `BP02/000-099.json` | `documents-df65287385f21872`／0／`text` |
| BP02-038 | `BP02/000-099.json` | `documents-ba742b92ce4bbca2`／0／`text` |
| BP02-047 | `BP02/000-099.json` | `documents-dbb131c569a8f145`／0／`text` |
| BP03-056 | `BP03/000-099.json` | `documents-be538b8be996c33b`／0／`text` |
| BP03-089 | `BP03/000-099.json` | `documents-5ff685186f4defe1`／0／`text` |
| BP03-097 | `BP03/000-099.json` | `documents-58eb63bcc23ed885`／0／`text` |
| BP04-023 | `BP04/000-099.json` | `documents-f138f839d27b9e45`／0／`text` |
| BP04-103 | `BP04/100-199.json` | `documents-1c57fb690fa8abc3`／0／`text` |
| BP07-036 | `BP07/000-099.json` | `documents-cd3f0e9e124f0418`／0／`text` |
| BP08-020 | `BP08/000-099.json` | `documents-55e685aa1800babe`／0／`text` |
| BP08-084 | `BP08/000-099.json` | `documents-32f7a97c882f4215`／0／`text` |
| BP09-004 | `BP09/000-099.json` | `documents-9cf0abef21cd5637`／0／`text` |
| BP12-029 | `BP12/000-099.json` | `documents-609f83d194b866b6`／0／`text` |
| BP13-033 | `BP13/000-099.json` | `documents-cca608e655d32e68`／0／`text` |
| BP13-093 | `BP13/000-099.json` | `documents-9d8bd14701558e92`／0／`text` |
| BP14-018 | `BP14/000-099.json` | `documents-f721a783eddae7d8`／0／`text` |
| BP15-046 | `BP15/000-099.json` | `documents-bc7e44ce9ed0ad5a`／0／`text` |
| BP18-007 | `BP18/000-099.json` | `documents-a3da95ecbd26f5d4`／0／`text` |
| BP18-081 | `BP18/000-099.json` | `documents-2a66f24129bc60d1`／0／`text` |
| BP20-020 | `BP20/000-099.json` | `documents-10e87a01a4e6247d`／0／`text` |
| BP20-P61 | `BP20/P/000-099.json` | `documents-2587c06ef9231eeb`／0／`text` |
| CP01-007 | `CP01/000-099.json` | `documents-9ad045277183eefb`／0／`text` |
| CP01-042 | `CP01/000-099.json` | `documents-563d8d9773244fe4`／0／`text` |
| CP03-008 | `CP03/000-099.json` | `documents-22d0607808786c41`／0／`text` |
| CP03-023 | `CP03/000-099.json` | `documents-b169d99f8e8090c3`／0／`text` |
| CP04-012 | `CP04/000-099.json` | `documents-d77411794b41b299`／0／`text` |
| CP04-012 | `CP04/000-099.json` | `documents-d77411794b41b299`／0／`sections/0` |
| CP04-045 | `CP04/000-099.json` | `documents-974f809c412b42da`／0／`text` |
| CP04-058 | `CP04/000-099.json` | `documents-b64be9e1c6c83b36`／0／`text` |
| CP04-058 | `CP04/000-099.json` | `documents-b64be9e1c6c83b36`／0／`sections/0` |
| CP04-PR18 | `CP04/PR/000-099.json` | `documents-706bb5aa7c7a655d`／0／`text` |
| CP04-PR18 | `CP04/PR/000-099.json` | `documents-706bb5aa7c7a655d`／0／`sections/0` |
| CP04-PR20 | `CP04/PR/000-099.json` | `documents-132afe00cb23095d`／0／`text` |
| CP04-PR20 | `CP04/PR/000-099.json` | `documents-132afe00cb23095d`／0／`sections/0` |
| CP04-PR21 | `CP04/PR/000-099.json` | `documents-22b0e492735a5df0`／0／`text` |
| CP04-PR21 | `CP04/PR/000-099.json` | `documents-22b0e492735a5df0`／0／`sections/0` |
| CP04-PR25 | `CP04/PR/000-099.json` | `documents-5f9a7e565228b88c`／0／`text` |
| CP04-PR25 | `CP04/PR/000-099.json` | `documents-5f9a7e565228b88c`／0／`sections/0` |
| ECP01-043 | `ECP01/000-099.json` | `documents-b63913ad637af297`／0／`text` |
| SP01-026 | `SP01/000-099.json` | `documents-5595db4b0b6f7fd4`／0／`text` |
| BP20-030 | `BP20/000-099.json` | `documents-5a6a4cd92bd4bf2b`／0／`text` |

| BP01-004 | `BP01/000-099.json` | `documents-caeac0d04b7db4fd`／0／`text` |
| BP01-013 | `BP01/000-099.json` | `documents-bdc77f7000142130`／0／`text` |
| BP02-027 | `BP02/000-099.json` | `documents-e8bef470bbfdb3b7`／0／`text` |
| BP04-055 | `BP04/000-099.json` | `documents-99ad9a2708ea7670`／0／`text` |
| BP04-064 | `BP04/000-099.json` | `documents-7e1de7941f45476a`／0／`text` |
| BP06-016 | `BP06/000-099.json` | `documents-42c49943a1faa1b1`／0／`text` |
| BP06-059 | `BP06/000-099.json` | `documents-dcf40005ec8aa6fd`／0／`text` |
| BP07-038 | `BP07/000-099.json` | `documents-5fda914354697b12`／0／`text` |
| BP10-101 | `BP10/100-199.json` | `documents-cca40563f5dcb8f8`／0／`text` |
| BP12-058 | `BP12/000-099.json` | `documents-d0713795246b6752`／0／`text` |
| BP12-058 | `BP12/000-099.json` | `documents-d0713795246b6752`／0／`sections/0` |
| BP12-095 | `BP12/000-099.json` | `documents-7be5e9488a9b8f9d`／0／`text` |
| BP15-001 | `BP15/000-099.json` | `documents-bdbbecf577ddf59a`／0／`text` |
| BP15-008 | `BP15/000-099.json` | `documents-5d4734ed75e73760`／0／`text` |
| BP15-012 | `BP15/000-099.json` | `documents-e17697bf2a7c6e40`／0／`text` |
| BP15-029 | `BP15/000-099.json` | `documents-d68c163c0d4b029d`／0／`text` |
| BP15-029 | `BP15/000-099.json` | `documents-d68c163c0d4b029d`／0／`sections/0` |
| BP17-030 | `BP17/000-099.json` | `documents-c8c39dbea91ec719`／0／`text` |
| BP17-074 | `BP17/000-099.json` | `documents-aa1f7aa4155968b1`／0／`text` |
| BP17-074 | `BP17/000-099.json` | `documents-aa1f7aa4155968b1`／0／`sections/0` |
| BP17-079 | `BP17/000-099.json` | `documents-d77a8bb167ab71d3`／0／`text` |
| BP17-P34 | `BP17/P/000-099.json` | `documents-8323a76835052f81`／0／`text` |
| BP19-038 | `BP19/000-099.json` | `documents-23f463739244e3cb`／0／`text` |
| BP20-036 | `BP20/000-099.json` | `documents-f71e6a38ef94fd0e`／0／`text` |
| BP21-096 | `BP21/000-099.json` | `documents-c143993864450ab3`／0／`text` |
| CP01-057 | `CP01/000-099.json` | `documents-c0d1f5c977158565`／0／`text` |
| ECP02-061 | `ECP02/000-099.json` | `documents-1131a75e6df95eff`／0／`text` |

## 驗證邊界

逐頭與融合數量變體、限次安全整數、錯上下文／型別／互斥、深層遞迴、列表保序與重數，由 JSON Schema 及 Python decoder 雙邊檢查並比較 roundtrip。所有具名定義有結構漂移對照；突變覆蓋 required、ref、enum、union、minItems、未知鍵與單鍵限制。Action／Step 的共用 payload 與 GrantedAbilityHead 的子集合引用另有結構保護。合成組合只證明 C0 形狀，不等於正式卡片或執行能力。

矩陣中的卡號是凍結卡文的設計證據，不全是已轉寫的驗證片段；實際驗證片段以 `author_ability_cases.py` 的清單與 partial／fragment 標記為準。未轉寫的交叉格仍由 C1 全卡工作補齊。
