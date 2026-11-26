use super::lowerer;
use crate::EngineFailure;
use crate::catalog::Catalog;
use crate::game::Game;
use crate::ir::{
    AbilityOrigin, Binding, CardProgram, Effect, KeywordId, ObjectOperand, PlayerRef, Polarity,
    WellFormedCardProgram, ZoneKind,
};
use alloc::sync::Arc;
use serde_json::{Value, json};
use std::{env::var_os, fs::read_to_string, sync::OnceLock};

#[path = "current_keywords/runtime.rs"]
mod runtime;

fn author(number: &str) -> Value {
    static SELECTION: OnceLock<Value> = OnceLock::new();
    let selection = SELECTION.get_or_init(|| {
        let raw = read_to_string(var_os("SVE_TEST_CURRENT_SELECTION").unwrap()).unwrap();
        serde_json::from_str(&raw).unwrap()
    });
    selection["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|author| author["input"]["card_no"] == number)
        .unwrap()
        .clone()
}

fn snapshot() -> String {
    read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap()
}

#[test]
fn runtime_rejects_changed_permission_headers_expansions_and_source_chains() {
    let input = author("BP01-138")["input"].clone();
    let lowerer = lowerer();
    let card = lowerer.lower_current(&input).unwrap();
    let original = raw_keyword_coordinates(card.program.card());
    let baseline = WellFormedCardProgram::check(original.clone()).unwrap();
    let unproven = raw_keyword_game()
        .load_internal_card("BP01-138", &baseline)
        .unwrap_err();
    assert!(
        matches!(unproven, EngineFailure::Unsupported(reason) if reason.contains("requires current declaration proof"))
    );
    let mut candidates = Vec::new();
    let mut zone = original.clone();
    zone.abilities[0].node.active_zones = vec![ZoneKind::Hand];
    candidates.push(zone);
    let mut holder = original.clone();
    holder.abilities[0].node.source.holder = ObjectOperand::Bound {
        binding: Binding::new(1, holder.abilities[0].node.scope),
    };
    candidates.push(holder);
    let mut controller = original.clone();
    controller.abilities[0].node.source.controller = PlayerRef::Owner {};
    candidates.push(controller);
    let mut missing = original.clone();
    missing.abilities[0].node.declarations.clear();
    candidates.push(missing);
    let mut extra = original.clone();
    extra.abilities[0]
        .node
        .declarations
        .push(original.abilities[0].node.declarations[0].clone());
    candidates.push(extra);
    let mut rule = original.clone();
    if let Effect::Permission { rule, .. } = &mut rule.abilities[0].node.declarations[0] {
        rule.0 = 0;
    }
    candidates.push(rule);
    let mut polarity = original.clone();
    if let Effect::Permission { polarity, .. } = &mut polarity.abilities[0].node.declarations[0] {
        *polarity = Polarity::Allow;
    }
    candidates.push(polarity);
    let mut nested = original.clone();
    if let AbilityOrigin::Keyword { origin, .. } = &mut nested.abilities[0].node.source.origin {
        **origin = original.abilities[1].node.source.origin.clone();
    }
    candidates.push(nested);
    let mut unsupported = original;
    if let AbilityOrigin::Keyword { keyword, .. } = &mut unsupported.abilities[0].node.source.origin
    {
        *keyword = KeywordId::Storm;
    }
    candidates.push(unsupported);
    for candidate in candidates {
        if let Ok(checked) = WellFormedCardProgram::check(candidate) {
            let failure = raw_keyword_game()
                .load_internal_card("BP01-138", &checked)
                .unwrap_err();
            assert!(
                matches!(failure, EngineFailure::Unsupported(reason) if reason.contains("whole-card ability"))
            );
        }
    }
}

fn raw_keyword_coordinates(card: &CardProgram) -> CardProgram {
    // This synthetic coordinate probe reaches the raw loader, without granting current proof.
    let mut card = card.clone();
    for ability in &mut card.abilities {
        ability.source.card_id.clone_from(&ability.source.card_no);
        ability.source.face = "base".into();
        ability.source.owner = "card".into();
        ability.source.field = "text".into();
        ability.source.fragment = None;
        if let AbilityOrigin::Keyword { origin, .. } = &mut ability.node.source.origin
            && let AbilityOrigin::Printed { face, .. } = origin.as_mut()
        {
            face.0 = "base".into();
        }
    }
    card
}

fn raw_keyword_game() -> Game {
    raw_keyword_game_for("BP01-138")
}

fn raw_keyword_game_for(number: &str) -> Game {
    let document = json!({"format":1_u8,"kind":"effect_set","cards":{
        number:{"status":"complete","abilities":[]}
    }})
    .to_string();
    let catalog = Catalog::from_documents(
        &snapshot(),
        "format: 1\nkind: keyword_registry\nkeywords: {}\n",
        &[("synthetic.yaml".into(), document)],
    )
    .unwrap();
    Game::new(
        Arc::new(catalog),
        &json!({}),
        &Value::Null,
        &Value::Null,
        "rejection",
    )
    .unwrap()
}
