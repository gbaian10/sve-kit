#![expect(
    clippy::panic,
    reason = "Unexpected decisions indicate a broken card fixture."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const CONTROLS: [(&str, &str, bool, bool); 4] = [
    ("nao-good", "仲間・神谷奈緒", false, false),
    ("nao-spell", "神谷奈緒の魔法", false, true),
    ("other", "別の仲間", false, false),
    ("cool", "クールな仲間", true, false),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, cool, spell) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":if spell {"スペル"} else {"フォロワー"},
                "traits":if cool {vec!["クール"]} else {vec![]},"cost":"0",
                "power":if spell {"-"} else {"1"},"hp":if spell {"-"} else {"3"},"text":null,"sections":[]
            }]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        Catalog::from_current_selection_with_rules(&snapshot, &keywords, &input, &rules.to_string(), &EngineIdentityInput::LegacyJp).unwrap()
    })
}

fn fixture() -> Game {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"ECP02-025"},{"id":"cool","card":"cool"}
    ]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        CONTROLS
            .iter()
            .filter(|(number, ..)| *number != "cool")
            .map(|(number, ..)| json!({"id":number,"card":number}))
            .collect::<Vec<_>>()
    );
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"noncool","card":"other"}]);
    state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"cool"}]);
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "ecp02-search",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

fn search_nao(game: &mut Game) {
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real amulet enters the field"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed fanfare")
    };
    let trigger = options
        .iter()
        .find(|option| option.card == "ECP02-025" && option.line == 1)
        .unwrap()
        .handle;
    let CardPlay::Awaiting(q) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(trigger))
        .unwrap()
    else {
        panic!("printed search")
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("Nao follower domain")
    };
    assert_eq!(
        domain
            .cards
            .iter()
            .map(|card| card.card.as_str())
            .collect::<Vec<_>>(),
        vec!["nao-good"],
        "only a follower whose name contains Nao matches, not a matching spell or unrelated follower"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "the fanfare searches one follower"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert!(
        view["P1"]["hand"]
            .as_array()
            .unwrap()
            .contains(&json!("nao-good")),
        "the found follower reaches hand"
    );
    assert_eq!(
        view["P1"]["deck_count"], 2_u8,
        "only the selected follower leaves the deck"
    );
    game.resume_internal_card().unwrap();
}

fn grant_cool(game: &mut Game) {
    let ability = prepared().current["ECP02-025"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("printed Cool target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("Cool follower domain")
    };
    assert_eq!(
        domain
            .members
            .iter()
            .map(|member| member.card.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("cool")],
        "only an allied Cool follower is eligible, excluding the other trait and opposing Cool follower"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["source"]),
        "the amulet is sacrificed for the activation"
    );
    assert!(
        !view["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "the paid source leaves the field"
    );
    let keywords = view["objects"]["cool"]["keywords"].as_array().unwrap();
    assert!(
        keywords.contains(&json!("突進")) && keywords.contains(&json!("指定攻撃")),
        "the real activated ability grants both printed keywords"
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn ecp02_025_searches_nao_then_sacrifices_to_enable_a_cool_follower_attack() {
    let mut game = fixture();
    search_nao(&mut game);
    assert!(
        matches!(
            game.play_internal_card("cool", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "a newly played follower needs Rush"
    );
    game.resume_internal_card().unwrap();
    let attack = json!({"do":"attack","attacker":"cool","target":"enemy"});
    assert_eq!(
        game.internal_rule_action(&attack).unwrap().outcome,
        "cannot-attack",
        "before the grant a fresh follower cannot attack the standing enemy"
    );
    grant_cool(&mut game);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"cool","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "Rush does not permit an immediate leader attack"
    );
    assert_eq!(
        game.internal_rule_action(&attack).unwrap().outcome,
        "resolved",
        "Rush and Assail together permit the new follower to attack a standing enemy"
    );
}
