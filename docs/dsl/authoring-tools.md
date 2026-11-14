# 效果作者工具

新增卡片時，主檔放在 `authored/rules/author-effects/<卡包>/<record_key>.yaml`（例如 `BP02/BP02-084.yaml`）；寫入工具依卡號建立分片，不再取下一個數字，既有 `NNN.yaml` 保留原名。索引寫入 `authored/rules/effects-index/<卡包>.yaml`，每檔依 `record_key`、`scope` 排序，卡包取第一個 `-` 前的前綴。新增 runtime 測試只需新增 `sim/engine/tests/current_repository/runtime/*.rs`；build script 會自動探索並排序，不須修改 `runtime.rs`。

`sve-carddb author inspect/write/check` 使用同一個 current 選用入口。作者直接編輯 `authored/rules/author-effects/` 及 `effect-bodies/`，無需機器錨點、驗證檔或原型 baseline。

## 輸入 profile

公開 profile 讀取已審查的 card-data 與身分登錄，指定 JP 批次即可；`revision` 可省略，不要求作者固定 Git SHA。

```json
{
  "profile": "public",
  "repo": "/absolute/project",
  "jp_batch_id": "<已審 JP batch id>"
}
```

`authored`／`card_data` 可指定其他絕對根目錄。資料庫 profile 仍需 `database`、`recipe`、`frozen_inputs` 以重建相同來源上下文，依 CLI 型別接受配置；一般作者使用公開 profile 即可。來源歸檔及已審解析輸入的完整性契約不變。

## 查詢與手寫

```bash
uv --directory /absolute/project/carddb run sve-carddb author inspect \
  --inputs /absolute/work/public.json --card SYN-01
```

主檔與索引只登錄每張永久卡片的一個代表 `record_key`；同卡的 JP 印刷別名由 current 選用依永久身分展開，不重複寫入主檔。

主檔可直接寫整卡 DSL，也可引用具名本體：

```yaml
format: 2
kind: author_effect_set
cards:
  SYN-01:
    admission: light
    meta:
      written_by:
        - actor: {who: "Author", model: null}
          date: "2026-10-27"
    abilities:
      - line: 1
        use: draw-self
        params: {n: 1}
```

```yaml
format: 2
kind: effect_body_shard
records:
  - id: draw-self
    admission: shared
    params: {n: Nat}
    written_by:
      - actor: {who: "Author", model: null}
        date: "2026-10-27"
    body:
      spell:
        do:
          - draw: {who: self, n: {param: n}}
```

`admission` 可放卡、本體或用途，值為 `light | shared`；用途優先於本體，本體優先於卡。只要任一層明示 admission，該卡走直接作者入口，其餘未標用途保留為共用語義責任。未標 admission 的既有卡維持來源重建路徑，亦不需要錨點。共用整理與審查見[常見效果作者流程](common-family-authoring.md)。

## 表面的進化牌組卡片翻回裏面

`targets[].from` 與本文的 `choose.from` 接受 `{who: self, zone: evolve_deck, face_up: true}`，
可用 `name: にんじん` 或 `kind: evolve_follower` 等既有公開篩選，再以 `flip_down: chosen`
將選中的卡片翻回裏面。打出時的正式目標須滿足指定數量；結算中的選取沿用能做多少就做多少，
`upto` 可選零張，後面的獨立指示仍會執行。

翻面沿用 `Action::FlipDown` 與共用翻面流程，保留卡片世代及區域，回條只記錄實際從表面翻回裏面的成員。
對手仍記得曾公開的卡片，但不再獲得該裏面卡片的所在位置；持有者仍可檢視自己的進化牌組。
目前只接受自己的表面卡片與已選取的具名綁定，不接受對手或裏面卡片、`flip_up` 及翻面付款。
`flip_down: {ref: chosen, result: flipped}` 可命名結果，以 `count_of: flipped` 讀實際翻成功的張數；
只供張數判斷，不接受 `did`、卡片屬性讀取、後續移動輸入或跨出 `optional` 分支。
母なる慈愛先確認有至少兩張表面的自然・進化從者，再讓玩家選擇是否選兩張翻面，依成功張數等於二抽牌及回 PP。
不足兩張時仍可選該模式，但不能只翻一張（Q2294、Q2295）；不能拿選取張數代替成功結果。
シュヴァルグラン的翻面是入場能力本文的條件式指示，並非進化付款。

`flip_down.ref` 可直接指定自己進化牌堆的正面集合，例如 `{who: self, zone: evolve_deck, kind: evolve_follower, face_up: true}`；沿用公開進化牌堆篩選，翻面後撤銷對手的公開位置知識。具名結果仍記錄實際成功翻面的成員。

## 同名建牌張數規則

直接作者入口接受卡級 `deck_rule: {deck: main | evolve, same_name_max: N}`。
它對應第一面的完整規則句：`『自身卡名』は自分のデッキにN枚まで入れることができる。`
或 `これはエボルヴデッキにN枚まで入れることができる。`，可出現在效果文或附帶條款。
作者值須與目前來源完全一致，未宣告、錯區、改動上限或沒有對應卡文均拒絕；
來源規則行自動計入整卡覆蓋，不可再列為能力、reminder 或 token 來源。
其他行仍須完整處理；沒有對局能力的卡可寫 `abilities: []`。

此欄是建牌資料，不產生對局中的持續或誘發能力。current handoff 保留規則值與原行座標，
lowering 核對宣告及整卡覆蓋後，沿用既有的空能力卡程式。
構築索引與作者入口共用相同的固定句型解析器；索引仍從快照的已採納本文產生 `copies`，
既有純函式驗證器依構築名稱與區域套用上限。放寬同名張數不放寬主牌組或進化牌組總張數。
未標 admission 的來源重建入口與其他建牌句型未因此擴充。

場上的 `extra_trigger` 靜態效果依 `phase` 與 `who`，在階段開始時為符合控制者的原誘發能力增加 `n` 個獨立待處理項目。多個來源的增加次數相加；每個項目保留原能力的來源、控制者、條件與付款，結束階段的能力也涵蓋在對手結束階段誘發的己方 EX 能力。提供者須在事件發生時仍有效，已成立的待處理項目不因提供者離場而取消。

## 對局能力

場上持續防傷的 `prevent.on` 接受物件分支的 `any_of`，每個分支保留自己的公開篩選與
`not_same_name_as_this`。例如 BP21-001 的學院分支排除來源同名，獸分支仍保護同名的獸從者：

```yaml
static:
  effects:
    - prevent:
        kind: ability
        on:
          any_of:
            - {who: self, zone: field, kind: follower, trait: 学院, not_same_name_as_this: true}
            - {who: self, zone: field, kind: follower, trait: 獣}
```

符合多個分支仍是同一個受益物件；來源失去能力或離場後，沿用既有的持續防傷求值規則。

自己的墓場或消滅區可用分組正式目標表示不同原費條件的從者：

```yaml
targets:
  - bind: chosen
    from: {who: self, zone: cemetery, kind: follower}
    n: 2
    groups:
      - {orig_cost_le: 5, not_class: ナイトメア}
      - {orig_cost_le: 3, not_class: ナイトメア}
do:
  - move: {ref: chosen, to: field, result: entered}
```

`n: 2` 表示兩組各恰選一張，`upto` 等於組數時每組至多一張；後者接受兩組或三組。
每組以 `orig_cost_le` 或 `orig_cost_eq` 擇一限定原費，可加 `trait` 或 `not_class`。
同一張實體卡只能佔一組；選取時一併驗證全部組，不能用兩張只符合較寬條件的牌填滿。
分組不與角色、名稱互異、合計費用、動態張數或本文 `choose` 並用。
必要目標在打出時必須選足，即使場上只有一個空位也不降低目標數量；隨後由既有移區容量流程
從已選牌中挑選能入場的牌（BP20-084，Q2670、Q2671），未入場的牌留在原區。
`upto` 可選零張，獨立的後續指示仍會執行（ECP02-029 的生命回復）。

擲骰使用 `dice: {bind: die}`（可明示 `who: self`），降低為既有 `Action::Dice`，共用洗牌與隨機選取的可重現亂數串流。後續本文用 `{dice: die}` 讀取該次最終接受點數；未定義、先讀後擲或重複命名均拒絕。骰值不由玩家宣告。

場上的重擲許可寫成 `static: {effects: [{reroll: {who: self, times: 1}}]}`；只接受固定正整數、自身與無額外條件的場上宣告；只計入自己場上仍有效的印刷能力，失去能力或離場的來源不提供額度。每次擲骰後可逐次選擇重擲或接受，多個來源額度累加；重擲取代前一點數，下一次獨立擲骰重新取得額度（CR 5.20、Q2205–Q2209）。決策公開目前點數及剩餘額度；回答 1 重擲、0 接受，沿用既有整數回答邊界。

`auto: {on: {dice_rolled: {who: self}}, do: [...]}` 觀察一次完整擲骰的最終結果，本文用 `{event: value}` 讀取該次點數。點數條件寫在本文，不能放在事件頭；重擲嘗試不另計誘發或歷史。歷史條件接受 `history: {event: dice_rolled, who: self, window: this_turn, where: {value: {eq: 6}}}`；`where` 可省略，點數門檻為單一 `eq`／`le`／`ge`，限 1 至 6。只查詢當前回合的已接受結果，包含目前指示與來源進場前發生的擲骰（BP21-081，Q2753）。

`repeat: {n: 2, do: [...]}` 接受固定非負整數次數，沿用 `Control::Repeat` 與一般能力本文執行。每次依序完成同一段本文，本文中的決策、重擲與誘發都保留各次的事件資訊；本文內的骰子綁定不能流出迴圈。0 次不執行本文；`until`／`while`、具名本體參照，以及跨迭代參照或迴圈外回條收集尚未開放。本文仍須通過既有逐構造准入與限制；本次不擴充其中的模式、付款、牌庫觀測或回條語義。

固定次數重複也可執行牌庫頂移動：`repeat: {n: 3, do: [{move: {top: {who: self, n: 1}, to: ex}}]}`（神魚・ディズレスタン）。EX 滿時移動失敗，卡仍留在牌庫；模式選擇可在重複外層，只有模式本文重複，不重選模式（キングヘイロー，Q385、Q386）。本文可在同一次迭代用具名移動回條的 `before` 成員判斷成功移入 EX 的卡片特徵（太陽の勇士・セタス）；失敗回條是空的，不能沿用前一次成功，不把各次回條收集後帶到迴圈外。

`box: {ref: enemy, until: next_opp_turn}` 目前接受正式選取的單一對方場上從者；
卡文要求橫置時先寫 `act: enemy`，成為箱子本身不額外橫置。期限為下一個對方回合結束。
`targets[].from.boxed: true | false` 可篩選場上的箱子狀態，不能用於其他區域。
箱子失去當時卡面的印刷能力與成為箱子時已持有的授予能力，之後獲得的能力仍生效（CR 5.31.2.1）；
進化後卡面的能力依 CR 5.16.2.2 視為新獲得，箱子狀態延續但不消除這些新能力。
類型、種族與已施加的數值修正不因此消失。回合開始的規則重置受阻，效果重置仍可執行（5.31.3.1）。
多個箱子效果各自保留期限與能力來源截點；期限終了後尚有效的原能力恢復，離場不延續至新世代。
成為箱子前已待機的能力繼續解決；箱子離場時不產生已失去的謝幕能力。

`apply: {on: enemy, effect: {lose_abilities: {}}}` 讓一個正式選定的場上從者失去目前全部能力；
`on: this` 也可對目前在場的來源自身使用。
省略 `until` 或使用 `game` 時沒有額外期限（BP18-T04）。`until: turn` 持續到本回合的結束清理，包含結束階段的 Quick 時窗。
`until: next_opp_turn` 持續至下個對方回合的結束清理，經過額外己方回合也不提早到期。
清理後仍有效的能力恢復，其他尚未到期的失去能力效果繼續適用（CR 7.4.8、10.2.1.2）。
BP11-041、BP11-P11 採用此期限，並在解決中檢查自己墓場的原始費用是否有六種以上。

印刷能力、當時已授予的能力及持續授予都按共同能力截止點抑制；之後新授予的能力與進化後新卡面的能力仍有效，已待機的能力繼續解決。
這個效果不改變箱子狀態或類型；普通換區後不延續。來源在自己的解決途中失去能力不會取消當次解決，
也不會撤銷已獨立安裝的攻擊限制。CP03-072 的片段回歸依 Q1418／Q1419 驗證待機失能與 Stand 的領袖攻擊限制；不代表整卡准入。
解決結束、來源離場與 interval 期限、選擇性關鍵字移除及其他非正式對象仍不開放。
BP05-061 的「本回合及下個對方回合中」有區間語義，不能直接改寫成連續至下個對方回合末。

BP11-018 與 BP11-024 為整卡種子；後者選零個目標仍會執行後續搜尋（Q1505）。

基本關鍵字的持續授予使用 `static: {while: {awakened: true}, effects: [{grant: {keywords: [storm]}}]}`；省略 `on` 表示自身，省略 `while` 表示無額外條件。`grant.on` 可用目前場上的公開篩選，例如 `{who: self, zone: field, name: オルカ, token: true}`。接受七種基本關鍵字及 `drain`，場上來源失去能力或離場時不再授予；條件與受影響對象每次查詢都依目前狀態重算，包含稍後入場的對象，不會改寫印刷關鍵字或既有授予效果。此入口預設只在場上生效；一般能力定義授予與其他持續修正仍依各自的 runtime 准入範圍。

同一印刷能力可以讓每條 Grant 宣告使用自己的條件；例如 BP08-082 的同一來源行：

```yaml
static:
  effects:
    - if:
        cond: {cmp: {left: {count: {who: self, zone: cemetery}}, op: ge, right: 7}}
        then:
          - grant: {keywords: [assail]}
    - if:
        cond: {cmp: {left: {count: {who: self, zone: cemetery}}, op: ge, right: 15}}
        then:
          - grant: {keywords: [storm, bane]}
```

墓場張數即時變化時各自重算；15 張以上兩條同時有效（Q1112、Q1113）。`while` 若存在，還須同時成立。
每個 `then` 恰一條 Grant，不新增來源能力或拆開來源行；`else`、巢狀條件及其他條件式家族不開放。
條件仍經既有讀值准入，不能用條件式讀取未授權的私有屬性。這次只接普通場上 Grant；裝備或場上／EX 跨區的條件式授予仍拒絕。授予的能力定義（`grant.abilities` 內的 `static`）也不接受逐條 `if`。
既有 prevention If 特例仍須搭配同能力的移除禁止，且不能同時使用頭部 `while`；宣告數與限制維持。

同一印刷 `static.effects` 可混合既有的 Grant、防傷／傷害修正、費用修正（含自身進化費用）及送墓改消滅宣告。
引擎依原順序逐條交給對應的 provider，不因首項家族省略其他宣告，也不拆成多個來源能力。
每族原有的區域、條件、對象、讀取權限及期限限制都必須成立；例如費用修正仍以場上為來源，
裝備只可混合各族原先接受的無條件 host Grant 與 host 傷害修正。
能力失去時同源的各族一起停止；截止點之後新獲得的能力與原有到期／世代規則不變。
這個混合入口不含授予的能力定義，也不把名稱追加、移除禁止或其他 static 家族一併開放；
既有移除禁止加防傷的完整特殊形狀仍走原路徑。這是各族的組合准入，不新增條件式費用語法。

`static.effects` 接受 `banish_instead: {from: field, to: cemetery, on: {who: opp, zone: field, kind: follower}}`
（BP18-061）；省略 `on` 表示自身，場上來源與公開對象每次移區前重新求值，沿用 `while` 條件。
效果、付款與規則送墓共用移區替代；禁止消滅優先，因此被禁止消滅的卡仍依原指示送墓（Q2345）。
同批離場在移動前決定適用來源；來源失去能力或先前已離場時不再提供替代。
移區回條保留原本的墓場目的地及實際消滅目的地；替代後不算成功破壞，也不誘發送墓或謝幕能力（Q2346）。
來源自身送墓的付款若成功改為消滅，仍算付出成本（Q2387）。

`static.zone: [field, ex]` 另接受無額外條件、每個宣告只授予單一 `rush` 的持續能力（BP13-003／BP13-SL03／EBD01-005）；作者陣列的反向順序會正規化為同一 IR，有效區只允許標準順序的場上與 EX。來源位於任一區域時，皆使用相同的持續授予求值、能力失去及世代規則，受益者仍依既有場上篩選；來源移到其他區域後停止授予。單一 EX、其他區域組合、條件式、其他關鍵字組合與能力定義授予不包含在此新增入口。

名稱追加使用 `static: {effects: [{also_named: {name: ゴースト}}]}`；不指定 `zone` 表示這條名稱宣告在所有遊戲區域有效，`zone: field` 表示「場にいる限り」。這個預設只適用於 `also_named`，其他持續能力仍以場上為預設。兩者都保留印刷名；失去能力時不追加名稱，場上限定宣告離場後不延續。`on` 只接受省略或 `this`，不接受額外條件、其他有效區、混合修正或解算時施加名稱。

名稱宣告降低為既有 `Effect::Continuous`／`CharacteristicPatch::AddName`，由共用名稱集合供精確與包含篩選、同名交集、名稱種類數、搜尋與名稱互異選取使用。事件主體與來源資訊保存讀取當下的完整名稱集合；離場前快照仍保留當時有效的場上別名，新區域物件則重新求值。衍生物建立與閉包繼續以明示的完整衍生物來源及原名稱模板解析，不把也視為某個名稱當成另一張衍生物的定義。

`grant: {on: ally, keywords: [drain]}` 沿用世代綁定的關鍵字授予，可省略期限或指定 `until: turn`。吸血經同一實際傷害事件與待機佇列回復生命，來源定位保留授予者的印刷卡號與行，能力持有者、控制者及實例則屬受益物件；不同受益物件有不同實例。依 CR 12.13.3，印刷、施加及持續授予的多個吸血只產生一次回復。受益物件離場後的新世代不繼承授予；施加效果不因授予者離場而提早終止，持續授予則每次依來源、條件與對象重算。

一般誘發能力可寫在 `grant.abilities`，例如 `grant: {on: ally, until: turn, abilities: [{attack_time: {do: [{draw: {n: 1}}]}}]}`；`static.effects[].grant` 也接受同一個 `abilities` 陣列，並可同時授予關鍵字。目前接受場上的攻擊時、Main／End 時點與謝幕能力，本文與正式目標沿用一般誘發能力的檢查及結算流程；巢狀能力使用獨立的區域與綁定，不能引用授予者本文的目標。每次施加各自產生能力實例，持續授予保留提供者世代及效果來源；能力持有者與控制者則取受益物件。謝幕在受益物件離場前收集，已待機能力保留離場資訊。施加期限與來源離場規則同上；EX 區另接受既有的 Main／End 時點能力頭；loader 明確拒絕其他事件，以及在授予能力中再次施加授予。

`auto: {zone: ex, on: {placed_ex: {ref: this}}}` 表示「EXエリアにこれが置かれたとき」（CP04-007）：能力只在 EX 有效，觀察的是這張卡自己進入 EX 的那次移動，依 CR 10.7.4.2 由剛進入 EX 的卡自身觸發；不觀察其他卡進入 EX，也不在場上生效。EX 的 `phase` 能力（例如 BP10-110「自分のエンドフェイズが来たとき、これがEXエリアにあるなら」）與起動能力現在也接受 `フォロワー・アドバンス` 卡型，Advance 從者在場上可使用一般起動能力（BP10-059）。`{アドバンス起動}` 的付款與具名入場見下節。

一般持續能力使用同一個 `grant.abilities` 入口，例如 `abilities: [{static: {effects: [{prevent: {kind: any}}]}}]`。支援既有的防傷、固定傷害加減與傷害上限宣告，以及只授予關鍵字的單層持續能力；本文、受益者、提供者與期限沿用同一份授予狀態。查詢持續授予時讀取印刷及已施加的能力，避免再次遞迴查詢完整持續授予結果；持續授予另一份持續授予能力、在授予的持續能力中再授予 Definition，以及未接線的限制宣告會被拒絕。不同施加仍保留獨立來源，傷害替代的參照會區分施加次數，不公開已離場提供者的物件 ID。

固定播放減費可寫成 `grant: {on: {who: self, zone: ex, name: カーバンクルの煌めき}, abilities: [{play: {cost_delta: -2}}]}`。它降低為同一個靜態能力中的既有 `CostModifier`，以受益物件自身為範圍，經共同播放費用計算、合併獨立減費並把實際費用限制在零以上；不建立另一套費用狀態。只接受固定非正整數，不接受改成固定費用、條件式或可選費用。宣告在 EX 與播放解決區有效，外層施加仍可使用既有的本回合期限。在 EX 到解決區、解決區到場的播放規則移動，以及 EX 直接入場時，與其他已支援授予一同攜帶原效果；入場觀察前就可讀到承接後的能力，普通移區不繼承舊世代。

授予可施加到場或 EX 區的適正受益者；其既有能力仍須通過一般准入邊界。`grant.on: {follow: moved}` 沿用同一效果的成功移動回條，對新世代施加能力；失敗移動不產生受益者，不把原物件 ID 當成永久追蹤連結。來源與目標的有效區、期限及生命週期分開處理，並沿用既有授予層對失去能力的處理。

施加式授予的普通自動能力可使用 `on: {placed_ex: {who: self, sel: {name: 輝く金貨}}}`（BP22-022／BP22-SL08／PR-625）。此新增入口僅接受場上受益者、自己的 EX、單一非空確切名稱；不開放其他玩家、未篩名稱、額外特徵或有效區。沿用同一 EX 入場事實及授予實例：同一效果先授予再放入 EX 會觸發，單批放入兩張會各產生一次待機（Q2801、Q2800）；受益者離場後停止後續觀察，已提交的待機照常解決。

數值 `buff.on`／`set_stat.on` 與 `grant.on` 共用同一效果的入場結果解析：`move.to: field` 的具名結果，以及 `search.to: field` 中 `sel.kind: follower` 且數量明示 `n: 1` 或 `upto: 1` 的具名結果，以及每組均為從者 query、各取 `n: 1` 或 `upto: 1` 且沒有 `distinct_by` 的群組搜尋結果，都可寫成 `{follow: entered}`。它只取實際成功入場的新世代；滿場、沒有找到或選擇不找到時，對空結果的指示不做，後續獨立步驟照常結算。若後續還有對自身的加成，將它寫成獨立的 `buff.on: this`；搜尋沒有結果時，仍結算該自身步驟。

