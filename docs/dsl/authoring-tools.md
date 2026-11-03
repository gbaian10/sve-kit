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

基本關鍵字的持續授予使用 `static: {while: {awakened: true}, effects: [{grant: {keywords: [storm]}}]}`；省略 `on` 表示自身，省略 `while` 表示無額外條件。`grant.on` 可用目前場上的公開篩選，例如 `{who: self, zone: field, name: オルカ, token: true}`。接受七種基本關鍵字及 `drain`，場上來源失去能力或離場時不再授予；條件與受影響對象每次查詢都依目前狀態重算，包含稍後入場的對象，不會改寫印刷關鍵字或既有授予效果。此入口只接受場上持續能力；一般能力定義授予與其他持續修正仍依各自的 runtime 准入範圍。

`grant: {on: ally, keywords: [drain]}` 沿用世代綁定的關鍵字授予，可省略期限或指定 `until: turn`。吸血經同一實際傷害事件與待機佇列回復生命，來源定位保留授予者的印刷卡號與行，能力持有者、控制者及實例則屬受益物件；不同受益物件有不同實例。依 CR 12.13.3，印刷、施加及持續授予的多個吸血只產生一次回復。受益物件離場後的新世代不繼承授予；施加效果不因授予者離場而提早終止，持續授予則每次依來源、條件與對象重算。

一般誘發能力可寫在 `grant.abilities`，例如 `grant: {on: ally, until: turn, abilities: [{attack_time: {do: [{draw: {n: 1}}]}}]}`；`static.effects[].grant` 也接受同一個 `abilities` 陣列，並可同時授予關鍵字。目前接受場上的攻擊時、Main／End 時點與謝幕能力，本文與正式目標沿用一般誘發能力的檢查及結算流程；巢狀能力使用獨立的區域與綁定，不能引用授予者本文的目標。每次施加各自產生能力實例，持續授予保留提供者世代及效果來源；能力持有者與控制者則取受益物件。謝幕在受益物件離場前收集，已待機能力保留離場資訊。施加期限與來源離場規則同上；EX 區另接受既有的 Main／End 時點能力頭；loader 明確拒絕其他事件，以及在授予能力中再次施加授予。

一般持續能力使用同一個 `grant.abilities` 入口，例如 `abilities: [{static: {effects: [{prevent: {kind: any}}]}}]`。支援既有的防傷、固定傷害加減與傷害上限宣告，以及只授予關鍵字的單層持續能力；本文、受益者、提供者與期限沿用同一份授予狀態。查詢持續授予時讀取印刷及已施加的能力，避免再次遞迴查詢完整持續授予結果；持續授予另一份持續授予能力、在授予的持續能力中再授予 Definition，以及未接線的限制宣告會被拒絕。不同施加仍保留獨立來源，傷害替代的參照會區分施加次數，不公開已離場提供者的物件 ID。

固定播放減費可寫成 `grant: {on: {who: self, zone: ex, name: カーバンクルの煌めき}, abilities: [{play: {cost_delta: -2}}]}`。它降低為同一個靜態能力中的既有 `CostModifier`，以受益物件自身為範圍，經共同播放費用計算、合併獨立減費並把實際費用限制在零以上；不建立另一套費用狀態。只接受固定非正整數，不接受改成固定費用、條件式或可選費用。宣告在 EX 與播放解決區有效，外層施加仍可使用既有的本回合期限。在 EX 到解決區、解決區到場的播放規則移動，以及 EX 直接入場時，與其他已支援授予一同攜帶原效果；入場觀察前就可讀到承接後的能力，普通移區不繼承舊世代。

授予可施加到場或 EX 區的適正受益者；其既有能力仍須通過一般准入邊界。`grant.on: {follow: moved}` 沿用同一效果的成功移動回條，對新世代施加能力；失敗移動不產生受益者，不把原物件 ID 當成永久追蹤連結。來源與目標的有效區、期限及生命週期分開處理，並沿用既有授予層對失去能力的處理。

正式目標與本文 `choose.from` 的公開篩選接受 `traits_all: [絶傑, 魔界]`（同時具有全部特徵）、`token: true` 及 `names: [没頭の実験体, 多頭の実験体]`（名稱符合任一項），共用既有物件篩選與目標合法性檢查。

場上正式目標另接受 `acted: true | false`（橫置／直立）、`evolved: false | true`（只比對從者；`true` 包含超進化）與 `entered_this_turn: true`（本回合入場，每個回合開始時清除雙方場上的標記）；這些狀態篩選只用於場上候選，不接受領袖、觀察卡或墓場／EX 來源。`not_same_name_as_this: true` 也可用於公開正式目標與本文 `choose`，以能力持有卡本身的卡名比較，因此謝幕在來源離場後仍排除同名卡；與領袖候選並用時拒絕。場上物件來源（傷害、破壞、起身等的 `{who, zone: field, ...}`）接受 `acted`、`kind: amulet`，以及 `except: {in: 已選目標}` 排除同一能力先前選定的目標（例如「それ以外の相手の場のフォロワーすべて」）；與 `as`／`where` 極值並用時拒絕。公開存在與計數查詢可用 `where: {cmp: {left: {power_of: item}, op: ge | le, right: N}}` 依有效攻擊力篩選。場上限定的持續能力可在 `static.while` 以 `standing: this` 讀取能力持有者目前是否直立；起動條件與本文等其他位置讀取持有者姿態仍不支援。

