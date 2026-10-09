# 引擎規則能力與資料身分 engine-rules/1

`engine-rules/1` 是可執行規則設定，不是另一份卡表、構築合法性結果或採納決定。
語法唯一權威為 [engine-rules.schema.json](../../dsl/engine-rules.schema.json)；
效果語法仍為 [effects.schema.json](../../dsl/effects.schema.json) 的 `astra/1`。
通過 schema 不代表執行器已支援、真實綁定正確或資料已通過來源採納。

## 1. 封套與版本背景

檔案位置是 `authored/rules/engine/index.yaml`，首版一個檔案，不另設分片或新的採納入口。
依 [LICENSING](../../LICENSING.md)，本專案在此路徑的貢獻採 Apache-2.0；官方及第三方內容仍排除。
公開設定只記代碼、身分參照、卡號定位器、來源版本及核對紀錄，不複製官方名稱或引文。

| 欄位 | 內容與限制 |
| --- | --- |
| `format` | 恰為整數 1 |
| `kind` | 恰為 `engine_rules`；原 engine-rules/1 設定語意不變 |
| `scope` | `region` 僅 jp／en，`rules_version` 保留當區官版字串，`rules_source_version_id` 指向該官版的正式凍結來源 |
| `input` | `kind` 僅 legacy-jp／resolved；`snapshot_sha256` 是完整輸入 exact bytes 的 64 位小寫 hex |
| `titles` | 作品代碼、定位 face、有限能力及 evidence ID；不收顯示名稱欄 |
| `resources` | 有限角色、規則名稱身分參照、必要 template 及 evidence ID |
| `evidence` | 核對紀錄的精確全集，不能以 schema valid 冒充人工確認或採納 |

所有 object 禁止未知欄位。陣列不能重複同一完整物件；業務鍵的唯一性仍由載入器檢查，
不同內容使用同一 title code／role／evidence ID 也必須拒絕，不能靠 `uniqueItems` 當作已驗證。
輸入 SHA、規則設定 exact bytes 摘要及 authored／schema 版本共同釘入 Catalog 背景；
載入與 resume 不得悄悄改用最新卡表或設定。

## 2. 身分參照及明示 legacy-jp 轉接

`faceRef` 恰選其一：

- `legacy-face`：`kind`、`region`、原樣 `card_no`、零基 `face_ordinal`。這是定位器，不是跨地區永久 ID。
- `face-id`：`kind`、`region`、既有永久 `face_id`；不得由顯示名稱臨時造 ID。

角色的 `names` 至少一筆，每筆是：

- `legacy-name`：`kind`、`face` 與 `name_source`。`printed` 使用該面印刷名稱；
  `rules` 使用該面的 program `rules_name` 覆寫，沒有覆寫時才沿用印刷名稱。
- `rules-name-id`：`kind`、`region` 與已採納的 `rules_name_id`。

Catalog 必須驗證參照存在、與 scope 同區、沒有歧義且在輸入背景內；角色名稱可涵蓋
經資料確認的多個身分。正式資料直接沿用既有 card／face／rules_name／title 登錄，
不能把舊卡號定位器或私有名稱的 hash 冒稱永久身分。

舊無 region 的 JP JSONL 只經明示 `legacy-jp` 載入邊界使用；該模式的 scope 必為 jp，
所有 locator 亦須同區。名稱與 title label 從同一份釘版私有輸入解析，
不由 EN 後綴去除、卡包、拼字或名稱相似性推導跨區配對。
`resolved` 模式接受同批正式身分投影；兩種模式皆須驗 SHA 與引用背景，不互相猜測。

作品文字的舊題庫轉接以 title 的 `anchor` face 建立私有 label ↔ title code 索引，
同一 label 映到多個 code 應拒絕。未解析、缺採納或跨區資料不能先當正式登錄。
新私有轉接檔若有必要，須先在私有 testdata 核對及釘版，再另更新公開鎖定清單；
公開設定及合成測試不保存原值。

