# 效果只從單一撰寫語法降低成有型別 IR

直接手寫 IR 會重複時點、結果與來源結構，另開任意腳本入口又會繞過相同檢查。效果採唯一 sve-author YAML 撰寫層，經巨集展開降低為有型別 IR，語法以 `dsl/` 的 JSON Schema 為唯一權威；新增能力須同步登錄及 capability，換取可追溯且不能靜默降級的執行契約。語法、解析邊界與型別分別見[撰寫規格](../dsl/author-syntax-1.0.md)及 [IR 規格](../dsl/ir-vocabulary-1.0.md)。