Look 的私有選取可寫 `choose.from: {any_of: [{in: seen, kind: follower, trait: クリスタリア}, {in: seen, kind: follower, trait: 妖精}]}`，表示「A 或 B」的同一候選集合（BP02-008、BP21-010）；每個分支須讀同一個 Look、各自帶篩選，同時符合多個分支的卡仍只算一張。`groups` 表示「A 與 B 各至多 1 張」，每組須寫 `kind`，可加 `class` 或 `orig_cost_le`（BP05-003 的エルフ從者與エルフ法術）；`groups` 不與 `any_of` 並用。可多選的 Look 選取若要進場，`move: {ref: <選取>, to: field}` 必須緊接在 `choose` 之後、位於同一層本文；選項上限在出題時依空位計算，中間或條件分支內的進場會在載入時拒絕。分組或聯集拿回手牌不受場位限制。

`play.additional_cost` 在卡片宣告打出前必須完整支付，沿用能力費用的 `Cost` 操作與素材選擇；付不起或素材不合法時整次打出取消。目前限法術，且不與模式付款或能力頭的可選付款並用。已接通自己場上素材的送墓／橫置、自己公開 EX／墓場素材的消滅，可用 `steps` 保留多組成本的順序。這不是可選付款，也不會在效果結算時才收費。`result` 可供同次法術本文以 `paid: {receipt: 名稱}` 讀取；指定進場素材條件、私有手牌材料與 Quick 預檢仍須各自的支援入口。

數值算式接受 `plus`、`minus`、`times`（各恰兩項）、`min`（非空列表）與 `per: {of, unit}`。`per` 是「N 枚につき」：`of` 除以正整數 `unit` 後向下取整（負數往負無限大取整），`unit` 為零在載入時拒絕；有號溢位使整個指示失敗、不改變局面。`max` 與 `half_up` 尚未接線。傷害事件頭（`damaged`／`dealt_damage`）的本文可用 `{event: amount}` 讀該次實際傷害值；其他事件頭、起動與入場曲不提供此讀值，`{event: value}` 亦未接線。

`play.cost_delta` 可使用既有公開計數、`minus`／`times`／`per`／`min` 運算與 `play.define` 別名，依付款時的公開數量加減印刷費用；正數為加費，負數為減費，所有修正合計後才以零為下限。場上、EX、墓場與自己表向進化卡的計數沿用一般數值與選擇器檢查，不能藉此篩選隱藏手牌／牌庫。`in_zone: {ref: this, zone: ex}` 在減費條件中讀取本次打出前保存的來源區域，因此卡片移入結算區後仍能正確付款；目前接受手牌與 EX，且可與公開存在／數量條件組合。

場上持續打出費用修改可用 `static.effects: [{cost_mod: {when_playing: {who: opp, kind: spell}, delta: 1}}]`；`when_playing` 可依玩家、卡種、類型、職業、卡名、原始費用上限及 token 篩選，或用 `any_of` 組合。省略區域時包含手牌與 EX，明示 `zone: ex` 時讀本次打出前保存的來源區域。持續來源每次付款依目前場上能力重算，來源離場或失去能力後停止套用；`static.while` 使用已准入的公開付款條件，但不接受僅本次打出才有的來源區域讀值（`in_zone`）。

「本回合下一張」使用 `player_effect: {who: self, effect: {cost_mod: {when_playing: {who: self, trait: 武闘竜人}, delta: -3, next_only: true}}, until: turn}`。此安裝保存玩家與打出篩選，涵蓋稍後才取得的卡片，不捕捉隱藏手牌。整組共用一次額度，僅在符合篩選的卡適正打出且全部付款成功時消耗；預覽、非符合卡及失敗宣告均不消耗。安裝者離場不提早失效，未用額度在本回合結束時移除。`delta` 與非負整數 `set` 二擇一；各費用層的增減合計後才以零為下限。

整卡 DSL 在索引中以永久 CardId 登錄一個代表卡號。current 選用依身分登錄展開同 CardId 的所有官方 JP 印刷卡號，共用代表卡的程式及來源定位；卡面仍讀各印刷的快照資料。不從卡號字串推測別名，也不為別名另登索引或保留原型程式。未解決的裁定同時阻擋這些別名。

實體 token 使用同一主檔、具名本體及行覆蓋入口。卡片層的 `token: {name: 操り人形, template: true}` 指定規則名稱與同名 token 的代表印刷；`template` 預設為 false，省略整個 `token` 時使用快照卡名。同名 token 有多個已選用程式時須恰有一個代表。卡面、數值與型別仍來自快照，`token` 只能標在單面的 token 卡上；印刷的突進、守護等以 `keywords` 宣告，與一般從者共用 lowering 與執行規則。