## 3. 作品能力與 token 模板

每個 title 具有 `title_code`、`anchor`、`capabilities`、`evidence`。title code 使用
已採納的作品詞彙；不能把顯示文字換到 enum、英文譯名或名稱 hash 後繼續比較。
`capabilities` 可明示空陣列，表示已登錄且沒有特殊能力；未登錄 title 不等同空能力。

能力恰為以下三種 object：

| `kind` | 其餘欄位 | 用途 |
| --- | --- | --- |
| `opening_start_amulet` | 無 | 開局起始護符選擇 |
| `opening_ex_resource` | `resource_role: lesson_item`、`count: 5`、`zone: ex`，均固定 | 建立五個開局 EX 資源 |
| `ub_enabled` | 無 | 作品構築的 UB gate |

同 kind 重複即拒絕。能力只能在 `construction == title` 且該 title code 有綁定時使用，
class／crossover 不因帶了 code 就獲得能力；玩家輸入不能自帶能力布林。
若同時提供 code 與舊文字，必須解析一致；矛盾不以任何一方優先。
這些背景不證明整副牌合法，也不能藉改造增加共享題庫未承諾的整副牌檢查。

角色僅 `lesson_item`、`meal_item`、`drive_point`、`stack_base`，
不是 keyword，也不是 carddb 的整卡 `construction_role`。
lesson_item／stack_base 必有 `template` face；其餘角色可只用名稱身分匹配。
模板必須是同區已存在的 token face，有完整可執行 program，名稱身分符合角色。
partial／rejected program、錯型別／面序、歧義或版本背景不一致皆拒絕。
指定模板不取消一般 create 的同名多 printing／token_template 解歧義。

## 4. 兩種名稱語意必須分開

### 4.1 費用 selector：有效名稱集合含 alias

selector 的 `zone` 與 `from` object 分支可加 `resource_role`，
沿既有 side、zone、type、條件、表裏面及數量等篩選共同求值。
若 selector 同時指定 `name` 與 `resource_role`，兩者以 AND／交集求值，必須同時滿足；
不得讓其中一項取代或放寬另一項。
角色匹配是「物件目前有效名稱身分集合」與「角色綁定身分」相交，
不是僅允許某卡號的靜態白名單。有效集合包含規則名稱、有效 zone 的 name_alias、
rules_name 覆寫及目前 information_source／面切換；額外名稱離開適用 zone 後失效。
一般 name／not_name／name_contains 等效果保留原語意。

`link_resource` 的 `name` 與 `resource_role` 恰選其一；subjects、count、from_zone、to 保留。
舊 name 形式在私有身分邊界解析，未知或多重角色拒絕，不能以字串猜特殊資源。
將顯式 ride 宣告正規化成內部 drive 費用，仍須同時符合：

1. 解析為 drive_point，且只有一份相符宣告。
2. subjects 恰為 self。
3. count 恰為 1。
4. from_zone 恰為 evolve_deck。
5. to 恰為 drive。

沒有顯式宣告才產生隱含費用；重複、缺欄、歧義或不支援形狀不能默默付一次。
`_drive_point` 仍是內部展開產物，不提供外部自由 opcode。
費用選項、可支付性與實際支付必須共用角色求值，取消／失敗不能留下部分支付。

### 4.2 banished 計數：單一規則名稱，不含 alias

EX 移到 banish 的 lesson_item 計數，只查既有 card_name 語意取得的單一身分：
先依 information_source 取資訊來源，以 program 的 rules_name 覆寫或該面印刷名稱解析，
不合併 name_alias，即使其 zone 此刻有效。不能與 §4.1 共用預設包含 alias 的 predicate。

判斷在物件移動及 knowledge 更新後、token 消去前，來源區用 previous.zone，
目的區用 destination，歸屬用 previous.controller。其他來源／目的區、非角色或
只有 alias 命中的物件不計；rules_name 覆寫及 information_source 轉接會影響單一名稱。
計數鍵仍為 controller 的 magic_item_banished，既有 authored 效果讀取保持相同。

