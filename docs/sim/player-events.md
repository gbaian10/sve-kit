# 逐觀看者的公開事件序列

`Game::events(viewer, after)` 提供同一對局、同一觀看者已提交的規則歷史。
`None` 從頭讀取，後續傳入最後一筆的 `EventId`；重讀同一 cursor 得到同一前綴，
讀取不執行規則、不配置物件代號、不消耗亂數。另一觀看者、另一對局及未到達的
cursor 回 `InvalidCursor`。這是 Rust 行程內介面，沒有穩定的 wire 或存檔格式。

`Game::event_batches(viewer, after)` 讀取完整的同時事件批次，事件與逐筆入口一一對應。
後續 cursor 必須是某批的最後一筆；傳入批次中間的事件回 `InvalidCursor`。
批次只含該觀看者獲授權的成員，不交付空批次或隱藏佔位。

`observe` 仍是當前局面的權威。事件保留移動方法、發生順序與已知因果，不能當成
完整的權威重播。`cause` 與 `group` 都只在該觀看者的流內編號；`parent` 僅連向
已經公開且來源明確指定的 Step 父事件，沒有可授權父事件時為 `None`。
不交付執行、指令、receipt、實體物件 ID 或原始 JSON。

## 啟用與提交

`start_player_game` 交付對局時啟用記錄；可信宿主建立的中途局面則在首次
`advance_player_input` 啟用。啟用前的局面由首次 `observe` 作為基線，不補造歷史；
既存 pending 的後續解決仍記錄，沒有已公開來源的 parent 為 `None`。

有懸置的 typed execution 時，事件只留在交易內，直到執行完成才發布。
這個保守邊界同時涵蓋 origin、nested play、付款與沒有 origin 的分支。
中途的 Resolution 或已公開資訊仍可從 `observe` 讀到。付款不足而 `Reverted`，
或故障回到 origin 時，未提交事件一起丟棄，不消耗公開編號。
終局與投降提交已執行的前綴；拒絕的回答不新增事件。

回滾帶回已提交的事件前綴與來源高水位，不能把原本已發布的來源再發一次。
已提交區塊與 cause／group／parent 索引由 `Arc` 共享，`Game::clone` 不深拷貝歷史。
新增資料時才對共用索引作 copy-on-write。
目前不修剪行程內歷史；存檔恢復後重新建立觀察基線，不延用舊 cursor。

投影缺口只讓對應觀看者的事件流固定回 `ProjectionGap`，之後停供事件，
不改變 `input`、`submit`、`observe` 的權威結果，也不自行判定勝負。

[對戰契約](battle-contract.md)另外定義供音畫使用的有限暫態呈現。
它不由 `Game::events` 輸出、不占已提交 cursor，也不放寬上述規則歷史的提交邊界；
授權、晉升、故障撤銷與播放一次性依該契約處理，不能直接把懸置 buffer 當公開歷史。

## 來源聯集與重疊

Step 在實際 `emit` 時記錄；typed facts 在中央 `record_fact` 寫入時記錄。
`Execution.facts` 使用唯讀集合介面，不能直接 push／extend；nested 搬運已記錄
facts 不再產生事件。反覆讀取累積的 `Progress` 不會重發歷史。

同一物理移動只發一筆 `Movement`，方法可區分抽牌、棄牌、typed Put／Destroy／
Banish／Return、規則破壞與一般程序。規則破壞由該次規則清理 producer 明確補為
`RuleDestroy`；堆疊歸零等其他移區不因此變成破壞。抽牌和棄牌的宣告 trace 由各自
primitive 明確補到同一次移動；typed Movement 的方法由該次 mutator 的來源範圍補入。
傷害、姿態、擲骰、裝備與正式卡片打出／解決，由共用 primitive 記錄同一結果；
typed facts 保留在權威 Progress。傷害分類透過實際 emit 回傳的 Step ID 補入。
效果進化也以該 primitive 回傳的明確 Step ID 補為 `EffectEvolve`，保留其規則角色。
沒有以顯示文字相等或最近一行的猜測來去重。