`exists: {in: {follow: entered}, keyword: ward}` 可在同一效果中檢查具名場上移動結果（BP13-092／BP22-R15／PR-416）。lowering 與 runtime 共用封閉的守護查詢形狀；它只檢查實際成功入場且仍在場上的同世代物件，並在條件求值時讀取從者目前有效的守護，包含持續授予；其他守護持有者不算。沒有入場、選擇不放入或場上已滿時條件為假，後續獨立指示照常執行。只接受已登錄的場上結果與 `ward`，其他區域、關鍵字、未宣告結果及額外篩選仍拒絕。

自己場上與 EX 的 `buff.on.any_of` 兩個分支各自沿用一般數值修正查詢，不限定 token 或相同類型；可依卡名合併場上與 EX 的普通從者，同一物件仍只套用一次。

例如兩區的 token 從者加成（BP16-002）：

```yaml
buff:
  on:
    any_of:
      - {who: self, zone: field, kind: follower, token: true, trait: 妖精}
      - {who: self, zone: ex, kind: follower, token: true, trait: 妖精}
  power: 1
  hp: 1
```

兩個 query 必須分別是自己的場上與 EX、`kind: follower`、`token: true`，且具有相同的非空 `trait`；兩區書寫順序可交換。其他玩家、其他區域、額外篩選、不同 `trait`、非 token 從者與 `set_stat` 聯集均不接受。這是一個數值加成指示，對結算時兩區的適正成員整批施加；EX 的數值修正在之後入場時保留（綜合規則 4.8.3.3）。

普通進化只觸發 `evolve_time`；超進化同時觸發 `evolve_time` 與 `super_evolve_time`，由玩家選擇結算順序（Q2078、Q2079）。BP16-002 先建立『フェアリー』再加成時，新 token 也受加成；反序時新 token 保持原數值，既有的場上與 EX『フェアリー』仍受加成。

搜尋固定一張並放到自己場上，可寫 `search: {sel: {who: self, zone: deck, kind: follower, name: クーガークローガール}, n: 1, to: field, acted: true}`。這張卡在進場事件發生前已是アクト狀態，不另做 Act，也不另詢問守護的入場姿態；找不到、選擇不找到或場滿未入場時，仍依搜尋規則洗牌。`acted` 只接受 `true`，且數量與目的區必須是字面 `n: 1`、`to: field`；`upto`、多張、分組、其他目的區與參數仍不開放。此姿態也不與 `distinct_by`、`cost_sum_le`、替代或分配目的地並用。

正式目標與本文 `choose.from` 的公開篩選接受 `traits_any: [獣, 魔法生物]`（任一類型；至少兩個互異的非空類型，BP13-051）、`traits_all: [絶傑, 魔界]`（同時具有全部特徵）、`token: true` 及 `names: [没頭の実験体, 多頭の実験体]`（名稱符合任一項），共用既有物件篩選與目標合法性檢查。

場上正式目標另接受 `acted: true | false`（橫置／直立）、`evolved: false | true`（只比對從者；`true` 包含超進化）與 `entered_this_turn: true`（本回合入場，每個回合開始時清除雙方場上的標記）；這些狀態篩選只用於場上候選，不接受領袖、觀察卡或墓場／EX 來源。`not_same_name_as_this: true` 也可用於公開正式目標與本文 `choose`，以能力持有卡本身的卡名比較，因此謝幕在來源離場後仍排除同名卡；與領袖候選並用時拒絕。能力頭成本的素材與棄牌查詢也接受 `traits_any`（BP14-107 墓場消滅、BP10-121 手牌棄牌）；領袖候選、極值查詢與分組 Look 仍拒絕（公開計數見下）。能力頭成本的素材查詢（例如 `return_to_hand.sel`，BP21-016、ECP01-004）與 `exists` 條件（CP01-076）也接受 `not_same_name_as_this: true`，付款或判斷時讀能力持有者目前的卡名；計數、名稱種類數等其他公開數值讀取仍拒絕。場上物件來源（傷害、破壞、起身等的 `{who, zone: field, ...}`）接受 `acted`、`kind: amulet`、`orig_cost_eq: N`（印刷原費恰為 N；數值修正的 `zone: ex` 來源同樣適用，BP12-070 分別對場上與 EX 各寫一次 `buff`），以及 `except: {in: 已選目標}` 排除同一能力先前選定的目標（例如「それ以外の相手の場のフォロワーすべて」）；與 `as`／`where` 極值並用時拒絕。公開存在與計數查詢可用 `where: {cmp: {left: {power_of: item}, op: ge | le | eq, right: N}}` 依有效攻擊力篩選。自己場上、EX 或墓場的 `count` 也接受 `traits_any`（「天使・フォロワーや堕天使・フォロワーが2体以上」，BP09-117），具備任一類型即計入一次；手牌、牌庫、消滅區與進化牌堆仍拒絕。`count` 的 `names: [甲, 乙]` 與單一名稱一樣可用於場上、EX、墓場及自己的進化牌堆；符合任一名稱就計入一次，並與同一查詢的其他篩選取交集，不擴張手牌、牌庫與消滅區的特徵讀取權限。模式數上限的自己場上類型計數也可加名稱篩選，沿用同一人口計數。公開存在查詢另接受 `evolved: true | false`（只限場上，BP13-080「エボルヴフォロワーがいるなら」）與 `traits_all`（BP12-011 EX 的妖精・プリンセス從者）；場上物件來源（破壞、傷害等的群體對象）接受 `hp_le: N`（有效體力上限，BP11-056）與 `not_trait`（「Aタイプを持たない」，ECP02-072），與 `where` 極值並用時拒絕。場上限定的持續能力可在 `static.while` 以 `standing: this` 讀取能力持有者目前是否直立；本文條件也讀相同的目前場上世代，來源已離場時不視為直立。起動條件仍不支援持有者姿態讀取。正式目標仍先宣告，才於解算 `standing: this` 時判斷來源目前是否直立；它不是時點事件的觸發篩選。

群體 `destroy` 的公開區域查詢沿用正式目標的狀態篩選，例如 `ref: {who: opp, zone: field, kind: follower, acted: true}`。在破壞指示解算時讀目前姿態；不改讀離場來源的舊快照，也不授權查詢私有區域。

`buff.on` 的公開物件查詢可用 `keyword` 篩選，例如自己場上具有 `ward` 的從者；依目前有效能力判斷，已失去該能力的卡不符合。 數值修正的 `keyword` 沿用物件查詢的有效關鍵字篩選，與 `other`、種類、名稱等條件取交集；來源仍是原有的場上與 EX，不包含墓場。

場上物件來源與持續能力的 `grant.on` 同樣接受上述 `where.cmp`，依每張候選目前有效攻擊力重算；與 `other: true` 等既有物件篩選共同作用，排除能力持有者。`grant.on` 沿用原有場上物件邊界，不將墓場卡當成數值或關鍵字的修正對象。

正式目標的「AかB」寫成 `targets[].from: {any_of: [<查詢>, <查詢>]}`（BP03-105 疾走或守護、BP04-128 攻擊力 1 以下或體力 1、BP10-116 只有墮天使分支帶 `other: true`、BP16-098／CSD01-007 墓場的法術或護符）。各分支是獨立的公開正式目標查詢，各自帶篩選、`other` 與區域；同時符合多個分支的卡仍只算一張，張數上限與 `n`／`upto` 共用。分支可讀不同區域，也可帶 `where`／`as` 極值，每個分支沿用單一正式目標的完整查詢；觀察卡（`in`）與領袖候選不屬於公開物件目標，仍各走自己的入口。搭配 `groups` 時，每組的條件加到每個分支上，須與該分支自己的篩選同時成立（例如分支 `orig_cost_le: 3` 與組 `orig_cost_eq: 1`）。後續本文對這個目標的區域與種類判斷只採各分支共同的玩家、區域與種類；只有一個分支帶的篩選不會被當成整個目標的性質。

場上正式目標與結算選取也接受 `power_le: N`／`power_ge: N`，依候選目前的攻擊力篩選（CSD02b-001「攻撃力3以下の…を選ぶ」、CP04-032「攻撃力3以上の…を選ぶ」）。極值 `where` 與聯集分支不帶這兩個條件，並用時拒絕。

手牌的 `choose.from: {who: self, zone: hand, ...}` 可接 `move: {ref: selected, to: field, result: entered}`，選項只向控制者公開；成功入場才公開卡片，且不支付印刷打出費用。多張手牌選取（如 BP17-098）與 Look 共用場上空位上限：入場必須緊接選取，出題時最多可選目前空位數。中間插入其他步驟或跨條件分支入場會在載入時拒絕；單張選取沿用既有滿場時留在原區域的處理。「手札のAかB」寫成 `choose.from: {any_of: [{who: self, zone: hand, kind: follower, trait: 透京, orig_cost_le: 3}, {who: self, zone: hand, class: ビショップ, kind: amulet, orig_cost_le: 3}]}`（BP18-098、BP13-090）：每個分支各自帶靜態篩選，同時符合多個分支的卡仍只算一張；分支可合併選牌者自己的手牌與 EX，但不可讀其他玩家的手牌或 EX，也不接受其他區域、`except`、極值或動態費用。

「自分の手札か自分のEXエリア」沿用同一個 `choose.from.any_of`（SP01-010），例如 `{any_of: [{who: self, zone: hand, kind: follower, trait: 兵士, orig_cost_le: 2}, {who: self, zone: ex, kind: follower, trait: 兵士, orig_cost_le: 2}]}`。兩區共用一次選擇與 `upto: 1` 上限，分支順序不影響私有權限；整題只向選牌者公開，對手仍看不到手牌候選。後續相鄰的 `move: {ref: selected, to: field, result: entered}` 沿用一般入場容量與事件處理，不支付卡片的打出費用。

本文 `choose.from: {in: created}` 可從同一能力先前的公開物件綁定再選子集，例如 `create.result` 或正式目標；選擇者沿用 `by`。張數沿用 `n`、`upto`、`any`；選擇時只保留原世代仍在場上、EX、墓場或表向消滅領域的卡，不重新搜尋同名物件。手牌、Look 與進化牌堆的選取仍遵循各自的權限入口。

Look 的私有選取可寫 `choose.from: {any_of: [{in: seen, kind: follower, trait: クリスタリア}, {in: seen, kind: follower, trait: 妖精}]}`，表示「A 或 B」的同一候選集合（BP02-008、BP21-010）；每個分支須讀同一個 Look、各自帶篩選，同時符合多個分支的卡仍只算一張。`groups` 表示兩組或三組各至多 1 張，搭配 `may: true` 與 `upto: <組數>`；每組須有 `kind` 或非空 `trait`，可加 `class` 或 `orig_cost_le`（BP05-003 的エルフ從者與法術、BP11-086 的荒野卡與低費從者、ECP02-034 的キュート／クール／パッション）。每張實體卡最多佔一組，同時符合多組也不能重複選取；某組沒有候選不妨礙選其他組。候選與選擇仍只向控制者公開，放入手牌的公開另寫 `reveal`。`groups` 不與 `any_of`、外層特徵或合計費用並用。可多選的 Look 選取若要進場，`move: {ref: <選取>, to: field}` 必須緊接在 `choose` 之後、位於同一層本文；選項上限在出題時依空位計算，中間或條件分支內的進場會在載入時拒絕。分組或聯集拿回手牌不受場位限制。

已從自己的 Look 選出的牌可用 `banish: {ref: picked, result: banished}` 消滅（CP03-053）。這個觀察綁定的成功 Before 成員可用於 `move: {ref: {in: seen, except: {in: {objects_of: banished, view: before}}}, to: deck, position: bottom, order: chooser}`，扣除實際消滅的牌後，將其餘牌依選擇順序放到牌庫底。不以選取張數或預先計數代替成功回條，也不授權結果的新世代引用。執行時仍須是本次私有 Look 選取的成員、留在自己的牌庫且位置仍已知；未觀察的牌庫、對手的牌庫與直接對整份 Look 的消滅均不接受。墓場或 EX 的公開正式目標可沿用一般 `banish: selected`，不需要先看牌。

本文用 `choose` 從自己的單一墓場選固定正張數時，也可用 `banish: {ref: fuel, result: banished}`，
以 `count_of: banished` 讀這次成功消滅的 Before 成員數。這不開放對手、聯集區域、正式目標或可變張數的具名消滅。
悪意の憑依（BP08-085）在自己墓場至少四張時才詢問 `optional`；接受須恰好選四張，成功消滅四張才讓結算中的法術回手。
結算中的自身不在墓場（Q1117）。不足四張不能選依 [R-0018](../../authored/rules/rulings/R-0018.yaml) 從 Q2295 類推，強度為 `generalized`。

「元のコストの合計がN以下になるように」寫成同層的 `cost_sum_le: N`（非負整數常量），限制所選全部卡片的印刷原費合計；費用修正不影響。目前接受 Look 的私有 `choose`（單一篩選或 `any_of` 聯集，CSD02b-002、BP21-025），以及墓場等公開區的正式目標與本文 `choose`（BP22-091）。出題的張數範圍沿用印刷數量；可行性與結算時的最低張數依最便宜的候選能放進合計的張數計算，超過合計的組合在作答時拒絕、題目不變；必要目標放不下時與其他目標不足相同。不與動態張數、領袖候選、手牌私有選取、`groups`、Advance 選擇並用。

マナリアパーティー（BP21-043／PR-571）看頂五張，再以學院從者、至多三張與原費合計六以下的私有選取放到場上，餘牌依操作者順序放到底。
同卡被自己的學院卡能力從手牌捨棄時抽一張，使用 `discarded.by.ability_of` 的特徵限制；法術主本文以 `section: 1` 與 `rule_sections` 承載，和手牌事件能力分開。

`search` 也接受 `cost_sum_le`（BP18-025、ECP01-051），限搬到單一區域的搜尋；可與 `any_of` 篩選、`distinct_by: name` 或 `groups` 並用，合計涵蓋所有找到的卡。單一篩選的搜尋改以一組涵蓋整個篩選後牌庫的分組題目作答，與名稱互異搜尋相同；找不到或少找仍依搜尋規則，超過合計的組合在作答時拒絕。分配到手牌與墓場的搜尋、找到後改放場上的替代搜尋不接受此欄。

起動成本的 `act` 素材可寫 `{any: true, cost_sum_ge: N, sel: {...}}` 表示「元のコストの合計がN以上になるように好きな枚數アクト」（BP11-P30）：出題時若全部候選合計仍不足 N 就不能起動，作答時合計不足的組合拒絕、題目不變；原費取印刷資料。其他素材動作、固定張數與交換付款不接受此欄。操縦中的乘物護符除【疾走】外也接受印刷的【突進】。

`play.additional_cost` 在卡片宣告打出前必須完整支付，沿用能力費用的 `Cost` 操作與素材選擇；付不起或素材不合法時整次打出取消。目前限法術，且不與模式付款或能力頭的可選付款並用。已接通自己場上素材的送墓／橫置、自己公開 EX／墓場素材的消滅，可用 `steps` 保留多組成本的順序。這不是可選付款，也不會在效果結算時才收費。`result` 可供同次法術本文以 `paid: {receipt: 名稱}` 讀取；指定進場素材條件、私有手牌材料與 Quick 預檢仍須各自的支援入口。

數值算式接受 `plus`、`minus`、`times`（各恰兩項）、`min`（非空列表）與 `per: {of, unit}`。`per` 是「N 枚につき」：`of` 除以正整數 `unit` 後向下取整（負數往負無限大取整），`unit` 為零在載入時拒絕；有號溢位使整個指示失敗、不改變局面。`max` 與 `half_up` 尚未接線。傷害事件頭（`damaged`／`dealt_damage`）的本文可用 `{event: amount}` 讀該次實際傷害值；其他事件頭、起動與入場曲不提供此讀值，`{event: value}` 亦未接線。

`play.cost_delta` 可使用既有公開計數、`minus`／`times`／`per`／`min` 運算與 `play.define` 別名，依付款時的公開數量加減印刷費用；正數為加費，負數為減費，所有修正合計後才以零為下限。場上、EX、墓場與自己表向進化卡的計數沿用一般數值與選擇器檢查，不能藉此篩選隱藏手牌／牌庫。消滅區只接受不加篩選的整區張數（`{count: {who: self, zone: banish}}`，BP17-051「自分の消滅領域の枚数」），與本文讀消滅區張數相同；加上種類等篩選仍拒絕。`in_zone: {ref: this, zone: ex}` 在減費條件中讀取本次打出前保存的來源區域，因此卡片移入結算區後仍能正確付款；目前接受手牌與 EX，且可與公開存在／數量條件組合。

場上持續打出費用修改可用 `static.effects: [{cost_mod: {when_playing: {who: opp, kind: spell}, delta: 1}}]`；`when_playing` 可依玩家、卡種、類型、職業、卡名、原始費用上限及 token 篩選，或用 `any_of` 組合。省略區域時包含手牌與 EX，明示 `zone: ex` 時讀本次打出前保存的來源區域。持續來源每次付款依目前場上能力重算，來源離場或失去能力後停止套用；`static.while` 使用已准入的公開付款條件，但不接受僅本次打出才有的來源區域讀值（`in_zone`）。

費用修正條件可讀本回合 `event: played` 的 `history_count`，沿用一般打出歷史的玩家、卡種、類型與 token 篩選；第一張符合的卡正式打出後，後續費用立即依已提交歷史重算。

「本回合下一張」使用 `player_effect: {who: self, effect: {cost_mod: {when_playing: {who: self, trait: 武闘竜人}, delta: -3, next_only: true}}, until: turn}`。此安裝保存玩家與打出篩選，涵蓋稍後才取得的卡片，不捕捉隱藏手牌。整組共用一次額度，僅在符合篩選的卡適正打出且全部付款成功時消耗；預覽、非符合卡及失敗宣告均不消耗。安裝者離場不提早失效，未用額度在本回合結束時移除。`delta` 與非負整數 `set` 二擇一；各費用層的增減合計後才以零為下限。

整卡 DSL 在索引中以永久 CardId 登錄一個代表卡號。current 選用依身分登錄展開同 CardId 的所有官方 JP 印刷卡號，共用代表卡的程式及來源定位；卡面仍讀各印刷的快照資料。不從卡號字串推測別名，也不為別名另登索引或保留原型程式。未解決的裁定同時阻擋這些別名。

實體 token 使用同一主檔、具名本體及行覆蓋入口。卡片層的 `token: {name: 操り人形, template: true}` 指定規則名稱與同名 token 的代表印刷；`template` 預設為 false，省略整個 `token` 時使用快照卡名。同名 token 有多個已選用程式時須恰有一個代表。卡面、數值與型別仍來自快照，`token` 只能標在單面的 token 卡上；印刷的突進、守護等以 `keywords` 宣告，與一般從者共用 lowering 與執行規則。

`equip: {on: this, token: アメスアミュレット}` 建立一枚已准入的具名裝備衍生物，放入獨立的 equipment 區並附屬於仍在場的來源從者；不占普通場格，同一宿主可裝備多枚。建立與一般衍生物共用快照模板、實體配置及來源驗證，並回傳具型別的裝備事件。普通 token 不能用此入口，任意接受者與可變數量尚未准入。宿主離場時解除附屬並依規則消去裝備；裝備控制者隨宿主控制者改變，擁有者不變，正常進化保留附屬關係（CR 14.5、11.11）。