## 5. 建立、還原與公開邊界

資源物件以角色的已驗證 TemplateRef 選模板，再經既有 new_token_object 流程建立；
不能跳過 ID 分配、容量、counter、入場 trigger、區域搬移與 token 消去。
stack 的 amount ≤ 0 不建立；已有 recipient 仍須選擇，沒有才由角色模板建立。
開局缺必要角色／模板不得留下部分建局。

磁碟與記憶體載入共用驗證；記憶體入口顯式收身分投影與規則文件，不能暗讀本機設定。
一般合成 Catalog 可明示沒有特殊能力，遇到需角色但缺綁定時回 Unsupported，
不能使用官方名稱常數作預設。凍結 v2.1 題庫的 title 文字由 engine adapter
透過同批私有索引轉接；中立題庫及 scenario-runner 不加入 engine 特有語法。

仍使用原型 astra-save/1；本契約不承諾不同二進位版本的舊存檔可續玩。
必要的 resolved binding／title code 隨 Catalog／Game 存檔，缺欄必須反序列化失敗，
不得以 serde(default) 補空表或重讀最新設定。相同版本的 save／restore／branch
須保留開局 queue、合法選項、事件、next_object 與隱藏資訊邊界。
正式 save/2、跨版遷移及 server 的房間能力授權另有契約才可提供。

view 可顯示已知的私有標籤，但不投影 Catalog 完整索引或可追蹤對手未知牌的身分。

## 6. evidence 與驗證層次

每筆 evidence 包含 id、source_version_id（既有 src:v1: 的完整內容身分）、
rule_refs（原版條號）、可選 qa_refs（整數編號）、checked_on。
scope 的 CR 來源與所有 evidence 皆須能驗回封存來源版本，且與實際規則相關。
bindings 的 evidence ID 恰連到本封套，不能只寫無關來源當成有核對。
checked_on 是 ISO 8601 的日期；不存 checked_by 這類流程紀錄，來源與條號仍需核對。
來源封存、資料採納與引擎執行器能力各自驗證，不互相替代。

合成層須證明名稱一致重命名不改規則，錯身分／能力不能靠同樣外觀取得特權；
檢查各種角色、zone alias、單一名稱計數、顯式／隱含 ride、模板錯配與還原。
私有整合層另外核對真實參照、模板與既有題庫結果。fork 的 excluded 模式
只驗合成層，摘要明示私有整合未執行；required 模式必取得並驗證釘版私有資料。
兩層與各自覆蓋率閘門不可互相冒稱。

## 7. 合成格式範例

下列只有合成定位器及 hash，用來說明格式，不能當真實 CR、採納或可執行綁定：

```yaml
format: 1
kind: engine_rules
scope:
  region: jp
  rules_version: synthetic-v1
  rules_source_version_id: src:v1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
input:
  kind: legacy-jp
  snapshot_sha256: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
titles:
  - title_code: synthetic_title
    anchor:
      kind: legacy-face
      region: jp
      card_no: SYN-001
      face_ordinal: 0
    capabilities:
      - kind: ub_enabled
    evidence: [synthetic_review]
resources: []
evidence:
  - id: synthetic_review
    source_version_id: src:v1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    rule_refs: ['1.2.3']
    checked_on: '2026-10-03'
```

實際資料必須替換為釘版真實輸入、已登錄 code 與能驗回的來源；
範例省略角色，不能用於需要資源角色的局面。

## 選項內追加費用（R-0006）

2026-10-09 官方確認採路線 B。`additional_costs` 可列多組，以 `modes` 限定所屬選項，本文透過 `paid.<key>` 決定效果是否適用。一組費用以 `optional_costs.additional` 回答；多組以 `optional_costs.<key>` 分別回答（`decline` 或付款材料），`key` 寫成 `mode_<選項編號>`，對應情境契約 §9.10。各組在打出時決定並合計檢查，付款事件先於 play，付不起取消打出；效果仍依卡面順序執行。
