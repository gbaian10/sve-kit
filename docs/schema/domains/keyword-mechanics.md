# 關鍵字登錄與機制投影

自身宣告（內部代號 H1）指「整行只有關鍵字」，表示卡片本身具有該能力。

本入口依[建置資料庫 §10](../build/build-db.md#10-dsl驗證與未實作卡片頁)與
[快照三態](../export/snapshot-format.md)提供查卡正標籤，不產生 DSL，不變更引擎支援狀態。

## 採納輸入

`authored/rules/keyword-registry.yaml` 是唯一入口；有界 YAML 讀取器拒絕 symlink、
未知欄位、非整數 format、重複 ID／code／term_id。形狀如下：

```yaml
format: 1
kind: keyword_registry
decision:
  id: decision:keyword-registry-v1
  state: confirmed
  note: Adopt the explicit existing keyword concepts for the manual H1 source adapter.
records:
  - {id: 'kw:ward', code: ward, term_id: 'term:keyword.ward'}
suppressions: []
```

records 明示永久 `kw:<code>` ID、英文 code 與既有 glossary term ID。
目前只登錄 `category=keyword, kind=printed_keyword`，definition 為 null、actions 空。
缺 term、非 keyword 類別或多重概念不能靠拼寫補配。日文名稱及多語 label 使用同一
已採納 glossary 概念的選詞與 low_confidence，不另維護譯名。保留 choice 的 origin、
source_id 與來源權威；已驗的官方數位詞典選詞為 official／digital_official，
不得改成 project／unofficial，也不得冒充實體 SVE 官文。品質合併 term／choice 的 low_confidence。

decision 是 mapping 與有限 H1 adapter 的明示採納，不是逐卡真人審核、DSL review
或 engine pass。建置保存 registry exact bytes 的 source_record、decision 及引用；
F1 configuration 納入 registry hash 與 adapter 版本。
此檔與舊 `authored/rules/keywords.yaml` 的 astra/1 原型是不同入口；後者不會被這條管線匯入。

## 來源與關係

只檢查已確認 card 的 JP face_current 主效果，逐面保留 exact owner。
雙面卡採「任一面有即標 has」：任一面證明自身宣告，就在 card 層級標記，
不表示正面一定具有該能力；公開投影沒有面向維度。
預設建置由已審查解析輸入取得來源版本及 locator；sealed/raw bytes 在產生解析資料及明示 `archive` 模式驗證，
不在普通建置重跑，見[解析卡片輸入〈建置入口〉](../ingest/parsed-card-inputs.md#建置入口)。更正沿用既有 corrected-source recipe。
H1 略過空主文；完整欄位 adapter 另驗來源的空值，不合併所有 printing、歷史 revision 或他 owner binding。

H1 只接受 `card_keywords.v2` 的完整 body part：整個片段由唯一解析的 keyword
宣告組成，葉皆為 Concept／keyword／GlossaryReference。SourceBinding 與來源位置重驗後，
每個已登錄概念產 `relation=has`。同欄位其他片段未翻譯或 pending 不妨礙這個正標籤，
卻仍阻止完整 coverage。不依翻譯 target、annotation、加粗或 HTML 圖示推論。

授予、選擇／數量參照、引號內能力、未知句型及 sections 均不由 H1 產生標籤。
token 定義的 keyword 不表示原卡自身具有。H1 不產 grants／refers／counts，
沒有標籤亦不代表沒有能力。

有限 grants adapter `manual-keywords-grants-bp01-v1` 只檢查首批固定永久 card ID 集合，
由 `mechanic_batch.FIRST_BATCH_CARDS` 承載。209 張來自固定解析輸入及 identity recipe 中
JP printing 原樣卡號以 `BP01-` 開頭、card 為 confirmed（排除 retired）的去重集合；
後續批次或新增 printing 不會自動擴張這份集合。執行時仍重驗 confirmed 與 JP current owner。
只使用既有 ward、storm、rush、bane、assail、intimidate、aura、drain、ambush、barrier 十個已登錄 ID，
依相同 glossary 的唯一日文拼寫匹配，不新增概念。

grants 只接受主文整行的有限句型：明示選擇自方從者、從墓場選擇並出場或產生具名 token 後，
「それは【keyword】を持つ。」（可有指定格式的攻防增加）；以及
「これがある限り、自分の『具名 token』すべては【keyword】を持つ。」。
前綴僅接受已列出的ファンファーレ、ラストワード或指定 PP／act 費用格式的起動；
尾句僅接受指定的任意數量 act 句。其他費用、條件、片段、引號能力及未知句型均不算已檢查。
含「」的主文整欄保守略過，避免跨行引述；sections（含 token_definition）不交給這個 producer，
token 原生 keyword 不會掛到母卡。主文明示額外授予生成 token 的 keyword 則屬母卡 grants。
來源 descriptor、current revision、exact field bytes 與凍結來源重驗後才交出正標籤，與翻譯無關。

有限 refers／counts adapter `manual-keywords-references-bp01-v1` 沿用相同 209 張永久 ID、
十個 keyword 及來源重驗。語意依用途分配：費用中依 keyword 選定捨棄的牌，以及依 keyword
選擇目標，屬 refers；明示符合 keyword 的從者數量門檻，屬 counts，不因同時提到 keyword 再加 refers。
固定捨棄一張牌是費用選擇，並非對 keyword 數量的統計。

目前只接受三種主文完整行（K 必須唯一對應已登錄日文名稱，N 為數字）：

- `{起動}{コストN}これを{アクト}墓場に置く：【K】を持つ自分のフォロワー1体を選ぶ。それは{攻撃力}+N/{体力}+Nする。` → refers。
- `{起動}{コストN}手札の【K】を持つフォロワー1枚を捨てる：1枚引く。` → refers。
- `{ファンファーレ}自分の場の【K】を持つフォロワーがN体以上なら、1枚引く。` → counts（門檻 N 至少 1）。

各 N 獨立，費用及攻防增量可為 0。不接受未知前後句、關鍵字字串命中或引號內能力；
含「」的主文整欄略過，sections 全不檢查，因此 token 的原生能力仍屬 token。
只在匹配行提交相應 relation 的局部檢查，`whole_field=false`；四關係都有 producer 仍不構成 complete。

完整欄位 adapter `manual-keywords-fields-bp01-v8` 沿用固定首批集合、confirmed JP current
及十個已登錄 keyword。每個面先驗 exact owner 主文與凍結來源的完整有序 section 清單；
來源與資料庫的 section 數量、順序或原文字節不符時，該面不提交完整檢查。
空主文須明驗凍結來源為 null 或空字串、已 intern 的原文確為空字串；空 section 也須驗自己的來源位置。
非空主文只接受整欄由下列已證明的 body part 及 layout 組成，至少須有一個 body：

- H1 的 `card_keywords.v2` 自身宣告：所有葉均為已登錄的 Concept／keyword／GlossaryReference，
  逐 part 重驗 binding 與來源座標。
- 固定 PP 進化整行：exact 原文為 `{進化}{コストN}：これは進化する。`，N 為一位以上 ASCII 數字。
  以單一連續 body span 的原文字節完整匹配，不依正標籤、翻譯或 normalizer 的語意辨識結果。
  這句只支付 PP 並讓自身進化，不宣告、授予、參照或計數這十個 keyword；不產正標籤。
- 固定傷害整行：`相手のフォロワー1体を選ぶ。それにNダメージ。`，N 為一位以上 ASCII 數字；
  目標也可為 `相手のリーダー1人か相手のフォロワー1体`，兩者的フォロワー前均可有 `場の`。
  亦接受 `フォロワーすべてにNダメージ。`，フォロワー前可有 `相手の`、`相手の場の` 或 `自分の他の`；
  `リーダーすべてにNダメージ。`（リーダー前可有 `相手の`）；以及 `自分のリーダーにNダメージ。`。
  傷害後可恰接一次固定 `N枚引く。`。
- 固定抽牌整行：`N枚引く。`。
- 固定主戰者體力回復整行：`自分のリーダーは{体力}+Nする。`，後可恰接一次 `N枚引く。`。

上述三組固定數值效果的各 N 獨立，均為一位以上 ASCII 數字。
行首只可無能力頭、`{ファンファーレ}`、`{ラストワード}`、`【進化時】`、
`自分のエンドフェイズが来たとき、`、`これが場を離れたとき、`、
`{起動}{コストN}これを{アクト}：` 或 `{起動}{コストN}これを{アクト}墓場に置く：`；
最後一種的 PP 費用可省略，但自身 act 與置入墓場不可省略。
這些能力頭、觸發時點、目標、固定 PP／自身 act／自身置入墓場費用與數值效果
均不宣告、授予、參照或計數十個 keyword，不產正標籤。
整行仍須以單一連續 body span 的 exact 原文完整匹配；不接受其他任意效果排列、
條件、其他費用、次數限制、未知能力頭或額外前後句。

另接受下列三組有限完整行；各 N 獨立，仍限一位以上 ASCII 數字：

- 固定從者增幅：增量為 `{攻撃力}+N`、`{攻撃力}+N/{体力}+N` 或 `{体力}+N`，
  後接 `する。`。對象限 `自分のフォロワーすべて`、`自分の他のフォロワーすべて`、
  `自分の{ロイヤル}フォロワーすべて`，或 `自分のフォロワー1体を選ぶ。それ`（フォロワー前可有 `他の`）；
  以 `は` 接增幅。只可無能力頭、fanfare、lastword 或自己 end phase 到來時點；
  選一體的分支可恰接一次 `N枚引く。`，全體分支不可。
  另接受 `自分の場に他のフォロワーが出たとき、これは` 及
  `自分の場に{ロイヤル}フォロワーが出たとき、それは` 接同樣增幅。
  雙區域分支只接受 fanfare 後 `自分の場の他の妖精・フォロワーすべてと、
  自分のEXエリアの妖精・フォロワーすべては` 接同樣增幅，原文在同一行，逗號後沒有換行。
  傷害與增幅串接只接受 `【進化時】相手のフォロワーすべてにNダメージ。自分の他のフォロワーすべては`
  接同樣增幅；不作任意句子組合。
- 有限條件自身增幅：`{ファンファーレ}【覚醒】状態なら、これは` 或
  `{ファンファーレ}【真紅】状態なら、これは` 接上述增幅；這兩個狀態不屬於十個 keyword。
  不接受其他條件、能力授予或額外尾句。
- 獨立 `{クイック}` body 行：只證明該行不涉及十個 keyword，其他 body 仍須各自完整證明。
  不接受與效果黏在同一行的形式，也不能藉此關閉未知效果。

以上各分支也須單一連續 body span fullmatch；royal 類別、妖精名稱及雙區域目標
均為有限 exact 原文，與 keyword 參照無關。token 名稱不授予 token 原生能力。

另接受三組有限卡片移動整行，仍須單一連續 body span 的 exact 原文 fullmatch：

- 有限移除只接受下列七種完整排列：
  `【進化時】アクト状態の相手のフォロワー1体を選ぶ。それを破壊する。`；
  `自分のフォロワー1体を選ぶ。それを破壊する。N枚引く。`；
  `場のフォロワーすべてを破壊する。`；
  `体力N以下の相手のフォロワー1体を選ぶ。それを消滅させる。`；
  `自分の場のカード1枚を選ぶ。それを破壊する。自分のリーダーは{体力}+Nする。N枚引く。`；
  `【進化時】相手のEXエリアのカード1枚を選ぶ。それを消滅させる。`；
  `相手の場のカード1枚を選ぶ。それを破壊する。`。
  不互換目標、區域、act 狀態、移除種類、能力頭及前後效果。
- 抽牌後移動手牌：`【攻撃時】` 或 `{ファンファーレ}` 後恰接
  `N枚引く。自分の手札N枚を捨てる。`；另接受 `{ファンファーレ}` 後恰接
  `N枚引く。自分の手札N枚をデッキの下に置く。`。各 N 獨立且限一位以上 ASCII 數字，
  有限移除的 N 亦同。不接受其他時點、任意手牌條件、其他費用或次數限制。
- 有限牌庫搜尋：`【進化時】自分のデッキから` 後只接受
  `{進化}を持つフォロワー` 或 `アミュレット`；
  `{ファンファーレ}自分のデッキから` 後只接受 `{ロイヤル}フォロワー`、`忍者・カード` 或 `スペル`。
  兩分支皆恰接 `1枚を探し、手札に加える。`，不改變數量或目的地。
  進化、royal 類別、忍者名稱與卡片種類均不屬於十個 keyword；沒有任意 keyword 篩選。

這三組只證明十個 keyword 的四種關係均不涉及，不產正標籤；獨立 quick、進化、
自身 keyword 宣告、其他面與 sections 仍須各自證明，不能由已證明的移動行補滿。

另接受三組有限觸發、墓場移動與 act／移除整行；各 N 獨立且限一位以上 ASCII 數字，
仍須單一連續 body span 的 exact 原文 fullmatch：

- 有限觸發傷害只接受下列排列：
  `これが場から手札に戻ったとき、相手のフォロワー1体を選ぶ。それにNダメージ。`；
  `自分の場に他のフォロワーが出たとき、相手のフォロワー1体を選ぶ。それにNダメージ。`
  （此分支目標亦可恰為 `2体まで選ぶ`）；
  `自分の妖精・フォロワーが攻撃するとき、相手のリーダー1人か相手のフォロワー1体を選ぶ。それにNダメージ。`；
  `相手のフォロワーが破壊されたとき、それのリーダーにNダメージ。自分のリーダーは{体力}+Nする。`。
  觸發事件、妖精名稱、傷害目標與最後分支的回復順序均固定，不互換能力頭或目標。
- 有限墓場移動只接受下列排列：
  `{起動}これを{アクト}墓場に置く：自分の墓場のコストN以上のフォロワー1枚を選ぶ。それを手札に加える。`；
  `{ファンファーレ}自分の墓場のフォロワー2枚まで選ぶ。それを手札に加える。`；
  `自分の墓場のコストN以下のアミュレット3枚まで選ぶ。それを場に出す。`；
  `{ファンファーレ}自分の墓場のコストN以下のアミュレット1枚を選ぶ。それを場に出す。`；
  `{ファンファーレ}自分のデッキの上N枚を墓場に置く。`。
  數量（除牌庫頂分支外）、區域、費用上下限方向、卡片種類、回手或進場與起動費用均固定；
  `コスト` 不擴張為 `元のコスト`，也不接受任意 keyword 篩選或移動後授予能力。
- 有限 act／移除只接受下列排列：
  `{ファンファーレ}` 或 `{起動}これを{アクト}：` 後恰接
  `相手のフォロワー1体を選ぶ。それをアクトする。`；
  `これはアクト状態で場に出る。`；
  `{起動}これを{アクト}墓場に置く：相手のフォロワー1体を選ぶ。それを破壊する。`；
  `自分のエンドフェイズが来たとき、これは消滅する。`；
  `【進化時】フォロワー1体を墓場に置く：相手のフォロワー1体を選ぶ。それを破壊する。`。
  自身進場狀態、觸發時點、費用與移除種類各自固定，不接受額外 PP 或其他犧牲條件。

這三組不宣告、授予、參照或計數十個 keyword，不新增正標籤；既有自身宣告、
固定 PP 進化、其他 body、面與 sections 仍須各自證明。token 原生能力仍屬於 token。

另接受三組有限條件、狀態規則與已有正標籤的授予整行；各 N 獨立且限一位以上 ASCII 數字，
仍須單一連續 body span 的 exact 原文 fullmatch：

- 條件固定效果只接受：
  `【進化時】【覚醒】状態なら、N枚引く。`；
  `相手のフォロワー1体を選ぶ。それにNダメージ。【覚醒】状態なら、N枚引く。`；
  `フォロワーすべてにNダメージ。【覚醒】状態なら、代わりにNダメージ。`；
  `{ファンファーレ}相手のフォロワー1体を選ぶ。それにNダメージ。【コンボ_N】代わりにNダメージ。`；
  `相手のフォロワー1体を選ぶ。それにNダメージ。【スペルチェイン_N】代わりにNダメージ。`
  （只此分支可帶 fanfare，且可恰接一次 `【SC_N】それのリーダーにNダメージ。`）；
  `{ファンファーレ}自分の場にアミュレットがあるなら、自分のリーダーは{体力}+Nする。`。
  覚醒、コンボ、スペルチェイン／SC 及 amulet 存在條件不涉及十個 keyword；
  不擴張其他條件、任意 SC 別名排列、目標、能力頭或 reminder。
- 有限狀態規則只接受：`これが受けるダメージを-Nする。`；
  `相手のターン中、これが与えるダメージを+Nする。`；`これは相手を攻撃できない。`；
  `{ラストワード}自分のPP最大値を+Nする。`；
  `{進化}リーダーの{体力}-N：これは進化する。`；
  `【攻撃時】フォロワーへの攻撃なら、これは{攻撃力}+Nする。`；
  `自分のエンドフェイズが来たとき、これは{体力}+Nする。`。
  傷害方向與正負、回合、PP 最大值、支付主戰者體力及攻擊對象各自固定，均不涉及十個 keyword。
- 有限授予只接受 `{ファンファーレ}自分の他のフォロワー1体を選ぶ。` 或
  `自分の墓場のコストN以下のフォロワー1枚を選ぶ。それを場に出す。` 後恰接
  `それは【K】を持つ。`，`それは` 後可有 `{攻撃力}+N/{体力}+Nして、`。
  K 必須唯一匹配已登錄十個 keyword 的日文名稱，不能因正標籤已存在就略過未知效果。
  整行可完整證明 grants，其他三種關係均不涉及；正標籤仍由有限 grants adapter 產生，
  完整欄位 adapter 不新增正標籤。自身條件授予、其他選擇／目的地、未知或歧義 K 仍留缺口。

這三組仍須逐行、逐面及逐 section 證明，不能關閉未知前後句；
獨立 quick、H1 自身宣告、其他能力與來源閉包的原有條件不變。

任何其他未證明句型、pending、reminder 或引號能力
都留下缺口，不能以另一個正標籤補滿；固定句本身可獨立證明，不要求翻譯已實作。
非空 sections 全數保留缺口，包含 token_definition；不把 token 原生 keyword 掛到母卡。

已證明的空欄位與上述完整主文，對十個 keyword 的 has／grants／refers／counts 各提交
`whole_field=true`，自身 has 正標籤仍只由 H1 產生。缺 current 的面不提交檢查，也不從必要範圍刪除。
因此同卡另一面或任一 section 未證明時仍為 partial；沒有任何檢查的 keyword 維持 unknown。

## 新鮮度與補正

`mechanic-source-bundle-v1` 對 card／shared 全部面（含缺 current 的面）、
所選 revision／來源、current basis、主文及有序 section kind／exact hash 計算 source_text_hash。
其他面、補充段落或 basis 變動因此使舊標籤失效；bundle 本身不是完整語意檢查證明。

producer_version 包含 `manual-keywords-h1-v1`、來源 normalizer／葉規則、keyword mapping
與 keyword 概念集合，不含中文呈現、無關 commit 或 suppression。
grants producer_version 另含自身 adapter、固定 card 集合及十個已登錄概念的 mapping／日文名稱；
refers／counts 的 producer_version 同樣釘自身 adapter、固定 card 集合及 keyword mapping；
F1 configuration 也釘兩個有限 adapter 與固定 card 集合 hash。
完整欄位 producer_version 另釘自身 adapter、固定 card 集合及 H1 producer_version（含 normalizer、
葉規則與完整 keyword mapping）；F1 configuration 同時釘完整欄位 adapter。
抽取或 bundle 演算法變更必須更新 adapter／recipe 版本；keyword/action universe 與 coverage 每次重建。

card_mechanic 使用 `source_kind=manual, dsl_id=null`，source_id 指實際原文，
decision_id 指採納的 mapping。候選 ID 由精確 binding、card／keyword／scope、
source hash 與 producer 決定。fresh 正標籤依 `(card_id,keyword_id,scope)` 合併，
relations／actions 排序去重，不公開原始證據或 unknown／absent 列。

選填 suppressions 每筆必有 `id,target_tag_id,source_text_hash,producer_version,reason,decision`；
decision 同上例且必須 confirmed。只有候選 ID、source hash 及 producer 完全相符才寫入並套用；
不相符報 stale_suppression，不遮蔽新候選，不把 partial 改 complete。
補正使用獨立的 mechanic_suppression decision category；只有實際寫入 override
時才保存其 decision 與來源，不為 stale suppression 留下未使用的 confirmed decision。

## Coverage 與承載

H1 提交已驗正標籤及自身宣告所在面／主文的局部 has 檢查，由共用 coverage reducer 合併；
不把局部宣告當整欄位已查。內部範圍與 producer 鍵見[建置資料庫 §10](../build/build-db.md#10-dsl驗證與未實作卡片頁)。
H1、grants、refers／counts 與完整欄位檢查均儲存後，才在共同編排點套用 suppression，並傳入全部有效 producer_version 投影一次。
grants 只提交所在面／主文的局部 grants 檢查；has＋grants 也不能補滿 refers／counts 或 sections。
公開每 card／shared 一列，只有所有必要範圍均已完整檢查的 keyword 才進 complete 集合；
全部 keyword 都完整時使用 complete_all。complete／partial 選較短 include／exclude，同長選 include；只投影 fresh coverage。
has present 不證 grants absent；缺標籤且 partial／缺 coverage 仍 unknown。
reducer 重驗內部 state 與 checked_ranges 一致，不能用手填 complete 冒充完整檢查結果。

目前不產 en_override，不授予 shared 跨區資格；reader 沿既有 aligned／region block 條件，
不能因 JP 繁中可顯示就套用 EN 機制。分母依 manifest 全地區 non-retired，含 provisional。

公開 keyword／mechanic_projection／card_mechanic_coverage 使用既有欄位與分片配置；
前者在共用 catalog，後兩者在 card 原有 engine 檔案。不改格式、capabilities、band width，
不新增邏輯分片。report.mechanics 保存 adapter／producer、證據／投影／coverage 列數及
拒絕原因，不含官方句子。建置 keyword 子能力包含七張機制表，DSL 仍未啟用。