裝備自身的裝備完成能力寫成 `auto: {zone: equipment, on: {equipped: {equipment: this}}, targets: [...], do: [...]}`，
只准入 Printed、一般能力、Equipment 區、AfterEntry 與逐事實觸發。
只有成功建立並附屬的裝備才產生事件；初始關聯或無效宿主不會補發事件。
本文 `{event: holder}` 是當次事件封存的宿主引用，與持續能力中查詢目前關聯的 `host` 不同；不推定 `it` 為宿主。
來源是裝備自身；宿主先離場使裝備隨後消去時，已待機或解算中的來源仍沿共用 LKI 保留，宿主舊世代不改指回手後的新世代。
CP04-T08 氷竜剣必須先有一名合法敵方從者目標，才會橫置它並替事件宿主永久加2／2；
目標不足時整個能力不能打出，不能只取加攻體（[Q2539](https://shadowverse-evolve.com/cardlist/?cardno=CP04-T08)）。
已是橫置的從者仍可作此目標，數值修正沿既有 Modify，不成為裝備的持續授予。

裝備區的持續授予使用 `static: {zone: equipment, effects: [{grant: {on: host, abilities: [{activate: {cost: {act_self: true}, do: [...]}}]}}]}`。`host` 在裝備持續能力內指目前仍在場的宿主從者；內層一般起動的 `this`、控制者與費用則屬宿主。不同裝備提供相同印刷定義時保留不同能力實例，起動時須指定能力實例；僅以印刷定義指定的入口遇到歧義會拒絕。提供者離場或失去能力、或宿主失去能力後不再授予，宿主離場後的舊實例不能重用。裝備另可授予下述 Played 自動能力與受驗的 UB 起動；場上光環的起動授予與施加式 UB 授予仍未准入；施加式一般起動見[正式執行子集合](author-effect-shapes-1.0.md#施加式一般起動能力)。

裝備的 `grant.on: host` 也接受場上 `auto.on.played`，沿用正式打出事件與既有待機來源，不另建觀測者表。內層 `this` 是宿主，`power_of: this` 讀取宿主目前資訊；宿主已離場時使用該次待機保留的來源資訊。`per_turn: 2` 按實際回合及能力實例計數，同一宿主的不同裝備各有額度。只有成功適正打出的法術符合 T06 的篩選；付款失敗、其他玩家的法術及從者打出不符合。

`half_up: E` 沿用既有有號算術 IR，代表 `ceil(E / 2)`，正負奇數與有號整數邊界都依向正無窮方向取整，不先加一造成溢位。裝備授予的能力在宿主或裝備失去能力後不再形成新觸發（[Q2504](https://shadowverse-evolve.com/cardlist/?cardno=CP04-T06)）；宿主離場前已捕捉的待機仍沿共用來源資訊處理。此批不開放場上光環或施加式授予的 Played 定義。

裝備授予的 `auto.on.dealt_damage` 可使用 `ref: this`、`kind: ability`、
`recipient: {who: opp, zone: field, kind: follower}` 與 `aggregation: batch`。
這是 CP04-T11 闇斧ナハトファング的封閉子集合：同一指示實際對一體以上的敵方場上從者造成正傷害，
每個能力實例只待機一次；不同裝備各有實例。
不同指示分別觸發，是依不同時點的事件分別誘發推得；Q2549 直接說明的是同一指示的多個接受者。
全防止或零傷害不算，對領袖、己方從者、其他來源與戰鬥傷害不符合此篩選。
[Q2549](https://shadowverse-evolve.com/cardlist/?cardno=CP04-T11) 的五目標傷害只產生一次觸發；
批次本文不能讀取其中某一命中的事件角色。來源、待機與失去能力仍沿用既有裝備授予流程。

裝備自身的傷害修正可寫 `static: {zone: equipment, effects: [{damage_mod: {source: host, delta: 2}}]}` 或 `{damage_mod: {on: host, delta: -2}}`，沿用既有傷害替代流程，涵蓋能力與交戰傷害。這些是裝備的能力，宿主失去能力不會移除它們；裝備失去能力後停止修正。目前只接受無條件、固定整數加減，且來源與接受者二選一為 host；其他傷害種類、對象、上限及防止宣告仍須另行准入。

所有觸發能力都可用 `targets` 宣告正式目標；費用時點依[作者語法 §3](author-syntax-1.0.md#3-費用目標與打出程序)區分頭付款與本文付款。控制者在檢查時點打出待機能力時選擇目標，沒有足夠合法目標就不發動，不執行後續效果。謝幕在來源離場後選擇目標。目標宣告前不執行本體；同一印刷行的入場曲與謝幕仍以不同 `scope` 區分。模式目標、單一目標組的分配傷害，以及 `at: resolution` 公開計數讀值沿用共同打出規則。模式目標（`option: N`）的候選須是不依賴其他選擇的公開集合；「場の他のフォロワー」的 `other: true` 只排除能力持有者自身，可用於模式目標（BP15-114）。

場上來源的 `auto.on.enter_field`、`left_field` 與 `destroyed` 共用事件主體篩選：`who` 限制該時點的控制者，`sel` 可篩 `kind`、`trait`、`name`、`class`、`other`，進場與離場另可篩 `token`／`not_token`；省略的條件不加限制，省略 `who`／`sel` 可監看所有物件。進場讀進場後資訊，離場與破壞保留離場前資訊；`this` 仍是能力來源，不自動改指事件主體。`enter_field` 的 `sel` 另接受非空 `name_contains`（「カード名に『アルベール』を含む…が出たとき」，BP13-027），依入場後的完整名稱集合比對片段；其他事件頭不接受此欄。`enter_field` 的 `sel` 也接受 `traits_any: [学院, 獣]`（「学院・フォロワーか獣・フォロワーが出たとき」，BP21-008），至少兩個不重複的類型、任一符合即成立，同時具有多個類型也只觸發一次，不與單一 `trait` 並用；`left_field`、`destroyed` 等離場事件仍拒絕（Played 另有自己的 `traits_any`，見下）。`left_field` 包含送墓、回手、消滅與移 EX，`destroyed` 只由實際破壞成立，付款送墓不會觸發；`moved: {from: field, to: cemetery}` 只監看送墓；`moved: {from: field, to: hand}` 只監看場上返回手牌，可用 `ref: this` 或 `from: {who: self, zone: field}` 限制自身或自己控制的卡。回手事件在離場前收集，因此來源自己回手仍會觸發；墓場回手不會觸發。來源與主體同批離場時，來源使用封存資訊，既有自身離場與謝幕各只收集一次；正式目標、付款與限次沿用共用觸發流程。

`enter_field.sel` 的 `orig_cost_eq`／`orig_cost_le`／`orig_cost_ge` 可與既有主體、玩家及自己回合條件組合，例如 `sel: {kind: follower, orig_cost_le: 2}` 搭配 `during: {turn: self}`。比較入場主體的印刷原費，不因有效費用或付款折扣改變。

事件本文的 `it` 共用標頭的型別綁定，可指向進場、入 EX、攻擊、進化、離場及破壞的主體；`this` 始終是能力來源。進場使用新世代，離場與破壞使用離場前世代；舊事件的主體離場後再回場，不會強化回來的新世代。沒有卡片主體的時點及起動能力不能使用 `it`，也不以來源代替缺少的事件欄位。本文條件可用 `exists: {in: it, trait: 獣}` 檢查該主體是否符合靜態篩選（BP09-002「場に出たフォロワーが獣・フォロワーなら、代わりに…」寫成 `if`／`else` 兩種強化），可與 `not`、`all`、`any` 組合；判斷比照已選正式目標：讀主體目前仍屬於事件那個世代時的特徵，主體已換世代（例如離場、破壞後）就不符合，不會改讀離場前的快照。

「前者の攻撃力」「それの攻撃力」這類在本文前段強化後才讀的值，寫成 `power_of: <目標>`（BP17-090、BP13-008、BP07-007、BP08-058）：接受至多一體、只從場上或 EX 選取的正式目標或本文 `choose`（雙方皆可），讀它結算到這一步時的目前攻擊力，因此包含同一本文先前的強化；EX 的從者讀它在 EX 目前保留的攻擊力，護符、已離開原區域或換世代時讀為 0。多張目標、手牌或其他區域的目標仍拒絕；要固定在選擇當下的數值時仍用 `at: selection`。

`at: selection` 的單一正式目標捕捉可讀 `cost_of` 與 `orig_cost_of`，沿用同一選擇回條與數值捕捉；原始費用取印刷資料，有效費用取當時修正。捕捉發生在選定後、第一個效果指示前，之後目標被破壞或返回手牌都不改變該數值；不授予隱藏區域或任意多物件讀取權限。

可選的單一正式目標（`upto: 1`）可用 `at: selection`、`selection: card` 與 `value: {orig_cost_of: card}` 捕捉選取時數值，`cost_of`、`power_of`、`hp_of` 同理。零張先走既有空集合條件，結果為 0；一張才讀該物件並凍結數值。一般的 `orig_cost_of: this` 仍是來源資訊讀值。

公開正式目標的集合也可捕捉 `{at: selection, selection: chosen, value: {distinct_orig_costs: {in: chosen}}}`，以所選整組卡片的印刷原費計算種類數。它沿用選擇回條與 `DistinctCount`，不受有效費用修正影響；例如 BP08-104 比較結果是否為 1，只有整組原費相同才回手。集合名稱須與 `selection` 一致；私有手牌、Look、未知選擇與本文選取仍拒絕。

攻擊本文可用 `is: {ref: attack_target, kind: follower}` 檢查本次宣告的目標，以及 `hp_of: attack_target` 讀取仍在場同世代目標的當前體力；`attack_target` 也可作已支援數值指示的對象。攻擊領袖仍產生待機能力，條件在解算時查（R-0003）；目標離場或換世代即不符合，不追隨新物件。這些讀值均由同一待機事件提供，不能在起動能力或其他事件偽造攻擊目標。

`moved: {from: {who: self, zone: field}, to: cemetery, sel: {kind: follower, keyword: ward}}` 依每個從者離場前的有效守護判斷，包含印刷、授予及持續效果提供的守護；直立也會觸發，來源自身及多個副本同時離場仍逐事件形成待機（Q846–Q851）。只讀同批移動前的狀態，結算時不改查墓場中的新世代。此關鍵字入口不開放其他關鍵字、目的區、事件或歷史查詢。

`enter_field: {who: self, sel: {kind: follower, other: true, orig_cost_eq: N}}` 接受正整數常量，依入場主體的印刷原費判斷；費用修正與實際付款不影響結果。每次其他己方從者成功入場各自判斷，能力來源本次自身入場不算，對手回合也適用（BP21-084，Q2754）。此入口不放寬其他事件或歷史的原費篩選。

場上或墓場來源的 `auto.on.played` 沿用共用事件主體篩選；`who` 可省略或指定 `self | opp`，`sel` 可篩 `kind`、`trait`、`name`、`class`、`other`、`token | not_token`，`during: {turn: self}` 限自己回合。事件以適正打出成功為準，包含已支援的從者、護符、法術及護符／法術 token；付款失敗、效果直接放場與生成 token 不產生打出事件。場上觀測者必須在成功時已位於場上，新打出的卡不倒算自身這次打出；墓場來源同樣在法術本文或入場前擷取，不能事後補觸發。正式目標、付款及每回合一次限制沿用共用觸發流程。

Played 的 `sel` 另接受 `orig_cost_eq`、`orig_cost_le` 或 `orig_cost_ge`，三者最多指定一個，值可為 0。比對已提交打出對象的印刷原始費用，不受折扣、有效費用或實付費用影響；費用未知時不符合條件。`traits_any` 接受至少兩個互異的非空類型名稱，任一類型符合即可，同時具有多個類型也只產生一次觸發；不可與 `trait` 並用，且只可搭配 `who`、費用條件與 `kind`。費用條件只供 Played 事件頭使用；`traits_any` 另開放給 `enter_field`（見上），兩者都不開放給其他事件、歷史查詢或一般條件本文。

`auto.on.played.sel.any_of: [{trait: なかよし部}, {trait: プリコネ, kind: spell}]`
在同一個打出事件中檢查「指定類型卡片，或另一指定類型的法術」。沿用打出成立時的卡片資訊，
一張牌同時符合兩支也只產生一次待機；未成功打出、單純放入場上及對手打出的牌都不算。
此入口要求 `who: self`，不限制在自己的回合（クロエ Q2434）。

目前只接受依序為「單一類型」與「另一單一類型加 spell」的兩支，不能混入費用、名稱、
其他卡種、`during` 或額外篩選；既有 `traits_any` 同類型聯集的行為不變。

場上來源的 `auto.on.attacked` 沿用共用事件主體篩選，`who` 可指定 `self | opp`；`sel` 可篩種類、類型、名稱、職業、token 與 `other`，另可用 `keyword` 篩有效關鍵字。`sel.as` 配合 `where` 可檢查該攻擊者的有效攻擊力下限，或用 `any` 組合關鍵字 `exists`；條件在真正宣告攻擊時判斷，先產生待機能力，結算後才進入 Quick。`this` 仍指能力來源，`as` 的具名別名只供標頭條件使用；本文可用保留的 `it` 指向本次攻擊者。

`auto.on.dealt_damage` 以同一筆實際傷害的來源套用事件主體篩選；`auto.on.damaged.recipient` 可指定 `this` 或 `{leader: self | opp}`。兩者的 `kind` 可省略或指定 `ability | attack | combat`，`during` 可限制回合；先處理禁止／防止，再以實際正傷害判斷，全防止與零傷害沒有事件。對領袖的攻擊傷害屬 `attack`，不屬 `combat`；從者交戰中攻擊方造成的傷害同時符合兩者，反擊方符合 `combat`（[Q282](https://shadowverse-evolve.com/cardlist/?cardno=BP02-020)）。傷害來源與受傷者各自篩選，正式目標、付款與每回合一次限制沿用共同待機流程。

`life` 的 `who` 接受 `self` 與 `{master_of: <目標>}`：後者是「それのリーダー」，取已選目標在選取時的控制者，目標隨後被破壞或移區仍回復同一位領袖。對手或對手集合的 `life` 尚未接線，載入時拒絕。`set_life` 的 `who` 接受 `self` 與 `opponents`（「相手のリーダーすべての体力をNにする」，BP02-075），其他對象拒絕。`draw` 的 `who` 同樣接受 `self` 與 `{master_of: <目標>}`（BP06-112「それのプレイヤーは1枚引く」），由該玩家抽牌並作為指示的執行者；其他玩家集合尚未接線。

`auto.on.life_changed` 可用 `who: self | opp`、`direction: up | down` 與 `during` 篩選領袖生命實際變更；省略條件即不限制。加減或設定生命都按變更前後的實際差額產生事件，差額為零不產生事件；生命不封頂於 20。生命支付與傷害造成的減少亦經同一變更入口，但直接加減生命不是傷害，不會因此觸發 `damaged`。這些事件頭沿共用觸發額度，可用 `per_turn: N` 宣告每個實際回合的正整數上限。

`history_count: {window: this_turn, event: life_changed, who: self, where: {direction: down}}` 計算本回合生命實際減少的次數，包含生命付款、能力傷害與攻擊傷害；零差額不計。沿用生命變更入口既有的本回合計數，稍後入場的卡也可讀取，回合開始時清空雙方紀錄。生命付款低於零時失敗，整份付款與其歷史一併回復；支付後恰好零仍可完成付款，再由通常敗北規則判斷。

`history_sum: {window: this_turn, event: life_changed, who: self, where: {direction: up}, field: delta}` 累加實際增加量；途中減少不扣抵增加量，設定生命亦按實際差額計算。兩者必填玩家與方向；次數查詢接受 `down` 的 `who: self | opp`，另接受 `up` 的 `who: self`，總和只接受 `up`，不接受 `distinct: subjects`，這些入口不接受其他歷史視窗、來源篩選或排除本次執行。

`history_count` 的 `life_changed` 開放本回合自己 `where: {direction: up}` 的事實數；一次實際正增量只計一次，與 `history_sum` 的增加總量分開。零變動、減少與對手增加不計，`set_life` 以實際前後值判斷，回合交替清除。

`history_sum: {window: this_turn, event: payment, who: self, where: {cost_kind: earth_rite}, field: removed_stack}` 累加本回合自己的【土の秘術】實際取下的【スタック】個數，跨多次付款加總（Q1895～Q1897）。只接受 `who: self` 與 `this_turn`；可作 `usable_if` 等條件的數值。付款計入雙方共用的回合計數，拒絕或回復的付款不計，回合開始時清空。
正式目標 `upto` 可直接讀這個封閉的增加次數查詢；上限為 0 時仍可選 0 個目標並繼續本文。此入口不放寬費用、搜索上限或其他歷史查詢。代表卡 BP21-092／BP21-SL22 與 BP21-T09（Q2769）。

`history: {window: this_turn, event: discarded, who: self, from: hand}` 查詢本回合自己是否曾捨棄手牌；`history_count` 使用相同查詢計算實際捨棄的張數。只接受這個玩家與來源形狀，`distinct` 可省略或為 `facts`，不接受主體篩選、其他來源、排除執行或 `subjects` 去重。效果、正式費用與手牌上限的捨棄都經共同入口計入；普通移區、消滅與抽牌不計，失敗付款連同歷史回復，雙方歷史在下一回合開始歸零。稍後入場的卡仍能讀取本回合較早的捨棄。

同一個 `history` 條件可放入 `play.cost_delta.if`，在共同打出付款流程讀取，適用手牌與 EX 起點；此費用條件入口不因此開放其他歷史事件。一般 `auto` 仍依 R-0004／R-0009 先選正式 `targets`、結算處理到 `if` 時才讀歷史。結束階段的能力先於手牌上限捨棄結算，因此稍後的規則捨棄不會補做已略過的傷害（Q302）；同一結束階段內較早結算的能力所支付的捨棄則已可讀取。

打出費用條件中的 `count` 可沿用一般數值入口所支援的計數，包括 `zone: evolve_deck`、`face_up: true` 與 `kind: evolve_follower`。因此表向的 Advance、非進化從者及裏向卡不納入此篩選；費用計算與本文使用相同的公開區域讀取邊界。

一般長寫 `auto` 的冒號在本文寫 `pay: {cost: {act_self: true}, do: [...]}`（送墓用 `self_to_cemetery`；可用有序組合）。正式目標在打出時先選，到了該指示才決定付不付；可放在已支援的 `if` 內。拒付或付不起只略過該段本文，能力仍算已打出並結算。付款原子提交，失敗保留先前已結算的效果並回復本段付款；來源須仍在場上，不能用另一張同名卡付款。這個結算付款子集只接受場上普通自動能力的自身橫置／送墓，不接受 EX 來源、私有材料或任意成本。

這三種事件頭尚不接受 schema 中的 `by`。`dealt_damage.zone` 僅接受下述自己場上類型來源的 `field` 子集合；其他區域或組合在載入時拒絕，不會忽略篩選條件。

敵方從者收件者 `recipient: {who: opp, zone: field, kind: follower}` 僅接受 `aggregation: batch`，來源形狀封閉為：

- `ref: this`、`kind: ability`，沒有 `who`、`sel`、`zone`、`during`（既有裝備授予形狀）。
- `who: self`、`sel: {trait: <類型>}`、`zone: field`、`during: {turn: self}`，省略傷害 `kind`（ECP02-057）。
- `who: self`、`sel: {trait: <類型>}`、`kind: ability`、`during: {turn: self}`，沒有來源 `zone`（CP03-064，可觀察己方法術）。

後兩種不混用 `ref: this`，`sel` 不附加種類、名稱、職業等條件，也不開放對手回合、其他來源區域或其他傷害種類組合。每個成功傷害指示的多名收件者只待機一次，兩個指示分別待機；`per_turn` 仍共用通常的能力實例額度。交戰傷害符合第二種，零傷害與完全防止不產生事件。來源區域與公開特徵讀取權威來源快照，待機或執行期間離場仍使用原區域最後資訊，不回讀新世代；受傷者是否敵方場上從者在傷害提交時判斷，致死傷害也計入。

`apply: {on: 目標, effect: {add_trait: {trait: 武装}}}` 對場上從者增加類型，沿用
`Action::Modify` 的 AddTrait 與既有授予狀態；`until` 可省略或為 `game`。
接受者只寫在外層 `apply.on`；`add_trait` 內層 `on` 尚未准入。
既有類型保留，重複增加同一類型不產生重複值；目標篩選、事件主體及離場 LKI 都讀有效類型。
授予者離場不撤銷已結算的類型；接受者離場後不追隨新世代，規則移動沿用授予狀態的承接。
此入口不開放 RemoveTrait、靜態類型修改、混合 patch 或其他期限。

`auto.on.acted` 與 `auto.on.stood` 訂閱實際橫置／起身，可用 `ref: this`、`who: self | opp` 或與進場相同的 `sel` 主體篩選。能力指示、費用、攻擊宣告、入場守護、結束階段守護及規則起身都經共用姿態入口；已是指定姿態的物件不產生事件。每個變更物件各是一個事實，本文與每回合一次限制沿用共用 pending 流程。

`auto.on.stat_changed` 接受 `ref: this`、`stat: power | hp`（或清單）及 `direction: up | down`。省略欄位表示不加該項限制；一個指示對同一物件的攻擊力／體力差額合為一個事實，清單採任一欄位符合，不因兩項都增加而重複觸發。零差額不產生事件；明示數值指示與超進化的 +1/+1 會產生，普通進化換底值、持續條件重算與期限到期不產生。這兩類事件頭目前使用場上來源與既有每回合一次限制，不忽略其他未支援欄位。

進化頭的 `usable_if` 可用 `history: {event: stat_changed, window: this_turn, where: {ref: this, stat: [power, hp], direction: up}}`（BP11-035／BP11-SL07）。條件讀同一物件世代本回合已發生的正差額，任一指定屬性增加即符合。省略 `stat` 時讀兩項，亦可明示單項。先受傷再獲得體力，即使只回到印刷值，仍符合 Q1513。資格先於 PP、EP／SEP 及進化結算判定（Q2074）。記錄共用數值變更事件的邊界，持續效果重算與到期依 R-0011 判讀。只接受自身、本回合、`direction: up` 的存在條件，`at_least` 可省略或填 1。

`ub: true` 是觸發／起動能力的獨立標記，沿用該能力原有時機、正式目標、付款與本文。能力控制者必須使用作品構築，且作品規則綁定含 `ub_enabled`；職業構築、交叉構築及其他作品下，UB 能力在所有區域無效。同卡未帶 UB 的能力仍有效。適正打出後在既有 `AbilityPlayed` 事實記錄 `UnionBurst` 資格，目標不足、拒付或付款失敗不記錄；本文沒有成功改變狀態仍算一次適正打出。`history`／`history_count` 可查 `event: ub_activated`、`window: this_turn`，以 `who: self | opp` 篩選能力控制者，省略時計算雙方；`exclude: this_execution` 依執行 ID 排除本次正式發動。計次沿用既有 `ub_activated` 回合計數器，包括明示歷史開局；IR 執行紀錄只識別要排除的本次。只計事實次數，不接受主體篩選或 `distinct: subjects`；每回合重置。記錄在正式打出提交時即對本文與巢狀執行可見，並非解決完成後才計次。免原費用發動其他 UB 的 consumer 尚未准入。

`buff.instead` 可在只有 `power` 的強化指示上放一筆 `{if, power}`，以既有條件數值一次選定攻擊力增量；不先加基本值再補差額。`hp` 取代、同時攻體強化、複數取代與 `set_stat.instead` 尚未准入。

`auto.on.ub_activated` 可訂閱「其他己方場上從者發動 UB」：`who: self`，
`holder: { who: self, zone: field, kind: follower, other: true }`。發動點是正式目標與付款成立後、
本文開始前；一般能力、自己、他方或非從者不符合。事件固定本次 execution、能力實例、holder 世代與控制者，
在發動時截取場上觀察者，再沿既有 pending／次數限制流程處理；本文使觀察者離場也不抹除既有待處理能力。
此入口只准上述 holder 形狀，不開放墓場 UB 頭、其他玩家、任意能力使用頭。

入場曲、進化時與謝幕可用 `cost` 宣告頭付款（例如 `cost: {pp: 2}` 或 `cost: {leader_life: 2}`），沿用接受／拒絕與不足資源不發動的付款流程；目標在付款前宣告。`optional_cost` 與模式付款尚不接受 `leader_life`。

普通起動能力與誘發能力頭付款接受 `cost: {banish: {top: {who: self, n: 3}}}`（BP13-015），
或謝幕的固定十張（BP03-073）。張數限正整數常值，從自己的牌庫頂依序表側消滅，不看牌、不選材料。
接受付款後必須付滿；牌庫不足時整份費用不支付，能力不發動，並非本文放逐的盡可能執行。
這個入口不接受 `result`、動態張數、對手牌庫、Quick、追加費用或 `optional_cost` 結果付款。
拒付謝幕費用時也不執行後續復活及棄牌。

能力頭或法術的可選付款可用 `x: {}` 宣告一個 X，付款寫 `pp: x`；同一能力的本文數值、
正式目標或搜尋的費用上限以 `x` 引用同一答案，例如：

```yaml
fanfare:
  x: {}
  cost: {pp: x}
  do:
    - buff: {on: this, power: x, hp: x}
```

X 在接受可選付款之後、選取目標與手牌材料之前決定，付款完成前不扣 PP。
範圍由目前 PP 扣除本次固定費用後計算；一般法術固有費用沿既有報價，能力 PP 不套卡片減費。
`cost.steps` 重複兩次 `pp: x` 會支付兩次同一個 X，宣告上限也按兩份計算，不能各答不同數字。
X=0 合法；拒絕包含 X 的可選付款時不另問 X。`cost_policy: waive_original` 依原規則把 X 設為 0，
不詢問或支付原本費用。錯誤 actor／token／越界答案保留原題；固定費用不足或選不到必要目標則還原整次 play。

`x: {}` 不與 `define.x` 或同名目標綁定共存；`min`、`max`、`from_payment`、X 材料數量與
X 配模式仍維持拒絕。BP15-046 的必要公開兩張陰陽師手牌可與單一 X PP 可選付款並用，
沿共同交易先完成手牌公開、再支付 X PP；其他追加費用與可選本文付款組合不因此開放。

`cost: {lesson: 2}` 選取自己 EX 中規則名稱為「魔法のアイテム」的兩張卡並消滅；起動與誘發能力的 `cost.steps` 可將 PP、Lesson 與一組手牌費用依序組合；所有選擇先完成，整份付款成功才發動能力，後段付不起時前段付款一併回復。Lesson 沿用一般公開材料的付款，不產生專用計數器；能力標頭決定付款時點，正式目標與材料都在付款前選定。此入口只接受固定正整數，不以 token 的印刷名稱或卡號識別材料，也不代替該 token 本身的整卡准入。

EX 起動的 `cost.steps` 可依序組合 PP、`banish_self: true`、`self_to_cemetery: true`（「EXエリアのこれを墓場に置く」，BP12-054）與指定名稱的 `banish` 材料；`act_self` 仍只能由場上的來源支付。從者、法術、護符與各種 token 都可帶 EX 起動能力（BP11-004、BP12-095、BP17-099）。
自身加另外兩張同名卡時，材料使用 `sel.other: true` 排除來源；正式目標與材料都在付款前固定。
材料不足或 PP 不足時整份付款不成立，不先扣 PP 或移動任何卡片；場上來源不能冒用 EX 起動。

`static: {effects: [{ignore_ward: {on: this}}]}` 讓持有者攻擊時略過守護的目標限制（BP10-055、BP04-006、BP09-056 的第二面）。沿用既有 `IgnoreWard` 權限及攻擊合法性檢查；休息、入場當回合、站立目標、對象種類與禁止攻擊仍照常判斷。能力失效或失去能力後不再略過守護。可搭配既有 `static.while` 的公開條件，每次判斷攻擊時重新讀取；只接受印刷場上常駐能力的 `on: this`，不開放對他者授予、私有區或本文 `apply` 安裝。

`keywords: [single_drive]` 與 `keywords: [twin_drive]` 展開為自身攻擊時的完整 Drive check；Twin Drive 先完成第一張的圖示處理與去向，再取第二張牌頂。非作品構築不讀圖示、直接放回牌底；作品構築從權威端既有 card facts 讀取已驗證的 `trigger_icon`，缺資料時拒絕，不從提醒文推算。接受圖示後依一般抽牌、生命、數值與姿態規則處理並送墓，拒絕或無圖示則回牌底；Critical 只增加攻擊力 2，Stand 的領袖攻擊限制持續至本回合結束。

`keywords: [start_amulet]` 只能寫在アミュレット的印刷關鍵字行。作品構築的開局（CR 14.4.3）直接從牌組中已准入卡片的印刷特徵讀取，不經由場上關鍵字；篩選、授予與觸發條件都不能指名它。牌組只能放一種起始護符屬於構築規則，不在對局中檢查。CSD03a／b 卡上的開局說明 section 標 `ignored: reminder`。

`ride: {cost: {pp: N}}` 共用進化相當能力付款交易：從自己的エボルヴ牌庫選一張裏向 DrivePoint、可用一點 EP 代替一點 PP，將該資源移到 Drive 領域並關聯自身。使用憑依會消耗本回合進化相當能力額度，且同一場上物件只能使用一次；不產生進化事件。`gain_drive: {ref: this}` 與 `drive_time: {do: [...]}` 分別執行首次取得及訂閱首次取得事件。首次取得會授予 Drive、Single Drive 與突進；歷史按物件世代保存，失去能力不重置，離場重入才重新計算。離場解除關聯後，DrivePoint 沿既有資源回收規則回エボルヴ牌庫表向。

謝幕中的 `move: {ref: this, to: field}` 可將該次離場後仍在墓場的普通從者放回控制者自己的場上；沿用入場事件及守護姿態選擇，不重新打出卡片或支付印刷打出費用，但會照常觸發入場曲（[官方玩法說明](https://shadowverse-evolve.com/rules/ver_0324/)）。它只追蹤該次離場產生的世代，卡片後續再移區後不會追取新世代；場上已滿五張時不移動。墓場法術打出觸發也可將自身放回場上；其他持有者位置、任意選擇物件的回場、復活入場附帶效果與其他連結家族仍不由此入口准入。

`move: {ref: this, to: field, acted: true, result: returned}` 可讓自身回場時就處於橫置狀態；既有正式目標或結算選取的入場 `move` 也接受 `acted: true`。這是入場初始姿態，入場觀察者讀到橫置狀態，不額外產生 `acted` 事件，也不再詢問守護入場姿態。目前只接受一般 `move.to: field` 的 `acted: true`；`false`、其他目的區、棄牌事件的自身移動及 Advance 牌堆選取後的移動仍拒絕。

謝幕的自身回場依 CR 4.1.6 放到擁有者場上，容量也依該場判定。若能力操作者與回場後的控制者不同，不能檢視對方的私有進化牌堆，因此該次效果進化沒有成功結果（[Q1050](https://shadowverse-evolve.com/cardlist/?cardno=BP07-104)）。其他公開選取者的入場仍沿既有自己的場上入口。

`evolve` 可宣告 `result`，再以 `did` 讀取這次是否真正進化：

```yaml
- move: { ref: this, to: field, acted: true, result: returned }
- if:
    cond: { did: returned }
    then:
      - evolve: { ref: { follow: returned }, result: changed }
      - if:
          cond: { not: { did: changed } }
          then:
            - banish: { follow: returned }
    else:
      - banish: this
```

上述寫法對應 BP07-104：成功回場但未進化時，消滅實際入場的新世代；沒有空位而未回場時，消滅仍在墓場的原世代。若墓場的來源早已另行移區，舊謝幕不追取後來的世代。`banish: {follow: returned}` 目前只接受這種單一自身回場的具名移動結果。進化成功回條不以當前進化狀態代替；有合法資源但選擇不指定、沒有合法資源或來源不適正時都為假。回條不可在生產之前、條件分支之外讀取，也不能重複宣告同名結果。

謝幕中的 `banish: this`（「これは消滅する」，BP18-041、BP14-076，以及 BP12-071 授予的謝幕）同樣追蹤該次離場產生的墓場世代，將它消滅到擁有者的消滅區；卡片已因其他效果移區時不再追取新世代，不消滅任何卡。一般 `left_field` 等未限定送墓的離場觸發仍拒絕 `banish: this`。

墓場法術打出觸發可接正式目標、可選付款與模式，也可執行不操作持有者的普通本文；對 `this` 的操作只准放回控制者場上，不能把墓場持有者當成場上活物件強化、破壞或移 EX。

`entered_from`、`entered_by`、`played_from` 的 `ref: this` 在入場曲與 `auto.on.enter_field: {ref: this}` 讀取自身入場事件。監看他者入場時，可用 `entered_from: {ref: it, from: cemetery}` 讀取該次事件主體的來源；不把能力持有者當成事件主體，其他事件不提供此讀值。進化能力的 `usable_if` 另可用 `entered_from: {ref: this, from: cemetery}` 查詢持有者本次場上世代的入場來源；離場再回場即重新記錄，普通進化不改變來源，進化資格目前限查詢墓場來源。入場事件接受手牌、牌庫及墓場，`not` 可表示非手牌；不由入場方式推斷來源區域。入場來源條件亦可控制唯一的チョイス群組是否出現；`choice.instead` 另接受墓場來源條件以改變可選模式數，仍在宣告時選模式，並沿用同一事件的凍結來源。自身回場及離場後移 EX 需要該次離場確定落在墓場，目前只有謝幕符合；一般 `left_field` 不開放這兩種動作。

法術本文可用 `played_from: {ref: this, from: hand | ex}` 讀取本次適正打出的起始區域，與付款時使用的打出來源共用既有執行資料。自身入場曲也接受 hand／EX 的適正打出來源，效果直接入場不視為打出。先抽牌或移動其他卡不改變這個值；之後將結算中的自身移入 EX，仍受 EX 容量及一般法術清理規則限制。

EX 裡的フォロワー・トークン可由正式目標或本文 `choose` 選取後以 `move: {to: field}` 放上場（BP01-078 的ドラゴン、BP14-094 的招来の狐、BP13-010 的妖精），與 EX 的一般從者共用進場程序與場位檢查；結算時從自己 EX 多選且緊接進場的 `choose`，張數上限在出題時依空位計算，與 Look 的多選進場相同；アミュレット・トークン與其他區域的 token 不在此列。

法術 token 使用一般 `spell` 本體與印刷 PP 費用，從手牌或 EX 區正式打出；印刷 Quick 同樣經由權威產生的 Quick 問題選用。結算沿用法術流程，離開結算區進入墓場後依 token 規則消去，不留在墓場。

`line` 從 1 起；`face` 為 1 起面序或唯一 `front/back`；`section: main` 表示主文，數字 N 表示第 N 個 section。多面未消歧即拒絕。`scope` 區分同一行的能力，跨行能力用 `covers: [1, 2]` 明示覆蓋；每行都須有能力或 `ignored` 宣告。

進化頭可用 `usable_if: {cmp: {left: {count: {who: self, zone: cemetery, orig_cost_eq: 2}}, op: ge, right: 10}}` 表達自己的墓場中原始費用 2 的卡至少 10 張。費用與門檻目前只接受非負整數常量；每次宣告進化時先依當前墓場與印刷費用檢查，再沿用一般進化的公開、付款與連結程序。不足時不公開進化卡、不付款、不消耗進化次數，達標的墓場卡也不會被移動。這個入口不增加其他能力本體讀取原始費用或選取隱藏區域的權限。

同一印刷行若有多個能力，各自指定不同的 `scope`（從 1 起）。例如 BP04-123 同行的 Fanfare 與 Lastword：

```yaml
abilities:
  - line: 1
    scope: 1
    fanfare:
      do:
        - draw: { n: 1 }
  - line: 1
    scope: 2
    lastword:
      do:
        - draw: { n: 1 }
```

兩個能力保留同一來源行，載入時以各自的來源 scope 區分；同一座標的重複能力仍拒絕。

```yaml
ignored:
  - line: 2
    reason: reminder
```

`reason` 只有 `reminder | layout`；不可與能力覆蓋重疊。token 定義不能標成母卡能力；使用卡片層的 `token_sources: [{section: 1, line: 1, card: BP01-T02}]` 明示連到共用 token 卡，跨行定義可用 `covers` 完整覆蓋。引用須指向選用中的完整 token 卡；區段與母卡能力不能重複覆蓋。參數實值為整數或字串，型別必須符合本體宣告；布林不算整數。多個 edition 時必須選版，不自動取最新版。

真正的規則本文若位於尚未分類的 section，可在卡片層明示 `rule_sections: [{section: 1}]`；多面卡須加 `face`（面序或 `front/back`）消歧。只有該 section 每個非空行都由能力的 `line`／`covers` 完整覆蓋時才放行，不能用提醒文字或 token 引用補足能力缺行。此宣告只影響該卡的 direct author 承載，不改來源的 `kind=unknown`，也不取代 DSL lowering 與 runtime 准入。

未明示 `rule_sections` 的未知 section 維持原契約：可用上述 token 引用，或在每個非空行都明示 `ignored: reminder` 時略過；已分類為 token 定義的區段必須使用 token 引用，不能宣告為規則本文。未覆蓋或僅標 layout 仍拒絕。這不授予提醒文字所述共通規則的執行能力。

EP／SEP 卡（種類 `ep`／`sep`）沒有卡片能力：EP、SEP 是玩家持有的數值（CR 3.2），卡文只是開局數值（6.2.1.10、6.2.1.11）、EP 代付 PP 與超進化（12.2.3、12.2.4）的提醒，由引擎原生規則執行，不讀這張卡。寫成 `abilities: []` 加 `admission: light`，每個卡文行標 `ignored: reminder`；卡文空白的卡只寫空能力。

`create: {token: {same_name_as: chosen}, to: field | ex}` 從正式目標 `chosen` 捕捉唯一卡名，沿用同名 token 的代表印刷、整卡准入、容量與進場程序建立一張全新卡。正式目標須為公開區域；不複製目前的數值修正、指示物或附加能力（BP05-083，Q745）。`same_name_as: it` 只接受離場事件主體，名稱取事件離場前的 LKI，即使原 token 已由規則消去仍可建立（BP11-074）。只支援自己場上／EX、單張、無初始數值覆寫；名稱不唯一、查無 token 模板、目的區種類不符或整卡未准入時拒絕。

`auto.on.left_field.sel.orig_cost_eq: N` 接受正整數常量，依離場物件的印刷費用篩選，與 `token: true` 等既有條件取交集；不讀目前費用修正。BP11-074 的 `per_turn: 1` 仍使用各能力持有者的共同回合額度，多個符合物件同時離場時由既有待機選擇決定一次（Q1550–Q1553）；離場原費入口維持原有範圍；入場僅另支援上述己方其他從者的正整數原費相等形狀。

目前 `create: {token: フェアリーウィスプ, n: 2, to: ex, result: made}` 支援固定數量、單一規則名稱、自己 EX；`to: field` 支援從者與護符 token 至自己場上。省略 `n` 為 1；EX 與場上各最多五張，只建立並放入仍有容量的數量。`result` 或 `bind` 擇一，綁定實際放入的 token，滿區時為空；authority receipt 同樣只列出成功放入者。token 的整卡 runtime 程式仍須准入。場上建立共用進場事件與守護的選擇橫置程序；沒有先前區域的建立以 `EntryOrigin::Other` 表示，成功建立的每個 token 各發生一次進場。

Token 建立的數量可使用已准入的公開人口計數，例如 `create: {token: エンシェントアーティファクト, n: {count: {who: opp, zone: field, kind: follower}}, to: field}`。數值在該指示解算時讀取，建立數量仍受目的區容量限制；計數不授予選取私有物件的權限。

EX 的正式目標或結算選取可用 `from: {who: all, zone: ex}`，也可用 `self`／`opp` 限制玩家。`transform: {ref: chosen, into: ドラゴン}` 目前接受已綁定的 EX 選取與單一 token 模板：先消滅仍屬該世代的物件，再依實際成功量於原 EX 玩家處建立新 token；原卡的持有者不會改變新 token 的目的 EX。新物件使用印刷數值與新身分，舊 token 消滅後依共用規則消去。模板仍須整卡准入，建立仍受共用容量規則限制；進場附加效果須另行准入。EX 資源的目標保護沿用規則綁定。

「それはそれぞれ『ロイド』と『ヴィクトリア』に変身する」寫成 `into: {allocate: {among: [ロイド, ヴィクトリア], each_exactly_once: true}}`（BP08-011）。
依 CR 5.17.1 先消滅、再依實際消滅數建立：新 token 放在各原物件控制者的 EX，模板一律由能力的控制者決定。
依回合玩家優先逐一處理各 EX 玩家；該玩家還能從剩餘模板中選出不同組合時，控制者以結算選擇（ResolutionChoice）回答，題目沿用模板組合的 `token-capacity` 形狀，`destination` 是收到 token 的玩家、`available` 是該玩家的成功數（受 EX 容量限制），組合唯一時不發問；零張不建立也不發問。模板不對應原卡，打出時仍須選足目標（Q1060、Q1061）。
`among` 至少兩個不重複的名稱；載入時要求選取的上限不超過模板數，分配決策的 ID 不得與其他決策重複。`total`、`by` 與動態數量不開放。

回合時點自動能力可用 `auto: {zone: ex, on: {phase: {whose: self, which: end}}, do: [...]}`，只在持有卡位於 EX 時訂閱；省略 `zone` 仍為場上。EX 訂閱接受單一區域的 Main／End 時點，以及下述融合事件與融合／打出共用事件；有效區本身不授予從 EX 打出的許可。條件中的 `count: {who: self, zone: field, kind: follower, trait: 魔法使い}` 讀取結算當下符合條件的場上張數；私有區域計數仍不接受卡型或類型篩選。普通法術可在自身結算期間以 `move: {ref: this, to: ex}` 移動自身；EX 已滿則仍走既有結算清理。

條件式墓場打出許可可寫成：

```yaml
play:
  playable_from:
    zone: cemetery
    if:
      cmp:
        left:
          history_count:
            event: played
            window: this_turn
            who: self
        op: ge
        right: 3
```

這是 BP18-007 的基礎面印刷許可，只在來源位於自己的墓場時讀取條件；條件不成立不會禁止原本合法的手牌或 EX 打出。許可沿用一般打出的時點、印刷費用與入場程序。`history_count` 讀取打出前已提交的自己本回合卡片數，不包含正在嘗試打出的這張；能力直接入場與能力起動不計，換回合後重新計數，詳見[事件與歷史形狀](author-history-shapes-1.0.md)。目前接受單一 `cemetery` 加條件的印刷 `play` 頭；無條件、多區列表、其他區域、授予能力，以及同一頭混用費用修改、`define` 或 `cannot_if` 都拒絕。

BP18-007 的 Fanfare 以 `not: {entered_from: {ref: this, from: hand}}` 判斷非手牌入場，先宣告敵方從者正式目標，才結算傷害與自身置底。沒有合法正式目標時，該次 Fanfare 不能打出，其後句也不執行；卡片本身仍可完成入場。

墓場的自身回場訂閱可寫 `auto: {zone: cemetery, on: {played: {who: self, sel: {kind: spell, class: ドラゴン}, during: {turn: self}}}, cost: {pp: 1}, do: [{move: {ref: this, to: field}}]}`。這個例子在自己回合、自己適正打出指定職業法術時擷取當時已在墓場的來源；該法術本文才送墓的來源不補觸發。多張來源各自待機、各自宣告及支付 PP；拒付或不足費用不回場，滿場時付款後沒有成功移動。墓場來源與場上來源共用打出篩選；本文對 `this` 的操作只准自身回場或移到持有者的 EX（墓場的打出與進場訂閱共用這個限制），其他操作、其他目的地、任意目標回場與一般事件欄位讀取維持原邊界。

墓場的卡也可訂閱自己場上的進場，例如 BP07-041 的「自分の場に『蒼の反逆者・テトラ』が出たとき、{コスト1}：自分の墓場のこれをEXエリアに置く」寫 `auto: {zone: cemetery, on: {enter_field: {who: self, sel: {name: 蒼の反逆者・テトラ}}}, cost: {pp: 1}, do: [{move: {ref: this, to: ex}}]}`。進場事件提交時擷取雙方墓場中有此訂閱的卡；本文對 `this` 沿用上述墓場限制，`enter_field: {ref: this}` 在墓場沒有意義而拒絕。本文的自身移動也可放在 `optional` 內表示「…てよい」，例如 BP15-101 寫 `optional: {do: [{move: {ref: this, to: ex, result: moved}}, {if: {cond: {did: moved}, then: [...]}}]}`；只有實際移到 EX 才執行「そうしたなら」的後句。

墓場來源也可使用 `auto: {zone: cemetery, on: {phase: {whose: self, which: end}}}` 訂閱既有 Main／End 時點。事件發生時只擷取宣告有效區內的來源，本文沿用一般條件、可選付款與墓場自身移動；不以場上同名卡代替墓場來源。

墓場入場觀測者的自身消滅費用可寫 `auto: {zone: cemetery, on: {enter_field: {who: self, sel: {name: 侮蔑の絶傑・ガルミーユ}}}, cost: {banish_self: true}, targets: [...], do: [...]}`（BP08-068、Q1098）。目前只接受這個能力頭中的單一自身消滅費用；宣告是否支付、正式目標及原子付款沿用既有能力頭流程。拒付取消該次打出，本文不執行；每個墓場副本只支付自己的來源世代，來源已移區後不能改用新世代或另一張同名卡。這個例外不開放本文的 `banish: this`、打出觀測者、其他有效區或與其他費用組合；入場當下不在墓場的卡不補觸發（Q1081）。

手牌的起動能力可用 `activate: {zone: hand, cost: {steps: [{pp: 1}, {self_to_ex: true}]}}` 表示「{コスト1}手札のこれをEXエリアに置く：…」（BP10-017、BP08-072）。本卡依一般移動規則從手牌放到持有者的 EX 並公開；`self_to_ex` 只接受 `zone: hand` 的非 Quick 起動費用；Quick、其他區域、自動能力的 `pay`、進化與打出費用都拒絕（Quick 候選尚未預檢 EX 空位）。

手牌起動能力可把目前綁定的自身放入 EX；付款、物件身分、世代與擁有者須仍屬本次能力來源。與既有自己的手牌選取結果共用 EX／牌庫的移動授權，不因此允許讀取或搬動其他未選手牌。

手牌自體棄牌成本可用 `{discard: {ref: this, result: paid}}` 命名結果；後續 `move: {ref: {follow: paid}, to: deck, position: {nth: 3}}` 沿用該棄牌回條的新世代放回牌庫。牌庫不足時放到底部，不沿用棄牌前的舊物件身分。
效果本文的 `discard` 若帶 `result`，產生的棄牌回條也可沿用同一個 `follow` 牌庫指定位置移動，追蹤實際成功棄牌後的新世代。

融合用 `activate: {fusion: {sel: {who: self, zone: [hand, ex], trait: 財宝, orig_cost_ge: 1}, n: 1}, do: [...]}`，省略有效區時固定為手牌，也可明寫 `zone: hand`。沿用私有成本選材問題及原子付款：材料排除來源本身，手牌材料捨棄，EX 材料送墓，再把來源放到自己的 EX。EX 滿格時可先用 EX 材料騰出空位；整份付款失敗一併回復。本文的 `this` 精確跟隨該來源移入 EX 後的世代，原有的發動來源仍保留在能力紀錄。

目前融合入口接受固定 `n: 1` 或 `upto: 2`，材料可依卡型、單一特徵、`token: true` 或 `not_token: true` 與原始費用下限篩選；不接受 `any`、其他張數、Quick、額外成本、可選成本或模式宣告。`upto: 2` 的可選材料非空時至少選一張；沒有材料時可選零張，來源仍移入 EX，整個本文略過。這是融合的最低材料規則與零材料例外，依 [CR 1.28.0 §12.18](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf)，不能套用普通 `upto` 可任意選零的規則。

從 EX 打出卡片時，原有計數器跟隨卡片進入結算領域。從者或護符結算入場時，場上的新世代保留同一批計數器，入場能力可以支付這些計數器作為費用。從 EX 直接入場也保留計數器。依 CR 10.6.2.1.3、10.6.2.8.1.1 與 [Q2737](https://shadowverse-evolve.com/cardlist/?cardno=BP21-057)。

`fusion.result: fused` 為該次融合的實付材料命名。本文條件可用 `count: {in: {objects_of: fused, view: before}, kind: follower, trait: 八獄}` 讀符合條件的實付張數（BP19-038）。手牌與 EX 材料皆使用付款前凍結的資訊，每張符合卡計入一次。回條隨完整融合付款提交，讀取範圍限該能力的本文。規則依據為 CR 12.18.4.1。

融合事件用 `auto: {on: {fused: {who: self, material: {trait: 財宝}}}, do: [...]}`，可在場上或 EX 訂閱。一次融合不論材料張數只產生一次事實，使用實際材料付款前凍結的特徵；來源本身的特徵不代替材料。融合與打出共用一條能力時寫 `on: {any_of: [{fused: {who: self, material: {trait: 財宝}}}, {played: {who: self, sel: {trait: 財宝}}}]}`，也可再加自己的 Main 時點。分支保留各自的事件快照時點，所有分支共用同一個能力實例與 `per_turn` 額度；只准這個所需組合，不能任意拼接其他事件或不同材料特徵。

自身作為融合材料可用 `auto.on: {fused_as_material: {ref: this, by: {ability_of: {who: self, kind: follower, trait: 八獄}}}}`（BP19-048）。手牌捨棄與 EX 送墓都能成為實付材料。觸發使用付款前的自身世代，以及發動融合的從者與特徵。融合完成後，每張符合的實付材料各有一次待機能力，沿用同一融合事件的結算時點。

紋章達門檻後消滅與後續找牌仍走共用移動流程。Token 的存活區域採同一份 CR9.1.4 判斷：紋章只可留在 EX，法術 Token 只可留在 EX 或結算領域；移到其他區域後立即消去，再繼續本文，不把 Token 留在除外區。

牌庫送墓用 `moved: {from: {who: opp, zone: deck}, to: cemetery}`，例如 BP05-029 的「自分のターン中、相手のデッキ1枚が墓場に置かれたとき」再加 `during: {turn: self}`。每張從牌庫移到墓場的卡各觸發一次；被移動的卡沒有場上資訊，`who` 必填，`ref`、`sel` 與其他目的地拒絕。這個訂閱只由場上的卡觀察。

`discard: {any: true, who: self, result: discarded}` 讓控制者從自己的現有手牌選任意張數（含零），透過私有選擇問題回答。`count_of: discarded` 讀取實際捨棄成功的張數，可用 `draw: {n: {plus: [{count_of: discarded}, 1]}}` 接續抽牌。效果本文的 `discard: {who: self, n: 1, result: discarded}` 也可讀取成功張數：幼狼奮闘以 `optional` 包住捨棄，再用 `count_of: discarded` 等於 1 決定是否堆墓；空手牌接受時仍是零張，拒絕時不執行分支。單張回條也沿用既有 `count: {in: {objects_of: discarded, view: before}, kind: spell}` 的牌面條件計數，空回條為零；這不是保證支付的一張成本，不能用 `cost_of` 讀原費。這次不開放多張固定數量、隨機、全捨、對手或手牌篩選的結果命名。

效果本文的 `discard: {who: self, n: {x_paid: fairies}}` 讓控制者從自己的現有手牌私下選擇與數值相同的張數（BP22-002「EXエリアの妖精・カードX枚を消滅：X枚引く。自分の手札X枚を捨てる。」）。數值在該指示結算時讀取（之前的指示已生效），這裡是頭付款實際消滅的張數，抽牌與棄牌讀同一個回條；零或負數不棄牌也不發問，手牌不足時捨棄全部。`who: opp` 由對手從自己的手牌選；`random: true` 從整個手牌隨機捨棄該數值張數；`result` 可命名實際捨棄的回條，上限與任意張數捨棄相同視為未定。動態數量降低為 `DiscardMode::ChosenCount`，隨機時降低為讀同一數值的 `DiscardMode::Random`；`n` 與 `any`／`all`／`ref` 是不同的捨棄形狀，不能並用。手牌篩選（`sel`）仍是捨棄的共同缺口，常值與數值張數都尚未支援。

`win: {who: self}` 表示能力控制者在效果處理中勝利，沿用共用勝負判定及禁止檢查。勝利即停止剩餘效果並清除所有待解問題，不先執行下一個檢查時機；因此斯巴達克斯先抽空牌庫、再滿足勝利條件時，勝利先於空牌庫的規則敗北。條件只在本次程序指定處檢查，不因其他待機能力稍後抽牌而重驗（Q1162、Q1480）。此變體不含對手／全體、`result` 或 `precondition`；不提供常駐的禁止勝利／敗北能力。

手牌棄置可用 `auto: {on: {discarded: {who: self, from: hand}}, do: [{pay: {cost: {act_self: true}, do: [{draw: {n: 1}}]}}]}` 訂閱。省略 `who` 時觀察雙方棄牌，也可用 `opp`；場上來源只在場上有效。`ref: this` 則訂閱自身離開前的手牌世代，省略 `zone` 時依該事件使用 Hand，也可明寫 `zone: hand`（BP01-170）；不接受矛盾的 `ref: this`／`who: opp`，也不沿移區後的新世代發動。只由實際成功的捨棄事實觸發，普通手牌移動或消滅不算。每張卡各產生一次待機；同時捨棄兩張時，天威的兩次待機都保留，但首個在結算時支付橫置自身後，第二個不能再支付，因而只抽一張（Q2150）。場上觀察者可加 `during: {turn: self}`，在捕捉時以當前回合玩家判定，不把捨棄者或能力因果當成回合玩家；自身手牌訂閱與 `during` 的組合仍拒絕。

墓場中的 `auto` 可觀察 `on.discarded`，沿用玩家、回合與每回合限制。冒號後的解決時付款寫在本文 `pay`；拒付不移動來源，付款成功後可用既有自體回場指示。

「從自己的手牌捨棄或消滅這張卡」使用既有的 `any_of`，例如 BP14-074：

```yaml
auto:
  zone: hand
  on:
    any_of:
      - discarded: { ref: this, from: hand, who: self }
      - banished: { ref: this, from: hand, who: self }
  do:
    - optional:
        do: [{ move: { ref: this, to: ex } }]
```

這個子集只有上述兩種互斥事件，仍是一條作者能力；限次、批次彙整、其他事件組合與巢狀授予均拒絕。
捨棄包含規則手牌上限（Q1922、Q1930）；消滅必須是成功的消滅指令，普通移區不算。
`move this to ex` 只跟隨該次成功移動的新世代；EX 已滿或卡片已再次移區時略過。
BP14-081 的同一事件頭可接正式目標與一般本文，其法術主效果仍完整寫在原有 section 座標。

被捨的卡可用 `sel: {kind, class}` 篩選（至少一項），以離手前的手牌特徵判定，例如 BP09-006 的 `sel: {class: エルフ, kind: spell}`；不接受其他 `sel` 欄位，也不與 `ref: this` 或 `by` 並用。

「自己回合、自己捨棄一張以上」另用 `auto.aggregation: batch` 搭配 `on.discarded: {who: self, during: {turn: self}}`（BP12-052／053）。單一捨棄指令中的多張成功棄牌，只讓每個來源能力待機一次；分開的兩次捨棄仍各觸發，去重先於每回合額度。
規則手牌上限的一次完整選取也屬同一批；抽牌後再次超限的另一次捨棄則是新批（[Q1666–1668](https://shadowverse-evolve.com/cardlist/?cardno=BP12-SL14&faq=)）。
此聚合只收上述場上、自身回合、自身捨棄的頭，不收手牌自身、其他事件、對手或省略 `who` 的組合；聚合後不得捕捉或讀取單張事件欄位，也不能用事件物件作目標來源。
此入口仍不收 `sel`、`by`、其他來源區或一般手牌觀察者，也不開放棄牌歷史查詢。

自己墓場的卡可放在 `targets`，例如 `from: {who: self, zone: cemetery, kind: follower, trait: 竜族}`；共用正式目標程序在付款前固定候選與宣告，沒有必需目標時不能付款。結算本文用 `move: {ref: returned, to: hand}` 回收。不能用本文的 `choose` 代替正式目標，否則會把本次付款才送墓的卡納入候選。這個目標入口不擴充回場的種類或容量選擇能力。

`move.ref` 可直接指定公開墓場集合，例如 `{who: self, zone: cemetery, name: 決意の予言者・ルーニィ}` 並移到 `ex`；不必先作任意張選擇，移動與 EX 容量處理由既有移動流程完成。 `move.ref` 的公開場上、EX 與墓場查詢沿用一般所有者區域移動和 EX 容量處理；`other: true` 排除能力持有者同世代。具名 `result` 與一般物件來源共用同一目的區權限，公開查詢不能繞過回條驗證。

`shuffle: {who: self}` 洗自己的整副牌組，可直接接 Look；空牌組或單張仍視為執行了整副洗牌。

整副手牌可用 `move: {ref: {who: self, zone: hand}, to: deck, result: returned}`，
緊接 `shuffle: {who: self}`。`zone: [hand, cemetery]` 將兩區合為一筆原子移動。
區域須為完整、無篩選的手牌或墓場，`count_of: returned` 從移動收據計算成功到達牌庫的張數。
移動沿用共同的世代、衍生物消去及事件捕捉程序，洗牌清除舊位置知識。
單獨指示的 `who` 使用 `self`；`each_player` 的階段內可使用該階段的玩家綁定。

墓場選定的卡回牌組後洗牌，必須相鄰寫成：

```yaml
- move: {ref: returned, to: deck}
- shuffle: {who: self}
```

上述範例中，`returned` 是自己的墓場選取；`ref: {who: self, zone: cemetery}` 則取自己的墓場全部。
這個精確 ref 形狀只用於回牌組洗牌配對，不開放其他 move 的墓場全體操作。
自身 Lastword 也可用 `ref: this` 跟隨該次實際進墓的新世代，再接洗牌。
不指定位置與順序的回牌組只能出現在這個配對內，所有成員須由自己擁有。
零張仍洗牌；不得用任意順序題近似。

`targets[].from.zone: [cemetery, banish]` 表示自己的墓場與消滅區之聯集（BP14-077）。
沿用同一個目標張數與名稱篩選，裏面消滅牌不列入候選；其他玩家或混入其他區域均不接受。
選定聯集後同樣相鄰寫 `move: {ref: selected, to: deck}` 與 `shuffle: {who: self}`，
回牌庫時只搬動仍是原世代的選定牌，不重新查詢另一區域的新世代。選零張仍洗整副牌組，
後續翻回裏面的指示獨立執行（Q1925）。此入口不授權未選取的消滅區全體回牌庫。

「餘牌洗牌後置底」使用既有的 `move.order: shuffle`，例如：

```yaml
- move:
    ref: {in: seen, except: {in: {objects_of: taken, view: before}}}
    to: deck
    position: bottom
    order: shuffle
```

這個子集只接受本次自己 Look 的集合與成功 Before 差集；不接受其他位置、結果綁定或牌組外的物件。
它不詢問順序，也不算整副洗牌。原 Look 紀錄保留原樣；洗過的成員不再有可重用的位置證據。

自己的消滅領域是公開區域，`targets` 的 `from` 可寫 `{who: self, zone: banish, ...}`（CP03-049、CP03-057、BP18-039）。被選中的卡沿用一般移動規則，可回手牌或入場；入場仍需寫 `result`。只接受自己的消滅領域，`who: opp`／`all` 與省略 `who` 都在載入時拒絕；明示裏向的物件不列入公開候選。

Look 中所選子集可用 `move: {ref: kept, to: deck, position: top, order: chooser, result: placed}` 排回牌庫頂，餘牌寫 `{in: seen, except: {in: {objects_of: placed, view: before}}}`，再排到牌庫底或送墓（CP01-070、CP02-081）。這個 Order 收據只供餘牌差集扣除，不能當移動輸入或追蹤對象。牌庫頂直接移到 EX 的 `move: {top: {who: self, n: 1}, to: ex, result: placed}` 可接 `apply: {on: {follow: placed}, effect: {cost_mod: {when_playing: {in: affected}, delta: -2}}}`；省略 `until` 時減費綁定該物件世代持續（BP01-084）。`delta` 與非負整數 `set` 二擇一：`set: 0` 把該卡的打出費用設為 0（CP01-035「このターン、それをプレイする際、コストを0にする」、BP02-091 不限本回合），沿用同一個移動回條與費用狀態，只套用到實際移到 EX 的新世代；正數 `delta`、負數 `set` 或與 `next_only` 並用仍拒絕。

`apply.effect.cost_mod` 的 `when_playing: {in: affected}` 也可安裝在已宣告的正式目標上，例如 `on: card` 搭配 `set: 0`、`until: turn`。與成功移入 EX 的 `follow` 共用物件世代及費用修正流程；沒有選取物件時不建立修正。

牌庫頂送墓（含對手牌庫）也可帶 `result`，但只供卡片條件讀取，不能當移動輸入或追蹤對象。離開牌庫的卡沒有修正，條件讀其印刷資訊：`cmp: {left: {count: {in: {objects_of: milled, view: before}, not_class: ウィッチ}}, op: ge, right: 1}` 表示「{ウィッチ}でないカードを墓場に置いたなら」（BP17-045）；`kind`、`trait`、`class`、`not_class`、`name`、`name_contains`、`orig_cost_ge` 可組合，多種卡擇一用 `any` 連接多個條件（ECP02-040 依キュート／クール／パッション分別判斷）。看牌後選取再移動的收據（`move: {ref: picked, to: hand, result: taken}`）同樣可讀，表示「〜を公開したなら」（CP02-015）。只接受「至少一張符合／沒有符合」的比較；從場上或其他公開區移動的收據可能帶有場上修正，仍拒絕。

公開場上物件移至 EX 的 `move` 可用 `result: moved`，後續 `if: {cond: {did: moved}, then: [...]}` 只在至少一個成員實際移動成功時執行；選零或目的 EX 已滿不算成功，搬兩張也只執行一次後句。這個子集只接受已綁定的 Move 收據名稱，不接受其他結果種類、過濾條件或前向／分支外引用；一般公開卡與本次已觀察卡移至 EX 時，部分容量另詢問精確剩餘格數；未放入者留在原區且不算成功。

動態數量另接受本次 `destroy: {ref: ..., result: broken}` 的成功回條：`n: {count: {in: {objects_of: broken, view: before}, as: victim, where: {cmp: {left: {player: {master_of: victim}}, op: eq, right: {player: p}}}}}`。只計本次實際從場上被破壞者，按離場前控制者區分，包含隨後消去的 token；破壞失敗不計數，不按目前區域或擁有者重新計算。

`each_player: {as: p, stages: [[{create: {token: ..., n: ..., to: field, by: p}}]]}` 支援單一、無選擇的 token 建立階段，雙方在同一批次建立並進場；守護進場選擇按回合玩家優先處理，在所有選擇完成後一次提交。建立與計數須指向同一玩家；各目的區沿用容量上限與守護進場流程，守護由該 token 的控制者選擇。建立階段不接受其他回條、after 計數、owner 比較或結果匯出。另接受單一非選擇階段 `each_player: {as: p, stages: [[{banish: {top: {who: p, n: 10}}}]]}`：先取得所有玩家各自牌庫頂的實有張數，再以同一批移動消滅；牌庫不足不視為抽牌失敗。此牌庫頂消滅入口接受正整數張數與該階段的玩家綁定。整個能力完成後才進入衍生觸發能力的處理。

非選擇的多階段程序可在 `stages` 中依印刷順序列出完整手牌／墓場回牌庫與洗牌，以及固定 0 至 10,000 張的抽牌。所有 `who` 使用同一個 `as` 玩家綁定。每個指示先跨雙方，再進入下一指示與階段（CR 1.3.4.1）；回牌庫先取得雙方完整集合，再一次提交，抽牌則每張先改變雙方手牌，再捕捉任一方該張抽牌的誘發。牌庫不足與抽牌禁止沿用一般抽牌規則。

```yaml
- each_player:
    as: p
    stages:
      - - move: {ref: {who: p, zone: [hand, cemetery]}, to: deck}
        - shuffle: {who: p}
      - - draw: {who: p, n: 5}
```

`banish: {ref: {any_of: [{who: all, zone: field}, {who: all, zone: ex}]}}` 將所有玩家的場與 EX 作為同一集合、一批消滅（BP13-107、[Q1794](https://shadowverse-evolve.com/cardlist/?cardno=BP13-107&faq=)）；不以先場後 EX 兩次動作替代。這個入口只接受上述兩種公開區的完整聯集，不接受私有區或額外篩選。

效果本文可用 `choose: {bind: selected, from: {who: self, zone: hand}, n: 1}` 在結算中選取自己的手牌；可加既有種類、職業、特徵及費用篩選，或以 `upto: 1` 表示可選零張。候選只提供給操作者，以本次問題的 handle 回答，不公開其他手牌，也不能放進正式 `targets`。選取綁定可接 `banish`、`reveal`、移到 EX／牌庫指定位置或一般從者／護符入場；這些移動不產生棄牌事件。公開只持續至本次結算結束；移到公開區域者依共用移動規則顯示。移到 EX 的成功回條可沿用 `follow` 安裝該物件的打出費用修正。一般多張移動、入場容量與同批順序仍依共用准入邊界，不因手牌入口額外放寬。

手牌作費用的 `discard.sel` 支援 `class`、`orig_cost_eq/ge/le`、`traits_all` 與基本關鍵字 `keyword`（依印刷能力判斷，例如 BP22-095「手札の【守護】を持つフォロワー1枚を捨てる」寫 `keyword: ward`），可與種類、單一特徵、名稱交集。`traits_all` 要求同時具有所有列出的非空特徵名稱；例如 BP20-042 必須捨棄同時具有「絶傑」及「魔法使い」的其他手牌咒術，只有任一特徵或從者都不能支付。篩選讀取印刷職業與原始費用，不受目前費用修正影響。觸發能力的頭費用沿用宣告、正式目標、私有材料選擇、付款的順序；拒付不執行本文，材料不足不提交付款。

BP22-068 的下一次規則起身限制使用下列 `apply`；`who` 的目前支援子集為 `self/opp`，兩處須一致，期限只接受下一個 start 的 end。安裝綁定所選物件世代與指定玩家，經實際回合排程等到該玩家下次開始階段的規則起身，隨後失效。效果造成的 `stand` 不受阻止，來源離場也不提早終止；目標離場後的新世代不沿用限制。

規則起身限制的 `apply` 也共用已驗證的公開場上篩選；原本的起身時點與物件世代限制不變。

```yaml
apply:
  on: enemy
  effect:
    cannot:
      what: stand
      on: affected
      during: { phase: start, who: opp, cause: rule }
  until:
    next_phase: { who: opp, phase: start, edge: end }
```

持有者永久不能在自己的開始階段依規則起身時，寫 `static.effects: [{cannot: {what: stand, on: this, during: {phase: start, who: self, cause: rule}}}]`（CP02-025、ECP01-053）。宣告只在持有者仍在場上且保有此能力時有效，沒有回合期限；每次自己的開始階段都重新讀取。它不阻止效果的 `stand`，因此 Main 階段的印刷付費能力仍可使其起身；沿用頭費用的拒付流程，拒付時保持橫置。此入口不接受 `affected`、對手開始階段或 `static.while`；有期限的施加仍使用上方既有 `apply`。

印刷的自身移動免疫使用 `static.effects`：`cannot: {what: destroy, on: this, by: ability}` 阻止能力直接破壞，`cannot: {what: banish, on: this}` 阻止消滅；`on` 省略也指自身。每次處理依持有者當下有效卡面及能力重讀，失去能力後不再保護；不另安裝有期限的狀態。禁止的指示沒有成功移動回條，也不產生離場或破壞事件。能力傷害或交戰傷害造成體力歸零仍會規則破壞，直接送墓不等於破壞（Q802、Q2263、Q2836）。本入口只接受無條件的自身宣告，不接受其他接受者、`static.while`、施加式免疫、未限定原因的 `destroy` 或 `banish:by:ability`。同一段印刷能力同時包含自身能力防傷時，須保留為一個 `static` 宣告；能力防傷可用靜態專用的 `if: {cond, then: [{prevent: {kind: ability}}]}` 控制，無條件的 `cannot` 放在同層。此形式只收自身能力防傷與自身移動免疫，不接受 `else`、巢狀分支或其他持續效果。

私有看牌後，`choose.from: {in: seen, kind: follower, trait: かげろう, except: {in: seen, name: ドラゴンナイト アリフ}}` 可排除同一批已觀察牌中的單一字面名字，並保留其餘種類、特徵條件。它與排除持有者同名的 `not_same_name_as_this` 不同；此例的持有者是進化後異名卡。`except` 的觀察綁定必須相同、名字非空；不開放公開目標、手牌選卡、搜尋、聯集或分組選卡，也不接受和 `not_same_name_as_this` 合用。未選牌仍沿既有 Look receipt 的成功移動差集與排序流程處理。

移動結果的數值後續可寫 `agg: {of: {in: {follow: placed}}, attr: orig_cost, fn: sum}`（BP13-050）或 `agg: {of: {in: {follow: entered}}, attr: hp, fn: sum}`（BP20-066／PR-548）。生產者限自己的牌庫頂端移動，以及私有 Look 選牌後的移動；`orig_cost` 只讀實際到達 EX 的成員，`hp` 只讀實際入場從者的當前有效體力，包含已生效的數值修正及傷害。數值本身須是這個 `agg`，消費端限 `life.delta` 或同量傷害的 `damage.n`；不開放其他算術包裝、費用／攻擊力、min／max、before、搜尋或建立 token 的結果，也不接受額外集合篩選。

這個讀值沿同次效果明示的移動收據跟隨新世代，逐一檢查要求與實際目的地及目前物件區域；物件換代或離區會拒絕，不改讀新世代或最後已知值。沒有實際成功成員時總和為 0；滿 EX、滿場或放棄入場不把未移動的候選算進去，後續步驟仍正常處理。

本回合打出歷史可用 `sel: {name: 卡名}`，並與既有的 `kind`、`trait`、`token` 篩選相交。
名稱在正常打出提交時保存，包含當時有效的別名；離場、換區或失去別名能力後，歷史仍保留原值。
例如 `play.cannot_if: {history: {event: played, who: self, window: this_turn, sel: {name: テラーナイト}, at_least: 1}}`
只檢查先前已提交的打出，不會把目前正在檢查合法性的卡算入。
失敗的打出不新增歷史；缺少已保存名稱的外部歷史仍依既有的完整性邊界拒絕查詢。

## 傷害能力持有者

`damage: {to: this, n: 1}` 以能力持有者作為傷害收件者（CSD02b-003），沿用一般傷害、資訊來源及物件世代檢查。來源引用直接進入本文，不需要正式目標宣告，控制者與來源身分沿用原能力；致命傷害也沿用一般規則檢查。搭配公開條件可表達「自己場上沒有其他クール從者時對自身造成傷害」，能力持有者本身由既有 `other: true` 排除。

## 同一目標集合的傷害角色

`targets[].roles` 支援一個 `{if_nonempty: 1}` 角色與一個 `remaining` 角色；
對應 BP14-071 及其 JP 再版。角色名稱只在作者 DSL 中使用，mapping 的鍵序不影響選取順序。
`damage.to: {role: {of: enemies, name: primary}}` 引用該目標的角色綁定；
兩個角色使用同一次正式目標宣告，不能改成兩個獨立目標或以陣列第一個成員代選主目標。
目前只准入場上的公開正式目標與角色引用，不接受私有選卡、收件者聯集角色或結算時 `choose.roles`。

已宣告正式目標的角色也可作姿態指示的對象，例如 `act: {role: {of: enemies, name: rest}}`。它與傷害角色共用同一份已選取集合，不再詢問第二次選取；`n: 2` 搭配一體 primary 與 remaining 可表示兩個相異從者的不同指示。

權威端的 `awaiting.domain` 在既有 `kind: targets` 題目增加 `roles` 陣列，
每項為 `{role: <題內整數>, cardinality: {if_nonempty: 1}}` 或
`{role: <題內整數>, cardinality: {remaining: true}}`；沒有角色的題目回傳空陣列。
角色 ID 與目標 handle 都只在該 decision token 內有效，不公開 IR binding 或永久物件 ID。

角色題目透過 `Answer::TargetsWithRoles {targets, roles}` 一次作答；每個 `TargetRoleAssignment`
含 `role: Nat` 與 `targets: Vec<u64>`。JSON 消費端保存同形狀的
`{targets: [handle, ...], roles: [{role: id, targets: [handle, ...]}, ...]}`；
scenario runner 的 AI 決定比對將選取與各角色成員當集合，仍區分不同角色的分配。
沒有角色的題目仍使用 `Answer::Targets`，角色題目不可省略分配；未開放角色的題目也不可附上角色答案。

答案必須恰好包含每個已公告角色一次、涵蓋所有已選目標且互不重複，不能使用未選或跨題 handle。
選零體時兩個角色都空；選一至三體時主角色恰好一體，其餘全部屬於 `remaining`。
驗證失敗不提交任何選取或傷害，原題仍可重答；結算時沿用正式目標的世代與合法性重查。

## 寫入與驗證

```bash
uv --directory /absolute/project/carddb run sve-carddb author write \
  --inputs /absolute/work/public.json --main /absolute/work/main.yaml \
  --bodies /absolute/work/bodies.yaml --output-root /absolute/project/authored \
  --card SYN-01 --remove-prototype
uv --directory /absolute/project/carddb run sve-carddb author check \
  --inputs /absolute/work/public.json --output /absolute/work/check.json
```

`--card` 可重複，或用 `--all` 選全部輸入卡。只有替換既有原型時帶 `--remove-prototype`；不再要求 `--base`、`--baseline-root` 或 `--storage anchors`。工具先驗整份候選、索引閉包與原型互斥，再寫主檔／本體及索引；I/O 中斷回報已完成檔案，由作者修復 Git diff。

`check` 檢查全部索引作者卡；`body_usage` 依永久 CardId 去重，達三張提示整理共用本體，無額外資格 consumer。review 紀錄只報實際 verdict，不以 SHA freshness 授予核可或阻擋。source pending 不擋直接入口，真正 disputed／revise 只擋受影響卡。

移除既有原型後，若歷史共享情境仍使用該卡，依[舊 DSL 回歸](v0-regression.md#已遷移到-dsl-10-的卡)原樣保存各舊原型印刷條目。歷史 fixture 只供回歸測試，不填回 current 選用，也不作為准入證據。

check 成功代表來源定位、行覆蓋與 DSL 結構通過；Rust lowering／runtime 能力准入及對局行為測試仍須通過。錯誤回傳位置及分類，不印整行卡文。完整回歸交 CI，無需額外證據 JSON 或結果 DB。

打出限制的 `play.cannot_if` 保留 `in_zone: {ref: this, zone: ex}` 與 `my_turn: true`，並可使用已准入的公開條件及其布林組合。例如 BP12-111 以 `cmp` 檢查自己場上的ゴブリン從者數不大於 2；一般打出與 Quick 候選都在扣費、移區或選擇前依當前狀態檢查，拒絕時不更動狀態。條件不能依賴稍後的選擇、結算捕捉或隱藏區域的卡片特徵。這些限制不阻止能力直接移動或將卡置入場，也不提供額外打出區域的許可。

`play.cannot_if` 可用 `{not: {in_zone: {ref: this, zone: hand}}}` 表達只能從手牌打出。來源區域依目前世代檢查；其他效果授予消滅區打出許可時，仍須遵守卡片本身的禁止條件。

起動能力的 `usable_if` 與 `per_turn` 共用既有宣告前條件及每實例每回合次數限制；PP 為 0 也會消耗成功宣告的次數。BP16-021 的墓場計數可用 `class: ロイヤル` 與 `kind: follower` 限定自己的公開素材，ECP02-003 則用場上的 `trait: クール`；條件不符時不扣費、不開啟目標選擇，也不消耗次數。

效果本文的 `evolve: {ref: this}` 可在結算時從控制者未公開的進化牌堆選一張同名普通進化卡，沿既有進化程序處理，並產生真正的進化時事件；不另扣 PP／EP／SEP，也不占用一般進化次數。`evolve: {ref: this, into: 勝利の化身 アリフ}` 可明示單一目的形名稱，候選必須符合該名稱，不退回同名比對；省略 `into` 維持同名。選取與執行前都以同一個受驗目的地重查資源。亦可用 `evolve: {ref: chosen}` 讓同一效果先前選出的單一自己場上從者進化；`targets` 或 `choose` 須明示 `who: self`、`zone: field`、`kind: follower`，上限為 1。`search` 單一從者到 `field` 的具名 `result`，以及自身回場或單一己方墓場正式從者目標移入己方場上的 `move` 具名 `result`，可用 `evolve: {ref: {follow: entered}}` 引用實際入場者；沒有成功入場者就跳過進化選擇。跟隨入場結果的收件者依自己的名字找同名進化卡，即使有合法卡也可選擇不進化（Q457、Q2111），不使用收件者印刷進化能力的費用或資格。來源先送墓仍可繼續處理實際入場者。自身效果進化也可選擇不指定進化卡（[Q1989](https://shadowverse-evolve.com/cardlist/?cardno=BP15-021)、CR 4.1.2.2）；固有進化能力的公開付款選擇不適用這項私有區域處理。`into` 只限 `ref: this`。其他玩家、其他區域或卡種、多張收件者、名稱清單、名稱包含篩選、`precondition` 及超進化仍拒絕；`gain_drive` 不接受 `into`。沒有合法進化卡時照既有結算選擇契約回答空集合，已支付成本不回退。`entered_from: {ref: this, from: hand}` 讀取這次入場事件原有區域，可用 `not` 表示非手牌；不等同能力入場。場上 `count` 保留既有種類／trait 篩選；本批滿場判斷使用不帶篩選的場上卡數（包含護符）。

正式目標綁定的單一公開物件執行 `move: {ref: target, to: field}` 時，若自己的場已滿，該物件留在原區域並產生沒有 `after` 的失敗移動回條；已支付費用不回退，後續指示繼續。這與自身回場、事件追隨回場的既有容量處理共用路徑。多個仍可移動的物件爭用剩餘場位時，在結算中另選精確剩餘格數；正式目標不因此重選，已支付費用也不退回。

公開區域的數值讀取可用 `distinct_names` 計算不同卡名數，或用 `distinct_orig_costs` 計算不同原始費用種類數。兩者與 `count` 共用公開區域選取、種類／職業／類型篩選及費用條件讀值；原始費用取印刷資料，不受物件當前費用影響，同名或同費用的多張卡只計一次；沒有費用資訊的卡不另算作一種 0 費。可用於能力本體條件、動態數值與打出費用修正；`at: resolution` 延續墓場結算起始讀取，不開放隱藏區域或 `other`。

傷害的 `to: {any_of: [{leader: opp}, {who: opp, zone: field, kind: follower}]}` 可把領袖與從者放進同一次傷害的受傷者集合。它沿用既有 `RecipientSet::Union` 與去重規則，只有一個傷害動作及數量回條；傷害替代條件對同批所有受傷者生效。

進化 PP 費用修改可用 `apply: {on: this, effect: {evolve_cost_mod: {on: affected, set: 0}}, until: turn}`，或以 `delta: -1` 取代 `set`；兩者擇一，設定值不得為負。省略期限時持續至該物件世代結束，也可選既有公開場上對象。修改共用費用效果狀態，作用於實際規則進化的 PP 費用，最後以 0 為下限；不改卡片的打出費用，也不修改已離場後重新入場的世代。

正式 `choice.options` 的模式可用單一 `pay: {cost: {pp: 1}, do: [...]}` 包住本文；這是 R-0006 的可選追加費用，包含既有固定 PP、固定張數的自己手牌棄牌、公開 EX／場上材料，以及場上來源自身送墓的有序組合。先選模式，再決定各選中模式付不付；正式目標仍須在付款前合法選定，拒付不免除目標要求。拒付略過該模式的整段付款本文，也不詢問材料；未選模式不問付款、不選材料、不執行效果。

模式本文的付款可包含傷害分配：先選模式與是否付款，再宣告該模式的正式目標與分配數值，最後選擇素材並付款。土之秘術等材料選擇沿用既有成本參數，不占用傷害分配的目標綁定。
分配依受者綁定尋找對應的正式目標組；最上層有多組目標時，也只對該組分配。模式本文仍不接受巢狀 `choice`，包含付款本文內的巢狀模式。

選中模式的費用沿既有付款程序，與固有卡片／能力費用一起完整提交，成功後才依印刷順序執行本文；付款失敗整次打出回復。卡片減費只修改固有 PP，不修改模式費用。手牌材料問答沿既有私有成本入口，對手無法查看候選；公開材料也使用自己的成本綁定，不把目標權限套在材料上。Quick 的模式追加費用可拒付，因此預檢仍驗固有費用與至少一組合法正式目標；接受費用後才收集並驗證材料，不因此開放必要材料費用的 Quick 起動。能力頭的公開材料費用可與沒有各自費用的模式並用，起動與入場曲等誘發能力相同（CP02-020「墓場の元のコスト1のカード2枚を消滅：下記から2つまでチョイスする」）。多個帶選取材料的模式同時選中、或模式材料與卡片／能力本身的材料費用並用時，所有材料集合（含手牌棄置的可棄集合）必須互斥才准入：區域或玩家不同（例如手牌對 EX、墓場對 EX），或同一讀值下的職業矛盾（`class` 對 `not_class`、或要求不同職業），任何一張卡都不可能同時符合兩組。卡種依印刷基底判斷：從者（含進化從者）、法術、護符、紋章彼此互斥（同一讀值下），但從者與進化從者是包含關係，視為可能重疊。可能重疊的組合（同區同篩選、手牌對手牌等）仍拒絕，因為各材料問題取自同一快照，不預留其他組已選的卡。可變 X、結算中的 `either` 付款、根付款與模式付款並用仍未准入。

`choice.upto` 可寫 `{count: {who: self, zone: field, trait: メジロ家}}`（ECP01-048）。上限在正式模式宣告時讀取自己的目前場上卡片數，包含符合類型的從者與護符；由既有程序限制在印刷選項數以內、排除沒有適正目標的選項、禁止重複選擇，正式目標先於回復與抽牌決定。零張時略過該模式群組。有可選模式時沿用既有 `upto` 的至少一項限制。此入口只支援自己的場、單一 `trait` 與 `upto`；不支援算式、隱藏區域、其他玩家、`n` 或 `instead` 混用。

印刷 `evolve.cost` 與起動能力共用 `Cost` 語法，例如 `{leader_life: 3}`、`{discard: {n: 1, sel: {who: self, zone: hand}}}`、`{banish: {n: 2, sel: {who: self, zone: cemetery}}}`。同名進化卡、進化次數、PP／EP／SEP 與超進化時點仍由原規則程序檢查，費用成功後才揭露及安裝進化卡。規則動作的材料使用既有 `costs` 輸入容器，公開材料為 `costs.materials`、手牌丟棄為 `costs.discard`，值為物件 ID 陣列；每項仍經正式材料候選與嚴格數量驗證，不能以目標選取代替付款。

本文、使用前提及常在條件可用 `my_turn: true` 檢查目前是否為能力控制者的回合；回合序數沿用 `turn_number: {who: self}`，讀取實際開始的玩家回合數。本文的 `in_zone: {ref: this, zone: ex}` 在該指示結算時，以來源原世代與控制者目前 EX 的交集判斷；來源離開 EX 或移區後再回來都不符合。`in_zone` 與其他 Condition 共用語法；此 EX 來源讀值的對局與世代驗證以本文為範圍，其他入口仍須通過各自的執行准入。它不授予新的有效區；打出限制與付款來源區域仍沿各自既有語義。

`auto.per_turn` 與 `activate.per_turn` 接受正安全整數 N，額度按能力實例與每個實際回合分開；觸發在進入待機時消耗，起動在付款成功且適正打出時消耗。`activate.usable_if` 與 `per_turn` 可並用，例如墓場キュート卡至少五張才可每回合起動一次；資格不符、拒付與付款失敗都不消耗起動額度。沒有上限時省略 `per_turn`，不能以零表示無限次。

打出時宣告增加 PP 使用 `play.optional_cost: {pp: 4, result: enhanced}`；`pp` 是正整數，不能同時指定材料、`one_of`、`optional_cost_delta` 或 `optional_cost_set`。接受宣告後，增加量與其他費用增減一起報價，最後才套用零下限，整次打出只支付一次 PP。拒絕宣告不加費；本文可用 `paid: {receipt: enhanced}` 讀取已完成的宣告，即使折扣使總費用成為 0。宣告在模式與目標之前，所有輸入完成後才付款；付不起則還原整次出牌。這與能力本文的可選付款不同。
同一法術的模式數可用 `choice.instead: [{if: {paid: {receipt: boosted}}, upto: 2}]` 讀這個宣告（BP21-026「コストを+2したなら、代わりに2つまで」）。模式在付款前選，此時讀接受宣告的答案；付款失敗會還原整次出牌，所以與付款完成的結果一致。

`play.optional_cost.act` 可選擇橫置自己場上符合篩選的固定張數卡片，配 `optional_cost_delta` 交換減費。
例如魔術指南使用 `{sel: {who: self, zone: field, kind: follower, trait: 学院}, n: 2}` 與 `optional_cost_delta: -2`；
温かな抱擁使用同一付款流程，以 `name: ナテラの大樹` 選兩張護符。
成本未指定玩家的「場の」依 CR 10.4.3／Q806 指自己控制的場，不能橫置對手卡；已橫置或不符條件的卡也不是材料。
接受時須選足兩張，拒付可支付原 PP 費用。魔術指南的己方從者正式目標仍須合法存在，否則無法打出，也不抽牌（Q2734）。

手牌棄牌素材的 `sel` 可用 `not_class` 排除印刷職業，並與卡種、名稱、原費等既有篩選取交集；素材選擇仍只向付款者公開。

`play.optional_cost.discard` 支援固定張數的自己手牌，配單一常值 `optional_cost_delta` 或 `optional_cost_set` 交換減費。
它與場上送墓／橫置、墓場消滅使用同一付款流程：先決定是否交換，再宣告效果目標，最後私下選足額手牌；原 PP 與棄牌都成功才打出。
正在打出的卡不列為手牌材料，拒付不棄牌且支付原費用，材料或 PP 不足則整次回復。Quick、可變張數及混合材料操作仍拒絕。
例如 ECP01-036 的 `discard: {sel: {who: self, zone: hand, trait: ウマ娘}, n: 1, result: fuel}`，
同次普通法術本文可用 `cmp: {left: {count: {in: {objects_of: fuel, view: before}, orig_cost_ge: 7}}, op: ge, right: 1}` 判斷已棄牌的原始印刷費用；拒付時該集合為空。
這項回條不注入之後的 Fanfare；Follower 的交換只影響打出付款，其入場能力仍使用獨立的待機與結算。

交換材料的捨棄、送墓與橫置使用共同事件入口，付款中只捕捉待機，完整付款與本文結束後才選序。手牌棄牌不觸發場上離場訂閱；場上送墓會捕捉實際離場與謝幕，材料已離場的來源資訊仍保留。付款失敗時待機與額度一起回復。即時讀目前 PP 的常駐條件或目標選擇器仍拒絕，以免付款先後改變捕捉；一般本文的 PP 讀值與 PP 上限、EP、SEP 條件不因此拒絕。

傷害集合亦接受正式目標名稱，例如 `to: {any_of: [enemy, {leader_of: enemy}]}`。同一個傷害指令同時處理被選從者與其控制者的領袖（BP19-035、BP19-111）；`leader_of` 沿用選取時保存的控制者，不能改用 owner。缺少必要的從者目標時整個能力取消，不會只對領袖造成傷害。

## クレスト公開計數

`count: {who: self, zone: ex, kind: crest}` 沿用公開區域計數，僅計入控制者 EX 區的クレスト；
普通 EX 卡與對手的クレスト不計入。可作為數值、比較條件或 `usable_if` 的運算元。
`crest` 僅擴充計數種類；搜尋、目標與歷史種類查詢仍維持原有範圍。
此入口不改變クレスト的建立、同名限制或 EX 區能力規則（綜合規則 9.1.4–5、10.3.6）。

## Advance 起動與具名入場

`activate.advance: true` 表示印刷的 `{アドバンス起動}`，限自己場上、主階段的一般起動時機。
印刷 PP 成本可將其中 1 PP 改用 1 EP；與普通進化、食事、ライド共用每回合一次的進化相當額度。
成本先支付；即使拒絕後續入場，也不退還 PP、EP、自我消滅成本或額度（CR 12.16.3、8.3.2.1）。
成本接受 PP 與後續有序來源成本；使用條件仍寫在 `usable_if`。
一般 `activate` 從エボルヴ牌庫入場不因此成為進化相當能力，也不能以 EP 代付。

《Advance 起動》的 PP 可替代付款後，後續有序來源成本沿用一般付款流程，例如 `steps: [{pp: 3}, {self_to_cemetery: true}]`，或先橫置自身再移墓。先付完成本才進行具名 Advance 選取；拒絕入場仍不退回已付成本。

本文中的具名 Advance 入場使用相鄰的選取與移動：

```yaml
- choose:
    bind: incoming
    from:
      who: self
      zone: evolve_deck
      kind: follower
      advance_follower: true
      name: 楽園の開闢・タケツミ
    n: 1
    may: true
- move:
    ref: incoming
    to: field
```

`name` 也可換成 `name_contains`（卡名包含該片段，BP10-063「カード名に『デュアルモード』を含むアドバンスフォロワー」），兩者擇一。這個選取只向控制者顯示符合印刷名稱的 Advance 從者，允許不選；同名エボルヴ從者與對手的資源不符合。
此選取只接受裏向卡；[綜合規則 1.28](https://shadowverse-evolve.com/wordpress/wp-content/uploads/2026/09/28121513/ShadowverseEVOLVE_cr_1.28.0_260928.pdf) 4.6.3 將表向卡排除於一般進化牌組參照之外，9.2.2 的表向返回不會使卡片重新符合選取。移動使用新的物件世代及既有入場程序；滿場時不入場。
相鄰移動也可寫 `to: ex`，把選到的 Advance 放進自己的 EX（BP10-109「エボルヴデッキの『大いなる《世界》・ゼルガネイア』1枚をEXエリアに置いてよい」），沿用同一個選取與一般 EX 移動；EX 已滿時不移動。選取綁定只供這個相鄰移動消費，不可逃逸到其他步驟、改送手牌／墓場等其他區域或作為後續 `follow` 的結果。
一般目標／私有選擇沒有開放エボルヴ牌庫。

指定アドバンス法術的正常費用打出（BP13-036）沿用相同的私有選取，改為緊接 `play_card`：

```yaml
- choose:
    bind: blast
    from:
      who: self
      zone: evolve_deck
      kind: spell
      advance_spell: true
      name: イクシードブラスト
    n: 1
    may: true
- play_card:
    ref: blast
```

此入口只接受完整名稱、自己持有與控制的裏向アドバンス法術、`n: 1`、`may: true`，以及相鄰且省略 `cost` 的打出。
它支付法術通常成本及必要追加成本（Q1744）；PP 不足或放棄選取時，母能力已付的成本仍保留。
法術沿既有巢狀 execution 解決，再依 9.2.2 表向返回進化牌組。選取綁定不離開這一對步驟；
名稱片段、費用覆寫、改送場上／EX、一般墓場正常費用打出都不由此入口准入。

## 表向資源計數與條件模式上限

起動能力可用 `if: {cond: {combo: N}, then: [{choice: ...}]}` 包住唯一的模式群組（BP04-007）。模式及其正式目標仍在宣告時選定；Combo 不足時不詢問模式或目標，但條件外的 `activate.cost` 仍完整支付，不能改成 `usable_if`。`targets[].option` 只為選中的模式宣告目標；起動能力本身不增加本回合已打出卡片數。Quick 預檢同樣略過尚未成立的群組目標，成立時照常檢查是否有適正目標。既有覺醒及入場來源條件也沿用同一個群組流程；入場來源仍須通過既有的入場事件／進化語境檢查。不增加新的 DSL 欄位。

`choice.instead[].if.cmp` 可使用自己的エボルヴ牌庫中，指定名稱且表向的卡片張數，
以 `ge` 比較正整數門檻。例如 CP03-125 的條件為：

```yaml
cmp:
  left:
    count:
      who: self
      zone: evolve_deck
      name: ドライブポイント
      face_up: true
  op: ge
  right: 3
```

配合 `choice.n: 1` 與該替代項的 `upto: 3`，宣告模式時讀取公開張數並保存本次上限；
選項仍不重複，選中的模式依印刷順序結算，正式目標在效果結算前宣告。
張數沿用公開計數與既有條件式模式流程，不使用另一套執行器。
エボルヴ牌庫的計數不接受對手、裏向卡、未指定名稱或額外篩選條件。

`instead[].if.cmp` 可讀任一玩家場上、EX、墓場的公開篩選計數，以及消滅區不帶篩選的張數（目前未開放有篩選的消滅區計數，見[型別化 IR 驗證](typed-ir-validation.md)），並與常數比較，例如 BP10-052 的「自分の墓場のチェス・カードが5枚以上なら、代わりに2つまで」寫 `{cmp: {left: {count: {who: self, zone: cemetery, trait: チェス}}, op: ge, right: 5}}`。`instead[].if.history` 接受與 `if.cond.history` 相同的本回合歷史，例如 BP16-074 的「このターン、自分の場のフォロワーが進化していたなら」寫 `{history: {event: evolved, window: this_turn, who: self}}`。`instead[].if.leader_life` 接受任一玩家主戰者體力與常數的單一比較（`eq`／`le`／`ge` 擇一），例如 CP04-091 的「自分のリーダーの体力が10以下なら、代わりに2つまで」寫 `{leader_life: {who: self, le: 10}}`；PP、EP 等其他玩家資源仍不開放。三者都在宣告模式時求值並固定本次上限；手牌、牌庫等隱藏區域的計數與任意算式仍拒絕。

## 手牌選牌的公開動態費用上限

單一手牌 `choose.from` 可用 `orig_cost_le: {resource: {who: self, kind: pp}}` 讀取選牌時自己的剩餘 PP，並以 `not_same_name_as_this: true` 排除能力持有者同名卡（BP21-093）。上限沿用公開數值運算，在出題時依目前狀態固定；卡片或能力成本已支付時讀取付款後餘額，最大 PP 不代替目前 PP。候選及答案仍只向選牌者公開，後續 `move: {ref: picked, to: field, result: entered}` 沿用私有手牌入場及容量處理，無須支付被選從者的打出費用。手牌聯集的各分支仍沿用既有靜態篩選。

## 公開數值宣告與查詢上限

「自分のリーダーか自分の場の（他の／特徴）フォロワー1体を選ぶ。それは{体力}+N する」寫成恰好一個（`n: 1`）的接受者目標，再以 `by_kind: {ref, as?, leader, follower}` 依種類分支：`leader` 內 `item`（或 `as` 名稱）是該領袖的玩家，可寫 `life: {who: item, ...}`；`follower` 內是該從者，可寫 `buff: {on: item, ...}`；另一分支不執行（BP21-101、CP04-041、ECP01-057）。目標可寫 `leader_or: true`，或 `any_of: [{leader: self}, {who: self, zone: field, kind: follower, other?, trait?}]`（兩支同一玩家、領袖支不帶其他篩選時視為同一目標）。從者在分支執行前離場則不作用。`item` 只在分支內有效，巢狀時須以 `as` 命名；`upto` 或多張接受者目標沒有單一種類，不能用 by_kind。

正式 `targets.n`／`upto` 及 `targets.from`、`search.sel` 的 `cost_le`／`orig_cost_le` 可使用既有 Number：自己的目前 PP 與 PP 最大值、自己場上／EX／墓場的公開計數與同一範圍的 `distinct_names` 卡名種類數，以及這些值的 `plus`／`minus`／`times`／`per`／`min` 與 `define` 別名。計數接受現有常量種類、職業、特徵、精確卡名與原費篩選，並補上常量 `orig_cost_ge`；不授予 `distinct_orig_costs`、隱藏手牌／牌庫、其他玩家、回條或時點覆寫讀值；來源自己的數值讀取見下段。公開上限不得與私下 Look 選取或 extremum 人口混用。

正式目標的動態張數及既有動態費用上限可讀來源自己的指示物數量與來源資訊數值（攻擊力、體力及費用），例如 `upto: {counter: {on: this, name: 魔力カウンター}}`，或 `from.orig_cost_le: {power_of: this}`。讀值與候選無關，沿用原本的公開選取、張數及印刷原費檢查。

`search.sel.orig_cost_eq` 同樣接受 Number，以等於比較候選的印刷原費。`orig_cost_of: <正式目標>` 讀至多一體、只從場上或 EX 選取的正式目標的印刷原費（BP12-114「元のコストX…Xは選んだフォロワーの元のコスト」、BP13-053「元のコスト+2」寫 `plus`）；原費是印刷值，目標在讀取前離場（例如同一本文先破壞）仍讀得到。載入時確認該 binding 來自公開、至多一張、候選全在場上或 EX 的選取，私下選取（例如對手手牌）的原費不能當公開數值；同一模式的後一個目標可讀前一個目標的原費，模式可否選取以綁定前題後的可行答案判斷。其他區域、多張目標與回條成員的原費仍不支援。

目標張數在準備正式問題時求值，負數依數量規則以零處理；精確張數同時固定上下限，`upto` 的下限保持零。宣告型正式目標讀取宣告當下的目前 PP，可能早於本次付款；結算型手牌 `choose` 則讀取該指示出題時的餘額。結果存入既有選擇問題，不在付款或結算時重新讀取。搜尋的數值篩選則在搜尋指示開始、發現私有候選前固定，保留有其他條件時可找不到的規則；例如 BP04-098 用 `PP最大值 - 1` 表示嚴格較低的原費，BP07-091 用自己的 EX 卡數作上限。

正式 `choice.upto` 另接受 `per: {of: {count: ...}, unit: <正整數>}` 的公開自身計數，依完整單位向下取整（BP22-074 每十張原費二的墓場卡）。零組略過，結果仍限制在印刷模式數內，沿用不可重複及適正目標規則；精確 `choice.n`、替換上限與任意數量算式沒有因此開放。

自己的持續進化減費可用 `static.effects: [{evolve_cost_mod: {on: this, delta: {minus: [0, {count: {who: self, zone: field, kind: follower, other: true}}]}}}]`（BP06-019）。每次報價讀取當前有效能力與場上集合，與既有進化費用效果合計後才以零為下限；不安裝永久鎖或保存過時數量。既有 `apply.evolve_cost_mod` 仍限 `on: affected` 與常量修正。

模式內的單一公開目標亦可供 `leader_of` 保存選取時的控制者（BP18-046）。捕捉須屬於該目標自己的宣告、exact 1、角色為 controller，且目標選擇器本身保持獨立；其他角色及任意捕捉仍拒絕。

一般進化頭的 `evolve.into` 接受卡名、非空卡名清單，以及
`{who: self, zone: evolve_deck, kind: evolve_follower, name_contains: レーヴァテインドラゴン}`。
省略時沿用同名進化。每個頭的目的地、費用與 `usable_if` 分別檢查；雙面進化卡按回答的面序比對該面名稱，再沿共同程序公開、付款並進化。
不符合名稱、面序或該頭條件時，在公開與付款前拒絕。此入口不擴充效果本文的進化主體或目的地範圍。

目的地選取的名稱包含必須同時符合進化從者卡種；同名片段的進化護符或進化法術不符合。
本入口比對進化候選所選面的印刷卡名；候選的改名或額外卡名能力仍須另行准入。

`pilot: {ref: this}` 依 CR5.32 將場上的來源護符在本回合變成印刷攻體的從者；缺任一印刷值或目前已是從者時不處理。保留衍生物身分與原有能力，當回合受傷和數值修改不繼承至下次操縱；授予的光環等能力依自己的期限保留（Q1581～Q1583）。印刷護符帶攻體及疾走，須整卡同面含自己的 pilot 指示才可准入。作者不重寫攻體或期限，也不提供另一個操縱者物件；橫置從者的費用仍寫在一般 cost.act，依 CR10.4.3 使用自己的從者。

同名建立若卡文明定「token 從者」，須寫 `token: {same_name_as: chosen, kind: follower}`。依 CR9.1.2.3／1.3.2 推論，名稱解析出的印刷模板不是從者 token 時不建立任何物件，指示與合法法術仍正常結算；不把來源上暫時的操縱效果複製到新物件。只寫「token 卡片」則省略 kind。

## 場上與 EX 的 token 送墓材料

BP12-074 的場上起動能力以同一組候選選取自己場上或 EX 的魔界 token：

```yaml
cost:
  steps:
    - act_self: true
    - to_cemetery:
        sel: { who: self, zone: [field, ex], trait: 魔界, token: true }
        n: 1
```

兩區書寫順序可交換；只接受固定一張、`token: true`、非空 `trait`，不帶 `kind` 或其他篩選。
特徵符合的 token 護符也可支付，不能把「token」縮限為從者（Q1685）。正式敵方目標先選，
再從兩個公開區選材，自身橫置與送墓共用既有原子付款；不新增另一套移區或費用狀態。
場上送墓會誘發謝幕，EX 送墓不會（Q1684）；token 在抵達墓場後依 CR9.1.4.4 消去，
不因此取消已完成的付款。其他材料移區、張數、結果命名及額外篩選不由此入口放寬。

## 場上能力的公開手牌成本

`cost: {reveal: {sel: {who: self, zone: hand, kind: follower, trait: 学院}, n: 1}}`
可用於場上起動能力及誘發能力的必要頭付款，固定公開一或兩張自己的手牌。
`cost.steps` 可與 PP、自身橫置依序組合；正式目標先選定，材料選擇只向控制者出題，
全部付款成立後才發動。材料不足、拒付或後段付款失敗會取消，之前的資源、姿態與公開資訊一併回復。

沿用法術必要追加費用的同一選材、付款與公開流程；材料保留在手牌，不產生棄牌事實。
付款前重驗張數、篩選、世代及所屬手牌，只公開已選材料；移區或本次解決結束後依既有流程隱藏私有位置。
篩選沿用卡種、類型、職業、卡名與原始費用條件，場上能力公開成本另接受非空 `name_contains`。
兩張同卡名的不同實體可以各選一次。

此場上能力入口不接受 `upto`、`any`、分組、隨機、他方手牌、材料結果命名、模式內付款或 Quick；
不擴大其他手牌成本的准入。法術必要追加費用與公開後置底仍依既有封閉形狀處理。

## 場上能力的單張手牌移區成本

`cost: {to_ex: {sel: {who: self, zone: hand, trait: 宴楽}, n: 1}}`
將自己一張符合條件的手牌作為成本移到自己 EX；`to_deck_bottom` 使用相同選材形狀，改放自己牌庫底。
例如 BP14-118 支付後才計算 EX 宴樂卡張數，BP14-008 置底後回復領袖體力，
BP17-071 將自身橫置與手牌到 EX 放在同一個 `cost.steps`。

「これを{アクト}デッキの下に置く」寫成 `cost.steps: [{act_self: true}, {to_deck_bottom: {sel: {in: this}, n: 1}}]`（BP07-012）。
`in: this` 指付款的來源自身，降低為與自身送墓相同的來源移動，改放持有者牌庫底；正式目標先選，可選來源自身，
但來源移走後成為新物件，本文對它的指示不再生效。橫置仍只能由場上的來源支付，已橫置時整份付款不成立。
`in` 其他綁定名稱仍只用於法術追加費用中前一步公開的手牌（BP17-054）。

場上起動能力與誘發能力頭付款沿用私下的材料問答，正式目標仍先選。
付款不額外公開手牌、不算棄牌；EX 到達後公開，置底保持隱藏。EX 已滿不能支付，
全部成本成功後才執行本文。只收自己手牌、固定一張、無 `result`，篩選沿既有手牌材料查詢；
拒絕 `upto`、`any`、多組材料、模式組合、Quick 及法術追加費用。
成本材料的私有綁定不提供給本文讀取。

## 選取對手墓場卡到自己的 EX

單張對手墓場的正式目標可接 `move: {ref: selected, to: ex}`（BP05-019 的進化時能力）。
目的地是能力控制者的 EX；卡片擁有者不變，EX 已滿時卡片留在原墓場。
沿用既有公開目標、世代重驗與移區流程；其他目的地及多張對手墓場選取仍拒絕。
自己墓場或場上卡移到 EX 仍回擁有者區域。
放在自己 EX 的對手卡可沿通常程序打出並套用自己 EX 的打出費用修正；法術解決後回原擁有者墓場。
從者與護符由打出者控制並進入其場上，之後離場時回原擁有者的區域。
此入口仍拒絕打出他方控制的 EX 卡及外來擁有者的手牌，不改打出時點或必要追加成本。

## 效果中打出墓場卡片

此節限定墓場入口；省略 `cost` 的指定アドバンス法術打出見[Advance 起動與具名入場](#advance-起動與具名入場)。

`play_card: {ref: selected, cost: 0}` 以既有單張墓場目標的綁定正式打出卡片；目標可屬自己或對手。
此受驗子集要求明示 `cost: 0`，降為 `SetBase(0)`，不略過必要追加成本或合法目標；省略費用仍拒絕。
卡片保留原擁有者，內層打出與效果由母效果控制者控制，Spell 解決後依既有移區流程回原擁有者墓場。
正式打出保留真實的原區域與其玩家，不以控制者代替原墓場玩家。

內層有獨立 execution、目標與付款，完成後接續母本文；它建立的入場曲等待處理能力不插入母本文。
失去目標世代、沒有合法內層目標、付不起或無空場位時，不產生內層打出事實；母能力已付的費用及
先前本文仍保留。錯誤 actor／token 維持原問題。本入口只接受單張墓場綁定及常值零覆寫，不接受
隱藏區域、エボルヴデッキ或任意費用；未支援形狀在 lowering／准入拒絕。

### 效果中打出指定能力

`play_ability: {of: this, trigger: evolve_time}` 選擇來源目前有效的進化時能力，依通常程序宣告、
支付其原始成本並解決；`cost_policy: normal` 可省略。這項授權不造成實際進化，也不製造進化事件。
拒付或付不起內層必要成本時，接續母能力，保留母能力已付的費用與先前效果。

`play_ability: {of: friend, ability: {ub: true, n: 1}, cost_policy: waive_original}` 要求 `friend`
是單張己方場上從者的正式目標綁定，從它目前有效的 UB 能力中選擇一個，包含仍有效的裝備授予能力。
選項使用此次問題的 handle，回答時重驗能力、持有者世代、控制者與稱號；能力失效或沒有
可用能力時不打出內層能力。`normal` 仍檢查起動次數；免原費用的 UB 授權依 Q2488 不以已滿的起動次數阻擋候選、回答或提交。
本專案採 [R-0016](../../authored/rules/rulings/R-0016.yaml) 的 `strength: inferred` 解讀：效果發動仍計入同一能力的起動使用歷史，
因此先被效果發動後，通常發動會受到該次數影響；這個反向順序尚無直接官方回答。全體 UB 歷史及事件照常記錄，助理模式須顯示本專案解讀。
`auto.per_turn` 的誘發次數與這裡的起動使用次數是不同的消耗點，沒有改變。

`waive_original` 省略內層能力的原始頭成本與原始成本材料選擇，原始成本的任意 X 為零；
原始頭的 `paid` 分支依免成本授權繼續。必要追加成本與本文 `pay` 仍循共同付款流程處理。
這不是將原始成本改成 PP 0，也不改寫儲存的能力定義；之後通常使用同一能力仍須支付原始成本。

內層沿用正式打出、獨立 execution、事件與續體流程。UB 在正式打出時即計入歷史，
`exclude: this_execution` 只排除查詢能力自己的執行，仍能看見已打出的母能力及同一定義先前的執行。
由此產生的等待能力在母本文完成後處理。此受驗子集不接受其他觸發種類、任意能力數量或費用政策。

場上印刷常在的 `no_trigger` 現支援 `who: opp` 與 `kinds` 的 `fanfare`／`evolve_time`：
依能力控制者禁止在事件發生時進待機，包含護符入場曲；不禁止效果直接打出該能力。
此入口不含施加式授予、條件式、其他區域或其他誘發關鍵字。

場上送墓事件的 `sel` 可加上 `where.history`，將離場前的從者世代與本回合實際受傷紀錄連結：

```yaml
on:
  moved:
    from: {who: opp, zone: field}
    to: cemetery
    during: {turn: self}
    sel:
      kind: follower
      where:
        history:
          event: dealt_damage
          window: this_turn
          who: self
          sel: {trait: ウマ娘}
          where: {recipient: item}
```

此入口只接受自己回合中，對手場上從者送墓的事件；傷害來源限自己具有指定特徵的卡片（BP22-R22），或以 `where: {ref: this, recipient: item}` 指定能力持有者自身、並省略該歷史查詢的 `who` 與 `sel`（BP20-025）。來源的控制者及特徵取實際傷害成立時的資訊；目標以物件與世代比對，零傷害不留下紀錄。它在離場事件成立時判斷，後續移動不改已待機的結果；回手再入場的新世代不繼承，下一回合清除歷史。沿用既有傷害事實、離場訂閱、待機及每回合上限，不建立第二條事件路徑。其他來源篩選、任意歷史視窗與事件頭仍拒絕。

### 多張移動的容量選擇

公開卡或本次已觀察卡一次移至自己的場上／EX、或公開卡移至各自擁有者的 EX 時，沿用共同移動計畫逐張檢查世代、取代與原有准入邊界，再依實際目的玩家計算剩餘空位（CR 4.4.4.2、4.8.3.2）。足夠時全批移動；已滿時不移動；正數空位不足時由效果控制者在結算中選精確剩餘格數。這個容量選擇不是重新宣告正式目標，不重新套用正式目標保護，也不改變已支付的費用。

容量題沿用 `InputDomain::SearchPartition`／`Answer::SearchPartition` 的問題內 handle 與精確基數。只列仍會實際爭用空位的物件；舊世代、留在同區與被取代而留場者不占空位。公開候選可附 `object` 對照既有公開物件，讓相同印刷但狀態不同的卡可被分辨；手牌、牌庫觀察等私有候選不提供物件 ID。未觀察的牌庫片段不因此取得顯示候選的權限。

所有容量選擇及成功入場者的守護姿態都完成後，才以原子批次提交移動。未被選中的原候選仍保留沒有 `after` 的失敗回條，`follow` 與 `objects_of: {view: before}` 只跟隨實際成功者；Look 的未放入者仍依既有 remainder 契約走後續指示。這支援 BP22-077 的墓場復活、BP07-071 的正式墓場目標進 EX，以及 BP22-110 的看牌進 EX，不改變各入口原有的卡型、來源與目的地限制。

## 追加回合

`extra_turn: {who: self}` 在這回合之後插入能力控制者的一個追加回合，沿用共用回合排程。它不立即切換回合玩家、刷新 PP 或進行抽牌；這些動作在該回合真正開始時執行。後來建立的追加回合先進行，原先應進行的普通回合保留；追加回合中可以再次追加（Q193）。目前只開放這個單次、自己、下一回合之前的寫法，其他玩家、`skip_turn`、張數、`result`、`precondition` 與自訂 anchor 仍拒絕。

次元の超越的打出時選付費用沿用 `play.optional_cost`：`banish: {sel: {who: self, zone: cemetery, kind: spell}, n: 10}` 配 `optional_cost_set: 7`。只有十張材料實際付款完成才設定原費用，之後再套一般費用修正；打出的那張仍不在墓場，不能拿它作材料（Q192、Q329）。

開闢の予言者使用 `play.optional_cost.banish: {sel: {who: self, zone: cemetery, orig_cost_ge: 1, orig_cost_le: 10}, n: 10, distinct_by: orig_cost}` 搭配 `optional_cost_delta: -10`。篩選與互異限制共同要求墓場的原費 1 到 10 各一張，沒有印刷原費的卡不算材料。這個互異限制限單一墓場消滅交換的固定張數材料。

## 能力種類篩選

看牌選取、正式目標與公開集合的 `keyword`，以及 `search.sel.keyword`，接受
`earth_rite`（土之秘術）與 `lastword`（謝幕）。沿用既有 `HasAbility` 查詢，
讀取候選目前卡面的完整能力定義，不以卡文包含關鍵字判斷。
土之秘術依能力的付款步驟判定，包含可選付款與模式；Stack 宣告不等於土之秘術。
墓場、牌組與看牌區仍能按持有的能力篩選，無須該能力可在當地發動或當下付得起費用。
場上的篩選使用有效能力，包含授予、失去能力與目前進化卡面的變化。

## Drive 事件觀察

場上印刷自動能力可訂閱 `drive_checked: {holder: {who: self, zone: field, kind: follower}}`，
每次完成的 Drive 檢查各產生一次待機，包括沒有圖示或拒絕執行 trigger 的檢查；
空牌庫沒有檢查事件。本文的 `it` 是發起該次檢查的從者，保留當時世代，不是翻出的卡或觀察者。
`drive_trigger: {who: self}` 只在 trigger 效果實際執行時待機；可加 `during: {turn: self}`
及能力的 `per_turn: 1`，限次沿用共同觸發准入。此事件沒有物件 `it`。

Twin Drive 在每次事件時捕捉觀察者，兩次檢查與 trigger 效果都完成後才於共同檢查時點解算待機
（checked：CP03 的 Q1403、Q1404；trigger：Q1418、Q1481）。正式目標仍在打出待機能力時選擇，目標不足則整個能力不打出
（CP03-112，Q1439）；模式也必須有合法目標，CP03-107 沒有消滅目標時只可選抽牌（Q1587）。這批只開放上述自己場上的觀察形狀，不開放任意事件篩選或歷史查詢。

## 原位公開牌庫頂

公開頂牌觀察在選出的從者／護符實際入場後，可把觀察集合扣除該次移動 before 成員的餘集移入手牌。沿用觀察回條與成功移動回條的共同作用域；因場上容量未能移動的成員仍屬餘集，不以原先選取清單替代成功結果。

`reveal: {top: {who: self, n: 1}, bind: shown}` 在能力本文頂層公開牌庫頂，不移動卡片。
它沿用同一批 Look Before 成員，後續不重新查詢最新頂牌。條件、模式及付款分支內的此入口仍拒絕。

公開後，`agg: {of: {in: shown}, attr: cost, fn: sum}` 讀該批卡片的成本總和；
`attr: orig_cost` 讀原本成本。`count: {in: {objects_of: shown, view: before}, kind: follower}`
可依種類、名稱、類型或職業檢查是否有符合成員。只看過而未公開的 Look 不取得此權限。

`move: {ref: {in: shown}, to: hand}` 可將同批公開成員移至手牌，`to: ex` 移至 EX；
`to: field` 必須同時寫 `result: moved`，沿用共同入場與容量處理。後續對成功移動者的減費等效果
仍使用 `follow: moved`（BP08-074），不能讓來源舊世代取代成功到達者。

整批公開成員可用 `move: {ref: {in: shown}, to: deck, position: bottom, order: shuffle}`
洗至牌庫底。沿用同區定位與子集洗牌，牌庫其餘成員維持原本順序，洗後隱藏該批成員的位置。
`distinct_names: {in: {objects_of: shown, view: before}}` 仍可計算該批公開卡的規則名稱種類數，
同名的不同印刷只計一次。輪廻の統治者・ゼラエル（BP19-113、BP19-P31）以此檢查是否公開九個
不同名稱，牌庫不足九張時仍處理已公開的卡，條件不成立（[Q2614](https://shadowverse-evolve.com/cardlist/?cardno=BP19-113)）。

空牌庫不曾公開任何卡；若卡文依「公開卡是奇數／偶數」或「不是某類卡」分支，
先以不帶篩選的 `count` ≥ 1 確認存在卡，再判斷屬性（CP01-083、BP21-066）。
總和為零不等於公開過零費卡。此入口不支援從公開集合選取成員後處理差集餘牌。

## 解決中的法術來源回手

法術本文可用 `move: {ref: this, to: hand}` 讓當次仍在解決區、世代相符的法術來源自身回手。
它沿用原有移動與收尾，回手後不再自動送墓；`this` 的舊引用不會繼續操作新手牌世代。
此入口不接受其他解決區卡片。BP05-034 先看四張、可選一張盗賊加入手牌並公開、將餘牌排至牌庫底，
再依成功選取者的移動前名稱判斷是否把法術來源回手；不選或選到其他名稱仍正常送墓。

法術本文的 `banish: {ref: this}` 可消滅當次仍在解決區且世代相符的法術來源。
移動沿用共用消滅事件與來源離場處理，之後按印刷順序完成剩餘本文。
收尾依 CR 10.6.2.8.3 只將仍留在解決區的卡送墓；已消滅的來源保留在消滅區，
後續 `this` 保留舊世代。運命への反逆（BP12-044、BP12-SL13）先消滅自身，
再讓雙方將完整手牌與墓場合併回牌庫、洗牌與各抽五張，最後給能力控制者本回合下一張卡減七費。
此玩家限定依 Q1657，減費共用既有 `next_only` 付款與期限規則。

## 場上能力的單張手牌消滅成本

`cost: {banish: {sel: {who: self, zone: hand, trait: ペイルムーン}, n: 1}}`
可用於場上起動能力及誘發能力的頭成本；沒有類型限制時省略 trait。
材料題目只向控制者公開，接受付款後必須精確選一張合格手牌，經 Banish 產生消滅事件與歷史。
付款前重驗原區域、所屬、世代與篩選；材料不足或付款失敗回復整次打出，拒付不執行本文。
原公開區域的消滅材料與手牌材料各自保留來源證明。

BP18-118 在固定選一項的 `choice.options` 中，模式 3 使用：

```yaml
- pay:
    cost:
      banish:
        sel: { who: self, zone: hand }
        n: 1
    do:
      - draw: { n: 1 }
```

這是 R-0006 的打出時可選追加費用：先宣告模式與正式目標，再決定付不付，拒付就不問材料。
目前只准入場上能力的固定選一項；手牌消滅模式只帶一個材料參數與唯一成本步驟。既有跨模式材料重疊限制不變，公開或移區手牌的模式費用仍不准入。
手牌為空仍可選該模式後拒付；接受卻付不起則取消打出。CP03-046 的頭費用若拒付，整個入場能力沒有打出（R-0001）。
不接受複數張數、upto、any、分組、結果命名、對手手牌、法術追加費用或 Quick。

## 本回合安裝的事件能力

`delayed` 要求 `until: turn`。指定目標離場的 `on` 如下，並要求 `next_only: false`（可省略 false）：

```yaml
delayed:
  on:
    moved:
      ref: enemy
      from: field
      to: cemetery
  until: turn
  next_only: false
  do:
    - draw: {who: self, n: 1}
```

`enemy` 必須是本能力已正式選取的一個公開場上從者；監看的是選取時的物件世代。
callback 僅支援固定抽 1、己方回血 2、固定 token 1 張到 EX；
不接受內層 targets、optional_cost、define、前置條件、巢狀 delayed、其他事件或期限。

每次安裝獨立，來源之後離場仍有效。目標回手、消滅、回場的新世代不符合場進墓條件；
本次傷害導致的破壞會在本文安裝完成後，於共同檢查時點發生。例：BP15-001／008／012。

`delayed.on` 也可寫 `enter_field: {who, sel, entered_by?}`，觀察安裝後本回合其他卡進入場上；
本文的 `it` 是這次入場的卡，本文走一般卡片本文的准入，不限上面的固定 callback。
`entered_by: play` 只收打出入場（BP03-089「次に自分がプレイしたフォロワーが場に出たとき」）。
`next_only: true` 表示本回合只誘發一次：同一批同時入場的多張符合時，沿用既有的每回合次數選擇，
由控制者選其中一張誘發，對應「次に自分の場にフォロワーが1体以上出たとき、その中の1体は…」（SP01-026）。
選擇在誘發進入待機時決定，不延到結算；這是本專案解讀（`strength: inferred`）。
進場型本文也可寫 `evolve: {ref: it}`，沿用效果進化的選牌（BP22-037）。

`delayed.on` 也可寫 `life_changed: {who, direction, crosses_zero?}`，與 `auto.on.life_changed` 相同的篩選，
觀察安裝後本回合每一次實際的生命變化，例如「このターン、自分のリーダーの体力が減少したとき」；本文同樣走一般卡片本文的准入。

## 場上勝敗禁止與生命跨零事件

場上持續的 `cannot: {what: lose, who: self}` 與 `cannot: {what: win, who: opp}`
沿用玩家權限 IR，於勝利效果與敗北規則檢查時讀取當前有效能力；離場或失去能力即失效，投降不受限制。
`auto.on.life_changed` 可加 `crosses_zero: true`，只接收生命從正值變成零以下的實際事件；
不因零以下再減少而重複誘發。待機能力沿用回合玩家優先順序，設定生命產生的增加仍可觸發其他生命能力。
此支援不擴充一般 `state` 誘發、條件式或 EX 區勝敗禁止及安裝型玩家權限。

## 本回合從自身消滅區打出

`player_effect` 的 `who:self`、`until:turn` 配合
`effect.play_permission: {from: [banish], on: {who: self, zone: banish}}`，
沿既有 `InstallEffect` 與 `Permission(PlayCard)` 准入。許可涵蓋本回合後續進入
自身消滅區的卡；來源離場不會提早結束，到實際回合結束即失效。
其他玩家、來源區、物件安裝與永久期間仍不准入。這個封閉入口不擴充通用 Zone。

這是正常打出：仍檢查所有者、控制者、主階段或 Quick 時機、正式目標、印刷費用
及原有禁止。無指定來源區的打出費用修正也適用；明示 Hand／EX 的修正維持原範圍。
Quick 只讓本來具有 Quick 的法術成為候選，不授予消滅區起動能力。
法術先從消滅區進入 Resolution，結算後去所有者墓地（官方 Q2376）。

## EX 區的チョイス數量許可

```yaml
static:
  zone: ex
  effects:
    - choice_any: { who: self }
```

這個印刷持續能力沿用 `Permission(ChooseMode)`，只在控制者原本要選至少一個
チョイス模式時，改成可選零個到當次所有合法模式。原本求值為零的群組仍不執行，
一般 `either` 解算選項與任意原始模式宣告不因此開放。
零個模式仍支付正常打出費用；每個模式的費用、正式目標、已用模式與群組條件照常檢查。
選多個模式時按印刷順序執行，不按回覆中的 ID 順序；重複、不合格與未列出的模式都會拒絕。

一般打出與 Quick 預覽使用當前有效能力，正式題目與保存的出牌計畫使用相同數量上限，
不是僅改顯示值。來源離開 EX 或失去能力即失效，只影響來源當前控制者，不影響對手。
目前僅准入 `zone: ex`、`who: self`、沒有 `while` 的單一印刷持續效果；
其他區域、條件、玩家安裝與物件安裝仍拒絕。BP20-P65 的零個模式語義依
CR 1.3.5／5.18.2.2 推論，與作者語法規格相同；Q2704 確認新クレスト在 EX 生效後，
後續打出的オケアノス待機能力即可選任意數量。

## 單張從者的奪取與交付

正式宣告的單一對手場上從者目標，可用 `steal: {ref: enemy}` 奪取，沿用
`Transfer` 與既有控制權變更流程。只開放固定一張目標、目的控制者為能力控制者；
`steal` 的多張、任意集合、`to`、`result` 與 `precondition` 仍不准入。

`give: {ref, to}` 把先前選定的從者交給一位玩家，同樣沿用 `Transfer`。
選取為空時不交付；選了多張就逐張交付，每張各自重驗接收方空位。
`to: other_players` 是交付者以外的玩家：在 `each_player` 程序內是該程序玩家以外的玩家，
程序外是能力控制者以外的玩家；`to` 也可寫 `self`、`opp` 或程序玩家名稱，`all` 不准入。
接收方場上的空位在交付當下才讀：BP13-043 的程序先讓雙方依序指定，再從回合方起依序交付，
後交付的一方看得到先交付造成的空位變化（Q1766）。`result` 與 `precondition` 仍不准入。

這不是離場再進場：所有者、物件世代、傷害、數值修正、計數器、進化與出走關聯都保留，
裝備沿既有規則跟隨穿戴者的控制者，不產生入場或ファンファーレ。
離場時仍去所有者的區域，離場能力使用離場前的控制者。

目的場上已滿時，正常宣告與付款仍成立，奪取不執行；後續 `stand: enemy` 照常作用於
原目標（BP03-109 Q539），因此不能把站立綁在奪取成功結果上。
控制權改變的當回合，先前已進化的身分仍保留，但不能靠那次進化取得攻擊資格；
突進、疾走維持原規則（Q534）。奪取不把目標標成新入場，原本已於本回合入場的事實也不會清除。
奪取後再進化可依新進化規則攻擊アクト從者，仍不因此取得攻擊領袖的資格。
奪取基底後的新進化使用目前控制者自己的進化牌組，
各實體仍保留各自的所有者，離場時分別返回對應區域（Q538）。

## 放回牌庫頂或底的分配

`move: {ref, to: deck, position: {one_of: [top, bottom], by}}` 由 `by` 指定的玩家把 `ref` 的每個物件
分到牌庫頂或底，並決定各堆的順序（BP01-008 Q156）。題目沿用 target roles：
角色 1 是牌庫頂、角色 2 是牌庫底，每個候選都必須分配；各堆答案的順序是由上往下，
第一張最靠近牌庫頂，在牌庫底時也是一樣。之後依答案把兩堆一起放回各自所有者的牌庫；`position.by` 決定位置，`move.by` 是放的人，
省略時由決定者自己放，`move.by: self` 則由效果控制者放。

`ref` 可以是正式目標或場上查詢。正式目標是先前已選定並授權過的物件，
提問時仍要是目前公開的卡；先前選定後已移區的物件已是新物件，不再列為候選。`by: {master_of: victim}` 在選取時記下該物件的控制者，
之後移動也不會改變決定者。`one_of` 目前只接受 `top`、`bottom` 兩者；`nth` 與其他位置不准入。

## 謝幕本體與 token 的同批 EX 容量

BP13-072／BP13-SL16 的謝幕使用明示 `simultaneous.do`，內含依序兩個成員：
`move.ref: this` 到 EX，以及固定一張具名 token 到自己 EX。移動本體可具名 `result`，
批次之外以 `counter.on.follow` 取得實際成功的新世代；不准在批次內消費同批結果。
目前只開放這個兩成員形狀，其他來源、目的區、數量或巢狀控制保持拒絕。

兩項依批次開始時的候選與實際目的區共同分配容量。EX 剩一格時，玩家可選本體或 token
（Q1776）；選 token 不會先移動本體，本體留墓，也不獲得休眠計數器。未獲選的 token
不配置物件。EX 已滿時兩項都不進入；兩格以上則一起提交。
本體沿原移區語義到擁有者 EX，token 到謝幕控制者 EX；兩位玩家不同時各自計算容量。

## 單次私有手牌與牌庫邊界選擇

`choose.position: top_or_bottom` 與緊接的 `move.position: {chosen: 同一 bind}`
目前准入固定一張自己手牌、不加篩選的形狀，目的地限定牌庫，`move.ref` 必須原樣引用
同一個 bind。Ginne BP16-022／BP16-SL08／PR-449 以此標準形同時決定卡片與頂／底，
不再另問第二次位置。選卡前仍先完成抉擇模式與正式目標，然後才執行抽牌。

入口不接受其他來源、數量、子集、另一 binding、排序或結果名稱；`one_of` 與其他位置
宣告不在此准入範圍。解決時沒有手牌仍沿原「盡可能執行」規則，不改成打出時的正式目標。

## 送墓成功成員的原始費用種類數

`distinct_orig_costs: {in: {objects_of: milled, view: before}}` 沿既有種類數聚合，僅讀同一本文先前由自己牌庫頂固定正張數送到墓場的實際成功成員。
`== 1` 同時表示成員非空且原費全部相同；不以指示要求的張數或墓場目前全部卡片代替。回條不存在、來自其他來源／目的地、After 投影或附加篩選皆拒絕；原費讀不可變本體印刷，不套用目前費用修正。
ECP01-037 及其全部日文印刷依 [R-0017](../../authored/rules/rulings/R-0017.yaml) 採 `strength: inferred`：只成功一張仍抽牌，零張不抽。這是本專案解讀，沒有直接官方邊界裁定；三語助理提示登錄在裁定檔，卡片 `meta.rulings` 引用它。

進化能力的 `usable_if.entered_by: {ref: this, by: ability | play}` 讀取該物件目前世代的已確認進場方式，跨回合保留。入場曲本文仍讀它所屬的進場事件；離場再入場後依新事件重記，不接受初始物件狀態中自行標示的進場旗標作為證據。

`play.optional_cost: {earth_rite: N}` 可搭配 `optional_cost_set` 或 `optional_cost_delta`，在同一次出牌報價中選擇是否支付土之秘術。材料保留 Stack 關鍵字及實付 counter 的付款事件；支付後剩餘 Stack 為零時依既有規則送墓。拒絕付款時沿用原本出牌費用，不消耗 Stack。

玩家出牌費用修正可用 `until: {during_next_turn: {who: self | opp}}` 指定只在下一個實際開始的指定玩家回合生效；安裝當下尚未生效。額外回合、跳過回合沿用回合排程，該回合結束後清除修正。修正的對象篩選仍相對於原效果控制者。

出牌費用條件的 `count: {who: self, zone: hand, other: true}` 在當次報價時讀手牌張數，排除正在打出的來源本身。來源原在 EX 時不扣除任何手牌；讀值只計張數，不開放手牌內容或對手手牌篩選。

攻擊時本文與授予的攻擊時能力可用 `damage.to: attack_target`，讀取該次攻擊事件保存的對象。對象仍須是原世代的場上從者；攻擊領袖不會產生從者傷害對象。授予能力保留攻擊者作為能力持有者與來源，授予者只提供能力本文。

對已選 EX 對象安裝的 `cost_mod.delta` 可使用既有費用數值表達式，例如以負數乘上本回合已打出卡片數。表達式保留原效果的控制者與來源資訊，每次報價重新讀當前歷史；提供者離場不取消安裝到該 EX 物件世代的修正。目標離區或期限結束後清除修正及其數值上下文。

`auto.zone: ex` 可觀察符合 `on.evolved` 篩選的場上進化事件。EX 來源使用原控制者與當前世代；`in_zone` 可在結算時確認它仍在 EX。返場可保留成功移動結果，再以 `evolve.ref.follow` 對新世代進化，不把原能力來源改成返場物件。

進化費用可使用既有 `skip_turn: {who: self}`，將下一個己方回合加入共同回合排程的跳過佇列。它是實際付款項，不增加進化次數或免除其他費用；選擇另一條進化能力時不支付此項，無效進化宣告會回復整筆付款。

`auto.on.any_of` 可合併不同事件家族，例如己方從者進場或進化。每個分支沿用既有事件生產器的快照與篩選；所有分支仍屬於同一個能力實例，共用次數限制與原來源、控制者。同一事實同時符合多個分支時只成立一次，不把共用本文改成多條獨立印刷能力。

進化事件的 `sel` 可同時以原費與種族篩選；原費取進化前的原始卡面，種族取事件完成後的特徵，不受現行費用修正或進化卡面無費用影響。

`auto.on.moved: {from: {who: self, zone: ex}, during: your_turn}` 搭配 `aggregation: batch`，每次實際移區指示只產生一次待機。本文的 `{event: amount}` 讀取該批符合玩家及目的地的成功離開數，包含離開後消去的 token，不包含未移動或其他區域的卡；該值在移區提交時保存，不受後續 EX 內容影響。省略 `to` 觀察所有目的地，正式打出從 EX 到解決區也會觸發。

## 放到場上的卡不能使用入場曲

`move` 到場上時可加 `entry_effects: [{cannot: {what: play_ability, ability: {keyword: fanfare}}}]`，
表示「それの{ファンファーレ}能力を使えない」：這次放上場的卡，自己的入場曲不會成為待機中的能力（GAP-A-044，不改寫成 `no_trigger`）。
入場曲只會在持有者自己進場時誘發，所以這個限制在該次進場的誘發判定時就處理完，不會留成持續效果；
同時進場的其他卡與其他觀察進場的能力照常誘發。目前只接受這一種 `entry_effects`，其他形狀與非場上目的地都會拒絕。
BP04-038 接著用 `apply: {on: {follow: entered}, effect: {cannot: {what: attack}}, until: turn}` 表示「このターン、それは相手を攻撃できない」。

## 持續傷害加值讀來源 counter

場上常在的 `damage_mod.delta` 除常值外，也接受 `{times: [{counter: {on: this, name: …}}, N]}`（N 為正整數），
在每次提出傷害時讀能力持有者目前的 counter 數乘以 N。BP20-021「これの融合カウンター1個につき、これが与える能力ダメージを+2する」
寫成 `damage_mod: {source: this, kind: ability, delta: {times: [{counter: {on: this, name: 融合カウンター}}, 2]}}`；
counter 為 0 時加值為 0。安裝型（`apply`、`player_effect`）的 `damage_mod` 仍只接受常值。

## 進場主體排除同名並成為進化對象

`auto.on.enter_field.sel` 可加 `not_same_name_as_this: true`，表示「これと同名を除く」：
進場的卡與觀察這個能力的卡在誘發當下只要有一個卡名相同，就不誘發。比較的是觀察者目前的卡名，不是事件中保存的資料。
這個篩選只用於卡片自身的進場觀察；`delayed.on.enter_field` 沒有觀察者卡，不接受此欄位。

同一觸發的本文可寫 `evolve: {ref: it}`，讓進場的那張卡進化（BP22-003／004「…が出たとき、それは進化する」）。
沿用效果進化的選牌：從自己進化牌庫選一張對應的卡，可不選（Q2789），不支付對方《進化》的成本（Q2788）。
進場的卡已離場或不是自己控制的從者時不進化。

## 公開候選的 counter 門檻與 EX 卡觀察進場

公開區域查詢的 `where` 除了 `power_of: item`，也接受 `counter: {on: item, name: …}` 與常值比較（`ge`／`le`），
逐張讀候選目前的 counter。例如 BP21-060 的「自分のEXエリアのカード1枚の情熱カウンターが4個以上なら」寫成
`exists: {who: self, zone: ex, where: {cmp: {left: {counter: {on: item, name: 情熱カウンター}}, op: ge, right: 4}}}`；
任何一張達門檻即成立，不加總多張的 counter。counter 只在公開區域的現存物件上讀取，私有區域選取不開放此篩選。

`auto.zone: ex` 可用 `on.enter_field`（不寫 `ref: this`）觀察其他卡進入場上，BP21-057 在 EX 時，每有一張自己的学院從者出場
就在 EX 的自身放一個 counter（Q2736；同時出場兩張誘發兩次，Q2740）。在場上的同一張卡不因此誘發；
從 EX 打出到場上時 counter 保留（Q2737）。

## 觀察放入消滅區

`auto.on.banished` 除了手牌的自身（`ref: this, from: hand`），也可寫 `{who: self | opp, during?: {turn: self}}`，
觀察「（自分のターン中、）自分の消滅領域にカードが置かれたとき」：不論卡從場上、手牌、墓場、牌庫或其他區域放入，
也不論是效果、成本或規則造成，每一張卡各誘發一次（Q2813）。`who` 指放入的消滅領域屬於誰，也就是卡的擁有者。
這是場上的觀察；這張卡自己從場上被放入消滅領域時仍會誘發（Q2816），token 放入後立即消去也照樣誘發（Q2814）。
沒有 `sel`，不篩選放入的卡。

## 安裝有期限的傷害上限

`damage_cap: {threshold: N, set: M}`（N > M）除了常駐，也可以安裝：
`player_effect: {who: self, effect: {damage_cap: …}, until: turn}` 讓自己的領袖本回合每次受到 N 以上的傷害都變成 M（BP05-101）；
`apply: {on: …, effect: {damage_cap: …}, until: turn | this_and_next_opp_turn}` 讓選到的物件在期間內同樣處理（BP08-024）。
上限逐次套用，不是累計（Q760、Q1067）；`this_and_next_opp_turn` 在自己的回合連續時，第二個自己的回合不生效（Q1066）。
物件上的上限是該物件獲得的能力（「それは『これが受ける4以上のダメージを3にする』を持つ」）：之後失去全部能力時一併失去，失去能力之後才獲得的上限照常生效。

## 成功進場成員的種族條件

`exists: {in: {follow: entered}, kind: follower, trait: 盗賊}` 檢查本次具名進場回條的成功成員（BP11-033；BP03-037 的ヒーロー條件使用相同方式）。`entered` 來自先前到場的 `move` 或 `search`；條件讀取實際進場的新世代及目前有效特徵，選零、滿場等沒有成功成員時為假。物件須仍是同一世代且在場上，移區或重新進場不會讓舊回條跟隨新物件。`exists` 是布林條件；後續 `buff.on` 與 `grant.on` 的 `{follow: entered}` 仍引用原回條的全部成功成員。上述兩張卡一次最多讓一張卡入場，因此條件與追加效果讀取同一個實際入場對象。

進場回條的存在條件可組合既有場上 `kind`、`trait` 與 `keyword` 篩選，讀取目前有效特徵；原本的守護判斷沿用既有路徑，其他已支援關鍵字也走共用場上篩選。
