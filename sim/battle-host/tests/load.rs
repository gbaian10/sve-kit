//! Session loading failures on the smallest inputs that reach each stage.

use serde_json::json;
use sve_battle_host::load::{Inputs, LoadError, load};

const KEYWORDS: &str = r#"{"format":1,"kind":"keyword_registry","keywords":{}}"#;
const EMPTY_SELECTION: &str = r#"{"documents":[],"authors":[]}"#;

fn setup(card: &str) -> String {
    let deck = json!({"construction":{"kind":"class","value":"ニュートラル"},
        "main":vec![card; 8],"evolve":[]});
    json!({"decks":[deck, deck],"first_chooser":"p1","seed":"load","control":["player","policy"],
        "max_submissions":10_u8})
    .to_string()
}

fn error(selection: &str, rules: &str, setup: &str) -> Option<LoadError> {
    let inputs = Inputs {
        snapshot: "",
        keywords: KEYWORDS,
        selection,
        rules,
    };
    load(inputs, setup).err()
}

#[test]
fn format_and_selection_failures_report_their_own_codes() {
    assert_eq!(error(EMPTY_SELECTION, "{}", "{}"), Some(LoadError::Format));
    assert_eq!(error("[", "{}", &setup("x")), Some(LoadError::Format));
    // Rule anchors name snapshot cards, so the match stage needs real inputs (session.rs).
    assert_eq!(
        error(EMPTY_SELECTION, "{}", &setup("x")),
        Some(LoadError::Selection)
    );
    assert_eq!(
        [LoadError::Format, LoadError::Selection, LoadError::Match].map(LoadError::code),
        ["format", "selection", "match"]
    );
}
