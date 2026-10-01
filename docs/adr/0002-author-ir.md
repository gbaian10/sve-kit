# ADR-0002：單一撰寫層降低為有型別 IR

狀態：已採用（2026-09-28）。規則版本：CR 1.27.0。

## 背景

BP09-SL05 的進化時效果同時建立兩種 token；SD03-016 則是選一種。兩者若都寫成任意字串陣列，下游只能猜「と」和「か」。CR 5.18 的チョイス又要求打出時選模式，不能與結算中的替代選擇混用。BP08-011 的 Q1060／Q1061 明確區分必選兩個目標與最後兩種 token 各一張，說明短語法仍必須保留型別與時點。

能力授予與單純安裝效果也不能靠相同鍵名混過。CP04-PR22 的裝備時數值修正，Q2540 證明失去能力後修正仍在；若降低成授予 static 能力，結果就不同。CR 10.9、10.12 及 14.5.2 需要能力持有者、文字提供者和效果來源可分辨。

## 決定

唯一手寫入口為 sve-author/1.0 YAML。流程為語法檢查、巨集展開、有型別 IR、引擎 capability 檢查。正式 JSON Schema 位於 dsl，語法只能由一個版本權威定義；本 ADR 不宣告 Schema 已實作。

每層保留卡號、面、section、line、clause 來源。拒絕重複鍵、未知欄位／enum、缺 binding、跨分支局部引用、非法 capture 和事件 payload。沒有 custom_effect、直接 op 或通用自由 JSON path。A.\* 表示撰寫構造，I／K／L 等是 IR 元素，穩定 ID 不重用給不同語義。

create 列表表示每個名稱各 n，either 表示結算時擇一，choice 表示規則チョイス。apply 降為 I.install-effect，不建立 AbilityInstance；grant.abilities 則建立可追溯的授予來源。inline 與巨集必須能比較降低結果，巨集不能增加輸入點或改時點。

### YAML 解析邊界

格式固定 YAML 1.2 core schema。on／n 是字串鍵，yes／no 是字串值；mapping 不接受布林或其他非字串鍵。先拒絕重複鍵、舊版本指示與非 core tag，再依構造登錄表檢查鍵，不能先轉一般 map 而丟掉重複資訊。設計抽驗時以 PyYAML 往返曾將 13 個樣本的 on 變成 true，證明純解析成功不足以驗收。

Python DSL 工具採 ruamel.yaml 純 Python safe 路線，固定 1.2 並關閉重複鍵容忍；另限定 core scalar resolver，避免隱式 timestamp 等擴充。2026-10-01（#140）：`carddb` 的 authored 讀取改採 PyYAML `CSafeLoader` 的 libyaml 事件串流，單趟解析與嚴格語法檢查，沿用原有 1.2 core 純量 resolver 並拒絕重複鍵；不使用 PyYAML 預設 1.1 resolver，缺 C 擴充即明確失敗。ruamel 寫出與後續 strict JSON／canonical 雜湊不變。官方文件指出 pure=True 可避免 C loader 的解析差異。[ruamel.yaml 用法](https://yaml.dev/doc/ruamel.yaml/basicuse/)

Rust 選 **saphyr** 作 1.2 解析前端：官方文件明列 YAML 1.2 與 core schema scalar 支援；載入邊界仍須自行拒絕非字串鍵、未知 tag／BadValue、舊版本及重複鍵，不能把函式庫支援格式等同 DSL 合法。本 ADR 只定選型、不新增 Cargo 依賴；接入時釘實際版本並在建 map 前驗證事件／節點，跑與 Python 相同的金絲雀及負例。未完成前不得宣告 Rust 載入器通過。[saphyr 官方文件](https://docs.rs/saphyr/0.1.0/saphyr/)

測試包含 on／n 原樣、yes／no 保持字串、true 值為布林；拒絕布林鍵與字串 true 鍵、重複 on、未知欄位、cannot.draw.n 及 stat_changed 歷史舊別名。YAML 原始文字作證據保存，不由 load→dump 產生可供反向翻譯的來源。

## 考慮過的選項

| 選項                              | 為何不採／採用代價                                                                                                                          |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| YAML 1.1 或只用全字串 BaseLoader  | 前者將 on 改為布林，後者連數字與 true 值也失去型別；兩者都不能滿足 core schema。採明確版本／resolver 與反例測試，不靠所有作者替 on 加引號。 |
| 直接手寫 IR                       | 可精確表達，但每張卡都重複 execution、receipt、來源等結構；審核者容易只對節點而漏看卡文意義。IR 留作編譯產物。                              |
| 短寫與任意 op 長寫並存            | 長寫繞過相同驗證，兩種輸入可能對 BP08-011 得到不同分配時點；維護者須維護兩套語法相容承諾。                                                  |
| 未知欄位忽略或以缺值預設處理      | 拼錯 until 或 next_only 會悄悄改整場效果，且不能用 CR 1.3.6 的「本來沒有資訊」替程式錯字辯護。                                              |
| 一套短寫與型別化 lowering（採用） | 新欄位需同步 Schema、型別、lowering、capability 與文件；代價可換取局部修改不破壞整張卡的可追溯性。                                          |

## 後果

大部分語法缺口只增加欄位或封閉變體，不增加任意執行器。item／as 是局部詞法 binding，巢狀必明確命名，不容許隱式遮蔽。move、search、create、transform 共用 entry_effects 型別，使新物件在入場掃描前得到效果。

至少要以 CP04-PR22 測能力移除後效果仍在，以 BP08-011 測分配總量，以 SD03-016 測結算分支。YAML 可解析僅是第一關，不能寫成 Schema 或語義驗收。

## 未決事項

JSON Schema 與 Rust enum 的生成方向、診斷格式及 source span 壓縮策略待實作。正式語法不可引用實作尚未支援的欄位而默默降級。