所有觸發能力都可用 `targets` 宣告正式目標；費用時點依[作者語法 §3](author-syntax-1.0.md#3-費用目標與打出程序)區分頭付款與本文付款。控制者在檢查時點打出待機能力時選擇目標，沒有足夠合法目標就不發動，不執行後續效果。謝幕在來源離場後選擇目標。目標宣告前不執行本體；同一印刷行的入場曲與謝幕仍以不同 `scope` 區分。模式目標、單一目標組的分配傷害，以及 `at: resolution` 公開計數讀值沿用共同打出規則。

場上來源的 `auto.on.enter_field`、`left_field` 與 `destroyed` 共用事件主體篩選：`who` 限制該時點的控制者，`sel` 可篩 `kind`、`trait`、`name`、`class`、`other`，進場與離場另可篩 `token`／`not_token`；省略的條件不加限制，省略 `who`／`sel` 可監看所有物件。進場讀進場後資訊，離場與破壞保留離場前資訊；`this` 仍是能力來源，不自動改指事件主體。`left_field` 包含送墓、回手、消滅與移 EX，`destroyed` 只由實際破壞成立，付款送墓不會觸發；`moved: {from: field, to: cemetery}` 只監看送墓；`moved: {from: field, to: hand}` 只監看場上返回手牌，可用 `ref: this` 或 `from: {who: self, zone: field}` 限制自身或自己控制的卡。回手事件在離場前收集，因此來源自己回手仍會觸發；墓場回手不會觸發。來源與主體同批離場時，來源使用封存資訊，既有自身離場與謝幕各只收集一次；正式目標、付款與限次沿用共用觸發流程。

事件本文的 `it` 共用標頭的型別綁定，可指向進場、入 EX、攻擊、進化、離場及破壞的主體；`this` 始終是能力來源。進場使用新世代，離場與破壞使用離場前世代；舊事件的主體離場後再回場，不會強化回來的新世代。沒有卡片主體的時點及起動能力不能使用 `it`，也不以來源代替缺少的事件欄位。

`at: selection` 的單一正式目標捕捉可讀 `cost_of` 與 `orig_cost_of`，沿用同一選擇回條與數值捕捉；原始費用取印刷資料，有效費用取當時修正。捕捉發生在選定後、第一個效果指示前，之後目標被破壞或返回手牌都不改變該數值；不授予隱藏區域或任意多物件讀取權限。

攻擊本文可用 `is: {ref: attack_target, kind: follower}` 檢查本次宣告的目標，以及 `hp_of: attack_target` 讀取仍在場同世代目標的當前體力；`attack_target` 也可作已支援數值指示的對象。攻擊領袖仍產生待機能力，條件在解算時查（R-0003）；目標離場或換世代即不符合，不追隨新物件。這些讀值均由同一待機事件提供，不能在起動能力或其他事件偽造攻擊目標。

場上或墓場來源的 `auto.on.played` 沿用共用事件主體篩選；`who` 可省略或指定 `self | opp`，`sel` 可篩 `kind`、`trait`、`name`、`class`、`other`、`token | not_token`，`during: {turn: self}` 限自己回合。事件以適正打出成功為準，包含已支援的從者、護符、法術及護符／法術 token；付款失敗、效果直接放場與生成 token 不產生打出事件。場上觀測者必須在成功時已位於場上，新打出的卡不倒算自身這次打出；墓場來源同樣在法術本文或入場前擷取，不能事後補觸發。正式目標、付款及每回合一次限制沿用共用觸發流程。

場上來源的 `auto.on.attacked` 沿用共用事件主體篩選，`who` 可指定 `self | opp`；`sel` 可篩種類、類型、名稱、職業、token 與 `other`，另可用 `keyword` 篩有效關鍵字。`sel.as` 配合 `where` 可檢查該攻擊者的有效攻擊力下限，或用 `any` 組合關鍵字 `exists`；條件在真正宣告攻擊時判斷，先產生待機能力，結算後才進入 Quick。`this` 仍指能力來源，`as` 的具名別名只供標頭條件使用；本文可用保留的 `it` 指向本次攻擊者。

`auto.on.dealt_damage` 以同一筆實際傷害的來源套用事件主體篩選；`auto.on.damaged.recipient` 可指定 `this` 或 `{leader: self | opp}`。兩者的 `kind` 可省略或指定 `ability | attack | combat`，`during` 可限制回合；先處理禁止／防止，再以實際正傷害判斷，全防止與零傷害沒有事件。對領袖的攻擊傷害屬 `attack`，不屬 `combat`；從者交戰中攻擊方造成的傷害同時符合兩者，反擊方符合 `combat`（[Q282](https://shadowverse-evolve.com/cardlist/?cardno=BP02-020)）。傷害來源與受傷者各自篩選，正式目標、付款與每回合一次限制沿用共同待機流程。

`life` 的 `who` 接受 `self` 與 `{master_of: <目標>}`：後者是「それのリーダー」，取已選目標在選取時的控制者，目標隨後被破壞或移區仍回復同一位領袖。對手或對手集合的 `life`、`set_life` 的其他對象尚未接線，載入時拒絕。

`auto.on.life_changed` 可用 `who: self | opp`、`direction: up | down` 與 `during` 篩選領袖生命實際變更；省略條件即不限制。加減或設定生命都按變更前後的實際差額產生事件，差額為零不產生事件；生命不封頂於 20。生命支付與傷害造成的減少亦經同一變更入口，但直接加減生命不是傷害，不會因此觸發 `damaged`。這些事件頭沿共用觸發額度，可用 `per_turn: N` 宣告每個實際回合的正整數上限。

一般長寫 `auto` 的冒號在本文寫 `pay: {cost: {act_self: true}, do: [...]}`（送墓用 `self_to_cemetery`；可用有序組合）。正式目標在打出時先選，到了該指示才決定付不付；可放在已支援的 `if` 內。拒付或付不起只略過該段本文，能力仍算已打出並結算。付款原子提交，失敗保留先前已結算的效果並回復本段付款；來源須仍在場上，不能用另一張同名卡付款。這個結算付款子集只接受場上普通自動能力的自身橫置／送墓，不接受 EX 來源、私有材料或任意成本。

這三種事件頭尚不接受 schema 中的 `by`；`dealt_damage` 也尚不接受 `zone`。未支援的欄位會在載入時拒絕，不會忽略篩選條件。

`apply: {on: 目標, effect: {add_trait: {trait: 武装}}}` 對場上從者增加類型，沿用
`Action::Modify` 的 AddTrait 與既有授予狀態；`until` 可省略或為 `game`。
接受者只寫在外層 `apply.on`；`add_trait` 內層 `on` 尚未准入。
既有類型保留，重複增加同一類型不產生重複值；目標篩選、事件主體及離場 LKI 都讀有效類型。
授予者離場不撤銷已結算的類型；接受者離場後不追隨新世代，規則移動沿用授予狀態的承接。
此入口不開放 RemoveTrait、靜態類型修改、混合 patch 或其他期限。

`auto.on.acted` 與 `auto.on.stood` 訂閱實際橫置／起身，可用 `ref: this`、`who: self | opp` 或與進場相同的 `sel` 主體篩選。能力指示、費用、攻擊宣告、入場守護、結束階段守護及規則起身都經共用姿態入口；已是指定姿態的物件不產生事件。每個變更物件各是一個事實，本文與每回合一次限制沿用共用 pending 流程。

`auto.on.stat_changed` 接受 `ref: this`、`stat: power | hp`（或清單）及 `direction: up | down`。省略欄位表示不加該項限制；一個指示對同一物件的攻擊力／體力差額合為一個事實，清單採任一欄位符合，不因兩項都增加而重複觸發。零差額不產生事件；明示數值指示與超進化的 +1/+1 會產生，普通進化換底值、持續條件重算與期限到期不產生。這兩類事件頭目前使用場上來源與既有每回合一次限制，不忽略其他未支援欄位。

`ub: true` 是觸發／起動能力的獨立標記，沿用該能力原有時機、正式目標、付款與本文。能力控制者必須使用作品構築，且作品規則綁定含 `ub_enabled`；職業構築、交叉構築及其他作品下，UB 能力在所有區域無效。同卡未帶 UB 的能力仍有效。適正打出後在既有 `AbilityPlayed` 事實記錄 `UnionBurst` 資格，目標不足、拒付或付款失敗不記錄；本文沒有成功改變狀態仍算一次適正打出。此入口尚未新增 UB 歷史查詢、UB 發動訂閱或免原費用發動其他 UB 的 consumer，不能據此准入那些本文。

謝幕可用 `cost` 宣告頭付款（例如 `cost: {pp: 2}`），沿用接受／拒絕與不足資源不發動的付款流程；目標在付款前宣告。

`cost: {lesson: 2}` 選取自己 EX 中規則名稱為「魔法のアイテム」的兩張卡並消滅；起動與誘發能力的 `cost.steps` 可將 PP、Lesson 與一組手牌費用依序組合；所有選擇先完成，整份付款成功才發動能力，後段付不起時前段付款一併回復。Lesson 沿用一般公開材料的付款，不產生專用計數器；能力標頭決定付款時點，正式目標與材料都在付款前選定。此入口只接受固定正整數，不以 token 的印刷名稱或卡號識別材料，也不代替該 token 本身的整卡准入。

EX 起動的 `cost.steps` 可依序組合 PP、`banish_self: true` 與指定名稱的 `banish` 材料。
自身加另外兩張同名卡時，材料使用 `sel.other: true` 排除來源；正式目標與材料都在付款前固定。
材料不足或 PP 不足時整份付款不成立，不先扣 PP 或移動任何卡片；場上來源不能冒用 EX 起動。

`keywords: [single_drive]` 與 `keywords: [twin_drive]` 展開為自身攻擊時的完整 Drive check；Twin Drive 先完成第一張的圖示處理與去向，再取第二張牌頂。非作品構築不讀圖示、直接放回牌底；作品構築從權威端既有 card facts 讀取已驗證的 `trigger_icon`，缺資料時拒絕，不從提醒文推算。接受圖示後依一般抽牌、生命、數值與姿態規則處理並送墓，拒絕或無圖示則回牌底；Critical 只增加攻擊力 2，Stand 的領袖攻擊限制持續至本回合結束。

`ride: {cost: {pp: N}}` 共用進化相當能力付款交易：從自己的エボルヴ牌庫選一張裏向 DrivePoint、可用一點 EP 代替一點 PP，將該資源移到 Drive 領域並關聯自身。使用憑依會消耗本回合進化相當能力額度，且同一場上物件只能使用一次；不產生進化事件。`gain_drive: {ref: this}` 與 `drive_time: {do: [...]}` 分別執行首次取得及訂閱首次取得事件。首次取得會授予 Drive、Single Drive 與突進；歷史按物件世代保存，失去能力不重置，離場重入才重新計算。離場解除關聯後，DrivePoint 沿既有資源回收規則回エボルヴ牌庫表向。

謝幕中的 `move: {ref: this, to: field}` 可將該次離場後仍在墓場的普通從者放回控制者自己的場上；沿用入場事件及守護姿態選擇，不重新打出卡片或支付印刷打出費用，但會照常觸發入場曲（[官方玩法說明](https://shadowverse-evolve.com/rules/ver_0324/)）。它只追蹤該次離場產生的世代，卡片後續再移區後不會追取新世代；場上已滿五張時不移動。墓場法術打出觸發也可將自身放回場上；其他持有者位置、任意選擇物件的回場、復活入場附帶效果與其他連結家族仍不由此入口准入。

墓場法術打出觸發可接正式目標、可選付款與模式，也可執行不操作持有者的普通本文；對 `this` 的操作只准放回控制者場上，不能把墓場持有者當成場上活物件強化、破壞或移 EX。

`entered_from`、`entered_by`、`played_from` 的 `ref: this` 在入場曲與 `auto.on.enter_field: {ref: this}` 讀取自身入場事件。監看他者入場時，可用 `entered_from: {ref: it, from: cemetery}` 讀取該次事件主體的來源；不把能力持有者當成事件主體，其他事件不提供此讀值。進化能力的 `usable_if` 另可用 `entered_from: {ref: this, from: cemetery}` 查詢持有者本次場上世代的入場來源；離場再回場即重新記錄，普通進化不改變來源，進化資格目前限查詢墓場來源。入場事件接受手牌、牌庫及墓場，`not` 可表示非手牌；不由入場方式推斷來源區域。入場來源條件亦可控制唯一的チョイス群組是否出現；`choice.instead` 另接受墓場來源條件以改變可選模式數，仍在宣告時選模式，並沿用同一事件的凍結來源。自身回場及離場後移 EX 需要該次離場確定落在墓場，目前只有謝幕符合；一般 `left_field` 不開放這兩種動作。

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

目前 `create: {token: フェアリーウィスプ, n: 2, to: ex, result: made}` 支援固定數量、單一規則名稱、自己 EX；`to: field` 支援從者與護符 token 至自己場上。省略 `n` 為 1；EX 與場上各最多五張，只建立並放入仍有容量的數量。`result` 或 `bind` 擇一，綁定實際放入的 token，滿區時為空；authority receipt 同樣只列出成功放入者。token 的整卡 runtime 程式仍須准入。場上建立共用進場事件與守護的選擇橫置程序；沒有先前區域的建立以 `EntryOrigin::Other` 表示，成功建立的每個 token 各發生一次進場。

EX 的正式目標或結算選取可用 `from: {who: all, zone: ex}`，也可用 `self`／`opp` 限制玩家。`transform: {ref: chosen, into: ドラゴン}` 目前接受已綁定的 EX 選取與單一 token 模板：先消滅仍屬該世代的物件，再依實際成功量於原 EX 玩家處建立新 token；原卡的持有者不會改變新 token 的目的 EX。新物件使用印刷數值與新身分，舊 token 消滅後依共用規則消去。模板仍須整卡准入，建立仍受共用容量規則限制；多模板分配與進場附加效果須另行准入。EX 資源的目標保護沿用規則綁定。

回合時點自動能力可用 `auto: {zone: ex, on: {phase: {whose: self, which: end}}, do: [...]}`，只在持有卡位於 EX 時訂閱；省略 `zone` 仍為場上。EX 訂閱目前只接受單一區域的 Main／End 時點，並不授予從 EX 打出的許可。條件中的 `count: {who: self, zone: field, kind: follower, trait: 魔法使い}` 讀取結算當下符合條件的場上張數；私有區域計數仍不接受卡型或類型篩選。普通法術可在自身結算期間以 `move: {ref: this, to: ex}` 移動自身；EX 已滿則仍走既有結算清理。

墓場的自身回場訂閱可寫 `auto: {zone: cemetery, on: {played: {who: self, sel: {kind: spell, class: ドラゴン}, during: {turn: self}}}, cost: {pp: 1}, do: [{move: {ref: this, to: field}}]}`。這個例子在自己回合、自己適正打出指定職業法術時擷取當時已在墓場的來源；該法術本文才送墓的來源不補觸發。多張來源各自待機、各自宣告及支付 PP；拒付或不足費用不回場，滿場時付款後沒有成功移動。墓場來源與場上來源共用打出篩選；本文對 `this` 的操作仍只准已有的自身回場，任意目標回場與一般事件欄位讀取維持原邊界。

自己墓場的卡可放在 `targets`，例如 `from: {who: self, zone: cemetery, kind: follower, trait: 竜族}`；共用正式目標程序在付款前固定候選與宣告，沒有必需目標時不能付款。結算本文用 `move: {ref: returned, to: hand}` 回收。不能用本文的 `choose` 代替正式目標，否則會把本次付款才送墓的卡納入候選。這個目標入口不擴充回場的種類或容量選擇能力。

Look 中所選子集可用 `move: {ref: kept, to: deck, position: top, order: chooser, result: placed}` 排回牌庫頂，餘牌寫 `{in: seen, except: {in: {objects_of: placed, view: before}}}`，再排到牌庫底或送墓（CP01-070、CP02-081）。這個 Order 收據只供餘牌差集扣除，不能當移動輸入或追蹤對象。牌庫頂直接移到 EX 的 `move: {top: {who: self, n: 1}, to: ex, result: placed}` 可接 `apply: {on: {follow: placed}, effect: {cost_mod: {when_playing: {in: affected}, delta: -2}}}`；省略 `until` 時減費綁定該物件世代持續（BP01-084），其他牌庫頂目的地仍不接受 `result`。

公開場上物件移至 EX 的 `move` 可用 `result: moved`，後續 `if: {cond: {did: moved}, then: [...]}` 只在至少一個成員實際移動成功時執行；選零或目的 EX 已滿不算成功，搬兩張也只執行一次後句。這個子集只接受已綁定的 Move 收據名稱，不接受其他結果種類、過濾條件或前向／分支外引用；一般 EX 搬移的部分容量選擇仍明確拒絕並還原，不因此擴充容量決策。

動態數量另接受本次 `destroy: {ref: ..., result: broken}` 的成功回條：`n: {count: {in: {objects_of: broken, view: before}, as: victim, where: {cmp: {left: {player: {master_of: victim}}, op: eq, right: {player: p}}}}}`。只計本次實際從場上被破壞者，按離場前控制者區分，包含隨後消去的 token；破壞失敗不計數，不按目前區域或擁有者重新計算。

`each_player: {as: p, stages: [[{create: {token: ..., n: ..., to: field, by: p}}]]}` 支援單一、無選擇的 token 建立階段，雙方在同一批次建立並進場；守護進場選擇按回合玩家優先處理，在所有選擇完成後一次提交。建立與計數須指向同一玩家；各目的區沿用容量上限與守護進場流程，守護由該 token 的控制者選擇。此入口不接受其他回條、after 計數、owner 比較、其它階段動作或結果匯出。整個能力完成後才進入衍生觸發能力的處理。

手牌作費用的 `discard.sel` 支援 `class`、`orig_cost_eq/ge/le` 與 `traits_all`，可與種類、單一特徵、名稱交集。`traits_all` 要求同時具有所有列出的非空特徵名稱；例如 BP20-042 必須捨棄同時具有「絶傑」及「魔法使い」的其他手牌咒術，只有任一特徵或從者都不能支付。篩選讀取印刷職業與原始費用，不受目前費用修正影響。觸發能力的頭費用沿用宣告、正式目標、私有材料選擇、付款的順序；拒付不執行本文，材料不足不提交付款。

BP22-068 的下一次規則起身限制使用下列 `apply`；`who` 的目前支援子集為 `self/opp`，兩處須一致，期限只接受下一個 start 的 end。安裝綁定所選物件世代與指定玩家，經實際回合排程等到該玩家下次開始階段的規則起身，隨後失效。效果造成的 `stand` 不受阻止，來源離場也不提早終止；目標離場後的新世代不沿用限制。

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

check 成功代表來源定位、行覆蓋與 DSL 結構通過；Rust lowering／runtime 能力准入及對局行為測試仍須通過。錯誤回傳位置及分類，不印整行卡文。完整回歸交 CI，無需額外證據 JSON 或結果 DB。

打出限制的 `play.cannot_if` 保留 `in_zone: {ref: this, zone: ex}` 與 `my_turn: true`，並可使用已准入的公開條件及其布林組合。例如 BP12-111 以 `cmp` 檢查自己場上的ゴブリン從者數不大於 2；一般打出與 Quick 候選都在扣費、移區或選擇前依當前狀態檢查，拒絕時不更動狀態。條件不能依賴稍後的選擇、結算捕捉或隱藏區域的卡片特徵。這些限制不阻止能力直接移動或將卡置入場，也不提供額外打出區域的許可。

起動能力的 `usable_if` 與 `per_turn` 共用既有宣告前條件及每實例每回合次數限制；PP 為 0 也會消耗成功宣告的次數。BP16-021 的墓場計數可用 `class: ロイヤル` 與 `kind: follower` 限定自己的公開素材，ECP02-003 則用場上的 `trait: クール`；條件不符時不扣費、不開啟目標選擇，也不消耗次數。

效果本文的 `evolve: {ref: this}` 可在結算時從控制者未公開的進化牌堆選一張同名普通進化卡，沿既有進化程序處理，並產生真正的進化時事件；不另扣 PP／EP／SEP，也不占用一般進化次數。只准入自身普通進化，其他對象、指定名稱及超進化仍須另行准入。沒有合法進化卡時照既有結算選擇契約回答空集合，已支付成本不回退。`entered_from: {ref: this, from: hand}` 讀取這次入場事件原有區域，可用 `not` 表示非手牌；不等同能力入場。場上 `count` 保留既有種類／trait 篩選；本批滿場判斷使用不帶篩選的場上卡數（包含護符）。

正式目標綁定的單一公開物件執行 `move: {ref: target, to: field}` 時，若自己的場已滿，該物件留在原區域並產生沒有 `after` 的失敗移動回條；已支付費用不回退，後續指示繼續。這與自身回場、事件追隨回場的既有容量處理共用路徑。多個物件爭用剩餘場位仍需獨立的容量選擇准入，不因此開放。

公開區域的數值讀取可用 `distinct_names` 計算不同卡名數，或用 `distinct_orig_costs` 計算不同原始費用種類數。兩者與 `count` 共用公開區域選取、種類／職業／類型篩選及費用條件讀值；原始費用取印刷資料，不受物件當前費用影響，同名或同費用的多張卡只計一次；沒有費用資訊的卡不另算作一種 0 費。可用於能力本體條件、動態數值與打出費用修正；`at: resolution` 延續墓場結算起始讀取，不開放隱藏區域或 `other`。

傷害的 `to: {any_of: [{leader: opp}, {who: opp, zone: field, kind: follower}]}` 可把領袖與從者放進同一次傷害的受傷者集合。它沿用既有 `RecipientSet::Union` 與去重規則，只有一個傷害動作及數量回條；傷害替代條件對同批所有受傷者生效。

進化 PP 費用修改可用 `apply: {on: this, effect: {evolve_cost_mod: {on: affected, set: 0}}, until: turn}`，或以 `delta: -1` 取代 `set`；兩者擇一，設定值不得為負。省略期限時持續至該物件世代結束，也可選既有公開場上對象。修改共用費用效果狀態，作用於實際規則進化的 PP 費用，最後以 0 為下限；不改卡片的打出費用，也不修改已離場後重新入場的世代。

正式 `choice.options` 的模式可用單一 `pay: {cost: {pp: 1}, do: [...]}` 包住本文；這是 R-0006 的可選追加費用，包含既有固定 PP、固定張數的自己手牌棄牌、公開 EX／場上材料，以及場上來源自身送墓的有序組合。先選模式，再決定各選中模式付不付；正式目標仍須在付款前合法選定，拒付不免除目標要求。拒付略過該模式的整段付款本文，也不詢問材料；未選模式不問付款、不選材料、不執行效果。

選中模式的費用沿既有付款程序，與固有卡片／能力費用一起完整提交，成功後才依印刷順序執行本文；付款失敗整次打出回復。卡片減費只修改固有 PP，不修改模式費用。手牌材料問答沿既有私有成本入口，對手無法查看候選；公開材料也使用自己的成本綁定，不把目標權限套在材料上。Quick 的模式追加費用可拒付，因此預檢仍驗固有費用與至少一組合法正式目標；接受費用後才收集並驗證材料，不因此開放必要材料費用的 Quick 起動。帶選取材料的模式最多只能選中一個，且不與卡片／能力本身的材料費用並用；多組材料共同選取仍未准入。可變 X、結算中的 `either` 付款、根付款與模式付款並用仍未准入。

`choice.upto` 可寫 `{count: {who: self, zone: field, trait: メジロ家}}`（ECP01-048）。上限在正式模式宣告時讀取自己的目前場上卡片數，包含符合類型的從者與護符；由既有程序限制在印刷選項數以內、排除沒有適正目標的選項、禁止重複選擇，正式目標先於回復與抽牌決定。零張時略過該模式群組。有可選模式時沿用既有 `upto` 的至少一項限制。此入口只支援自己的場、單一 `trait` 與 `upto`；不支援算式、隱藏區域、其他玩家、`n` 或 `instead` 混用。

印刷 `evolve.cost` 與起動能力共用 `Cost` 語法，例如 `{leader_life: 3}`、`{discard: {n: 1, sel: {who: self, zone: hand}}}`、`{banish: {n: 2, sel: {who: self, zone: cemetery}}}`。同名進化卡、進化次數、PP／EP／SEP 與超進化時點仍由原規則程序檢查，費用成功後才揭露及安裝進化卡。規則動作的材料使用既有 `costs` 輸入容器，公開材料為 `costs.materials`、手牌丟棄為 `costs.discard`，值為物件 ID 陣列；每項仍經正式材料候選與嚴格數量驗證，不能以目標選取代替付款。

本文、使用前提及常在條件可用 `my_turn: true` 檢查目前是否為能力控制者的回合；回合序數沿用 `turn_number: {who: self}`，讀取實際開始的玩家回合數。本文的 `in_zone: {ref: this, zone: ex}` 在該指示結算時，以來源原世代與控制者目前 EX 的交集判斷；來源離開 EX 或移區後再回來都不符合。`in_zone` 與其他 Condition 共用語法；此 EX 來源讀值的對局與世代驗證以本文為範圍，其他入口仍須通過各自的執行准入。它不授予新的有效區；打出限制與付款來源區域仍沿各自既有語義。

`auto.per_turn` 與 `activate.per_turn` 接受正安全整數 N，額度按能力實例與每個實際回合分開；觸發在進入待機時消耗，起動在付款成功且適正打出時消耗。`activate.usable_if` 與 `per_turn` 可並用，例如墓場キュート卡至少五張才可每回合起動一次；資格不符、拒付與付款失敗都不消耗起動額度。沒有上限時省略 `per_turn`，不能以零表示無限次。

`play.optional_cost.discard` 支援固定張數的自己手牌，配單一常值 `optional_cost_delta` 或 `optional_cost_set` 交換減費。
它與場上送墓／橫置、墓場消滅使用同一付款流程：先決定是否交換，再宣告效果目標，最後私下選足額手牌；原 PP 與棄牌都成功才打出。
正在打出的卡不列為手牌材料，拒付不棄牌且支付原費用，材料或 PP 不足則整次回復。Quick、可變張數及混合材料操作仍拒絕。
例如 ECP01-036 的 `discard: {sel: {who: self, zone: hand, trait: ウマ娘}, n: 1, result: fuel}`，
同次普通法術本文可用 `cmp: {left: {count: {in: {objects_of: fuel, view: before}, orig_cost_ge: 7}}, op: ge, right: 1}` 判斷已棄牌的原始印刷費用；拒付時該集合為空。
這項回條不注入之後的 Fanfare；Follower 的交換只影響打出付款，其入場能力仍使用獨立的待機與結算。

手牌交換不會產生離場事件，故載入的謝幕／離場訂閱不阻擋這類付款，也不因棄手牌而觸發。
其他可能觀察付款的觸發仍保守拒絕；場上送墓交換的離場限制維持原規則。

傷害集合亦接受正式目標名稱，例如 `to: {any_of: [enemy, {leader_of: enemy}]}`。同一個傷害指令同時處理被選從者與其控制者的領袖（BP19-035、BP19-111）；`leader_of` 沿用選取時保存的控制者，不能改用 owner。缺少必要的從者目標時整個能力取消，不會只對領袖造成傷害。

## クレスト公開計數

`count: {who: self, zone: ex, kind: crest}` 沿用公開區域計數，僅計入控制者 EX 區的クレスト；
普通 EX 卡與對手的クレスト不計入。可作為數值、比較條件或 `usable_if` 的運算元。
`crest` 僅擴充計數種類；搜尋、目標與歷史種類查詢仍維持原有範圍。
此入口不改變クレスト的建立、同名限制或 EX 區能力規則（綜合規則 9.1.4–5、10.3.6）。

## 表向資源計數與條件模式上限

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
這個入口不接受對手、裏向卡、未指定名稱、其他區域或額外篩選條件。

## 公開數值宣告與查詢上限

正式 `targets.n`／`upto` 及 `targets.from`、`search.sel` 的 `cost_le`／`orig_cost_le` 可使用既有 Number：自己的 PP 最大值、自己場上／EX／墓場的公開計數，以及這些值的 `plus`／`minus`／`times`／`per`／`min` 與 `define` 別名。計數接受現有常量種類、職業、特徵、精確卡名與原費篩選，並補上常量 `orig_cost_ge`；不授予隱藏手牌／牌庫、其他玩家、來源屬性、回條或時點覆寫讀值。公開上限不得與私下 Look 選取或 extremum 人口混用。

目標張數在準備正式問題時求值，負數依數量規則以零處理；精確張數同時固定上下限，`upto` 的下限保持零。結果存入既有選擇問題，不在付款或結算時重新讀取。搜尋的數值篩選則在搜尋指示開始、發現私有候選前固定，保留有其他條件時可找不到的規則；例如 BP04-098 用 `PP最大值 - 1` 表示嚴格較低的原費，BP07-091 用自己的 EX 卡數作上限。

正式 `choice.upto` 另接受 `per: {of: {count: ...}, unit: <正整數>}` 的公開自身計數，依完整單位向下取整（BP22-074 每十張原費二的墓場卡）。零組略過，結果仍限制在印刷模式數內，沿用不可重複及適正目標規則；精確 `choice.n`、替換上限與任意數量算式沒有因此開放。

自己的持續進化減費可用 `static.effects: [{evolve_cost_mod: {on: this, delta: {minus: [0, {count: {who: self, zone: field, kind: follower, other: true}}]}}}]`（BP06-019）。每次報價讀取當前有效能力與場上集合，與既有進化費用效果合計後才以零為下限；不安裝永久鎖或保存過時數量。既有 `apply.evolve_cost_mod` 仍限 `on: affected` 與常量修正。

模式內的單一公開目標亦可供 `leader_of` 保存選取時的控制者（BP18-046）。捕捉須屬於該目標自己的宣告、exact 1、角色為 controller，且目標選擇器本身保持獨立；其他角色及任意捕捉仍拒絕。

一般進化頭的 `evolve.into` 接受卡名、非空卡名清單，以及
`{who: self, zone: evolve_deck, kind: evolve_follower, name_contains: レーヴァテインドラゴン}`。
省略時沿用同名進化。每個頭的目的地、費用與 `usable_if` 分別檢查；雙面進化卡按回答的面序比對該面名稱，再沿共同程序公開、付款並進化。
不符合名稱、面序或該頭條件時，在公開與付款前拒絕。此入口不擴充效果本文的進化主體或目的地範圍。