完整同時事件批次由 producer 在全部結果凍結後，以 `player_event_mark`／
`seal_player_event_batch` 封口，不能由共同 cause 或 group 推論。未封口的結果各自形成
單件批次；多物件 producer 必須明確封口，後續規則動作若屬另一時點，須另批封口。

| 來源 | 公開事件 |
| --- | --- |
| 移動、場に出す；typed Movement／Draw／Discard | 一筆帶方法及前後端點的 Movement |
| ダメージ；typed Damage | 一筆實際傷害，保留 Ability／Attack／Combat 分類 |
| アクト、スタンド；typed Posture | 一筆姿態結果 |
| サイコロ；typed Dice | 一筆最終接受的值，中間重擲不另發 |
| プレイ、解決；typed CardPlayed／CardResolved | 一筆對應的卡片打出／解決 |
| 装備；typed Equipped | 一筆裝備關聯結果，與建立裝備的物理移動分開 |
| 進化、超進化；typed Evolved | 進化結果；效果進化保留獨立角色 |
| typed LifeChange、StatChange | 直接生命變化與明示的非零數值 facets |
| typed Fused、Raced、DriveGained、DriveTrigger、DriveChecked | 必要的完成結果，排除執行與關聯 ID |
| typed Payment、Shuffled、AbilityPlayed、AbilityResolved | 費用、洗牌與能力打出／解決結果 |
| 回復、PP最大値変化、EP変化、体力増加、カウンター | 公開資源／計數器結果 |
| 公開、表向き／裏向き、攻撃 | 依發生時授權的揭露、翻面與攻擊宣告 |
| 破壊、消滅、取代、消去 | 嘗試、替代及消去；不把未成功的破壞當成移動 |
| 勝利、敗北 | 普通敗北、同時敗北、效果終局與投降 |
| ターンスキップ、支配変更 | 已有共用程序的公開結果 |
| start_main、start_end、next_turn | 真正進入 Main／End／Start 的新來源，不改舊 Step |
| 開局護符裏放、finish_opening | 匿名放置，雙方換牌後才揭露 |
| 待機、待機取消、費用成立、抑制 | 凍結原型的 continuation trace；進入新流時回投影缺口 |
| 未知 Step kind | 投影缺口，不能靜默遺失 |

## 發生時的授權

移動前取得 before；after 在 A02 知識更新、隱藏排序處理及 typed 私有手牌
定位撤銷後取得，並在消去、規則返回等後續程序之前封存。控制權移動、起始護符與
消去另有對應接點。物件身分只使用 A02 的 `ObjectHandle`，不另配置實體代號。

事件卡片帶當時可知的 `CardProperties`。記憶型牌面沒有當前 handle；過去世代的
typed 成員只能從同一未提交交易內、已授權的確切來源快照取記憶，不能因現在重新
得知某個實體而接回舊辨識期間。事件讀取不重新套用當前權限，因此晚讀仍保留
當時允許的短暫公開資訊。

對手的未知抽牌只顯示公開的移區，不帶牌面、私有位置或固定代號；私有 Look
沒有額外的 Step／Fact 來源。Search 只交付其實際公開的揭露、移動及洗牌結果。
同時揭露的牌與同一私有移動批次依觀看者已獲授權的資料穩定排序，未知同區重排不產生事件。
公開程序或卡文明示的數量仍可見。

不變式：事件存在性、欄位、局部編號與因果群組，只取決於該觀看者已獲授權的
資訊及公開程序。只改另一席的未公開牌面、私有排序或實體 ID，不應改變其可見
事件序列；失去位置知識後的新辨識期間不能串回舊 handle。

歷史傷害來源使用 producer 提供的原世代；來源已離開時只使用該交易內的原世代
公開快照作記憶，不讀取重新公開後的新手牌代號。沒有原世代證據的 Step source
欄位不建立物件連結。

普通進化與超進化保留不同的 `EvolutionState`，超進化的物件狀態在權威旗標完成後
封存；正式打出／解決的執行者由 typed producer 補入同一 Step 事件。批次排序使用
公開端點、印刷特徵與物件代號的結構化鍵，不依賴 Debug 格式。
