#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Nature Look and token scenario."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, bool); 2] = [
    ("nature", "自然の魔法", true),
    ("other", "機械の魔法", false),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, nature) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":"スペル",
                "traits":if nature {vec!["自然"]} else {vec!["機械"]},
                "cost":"9","power":"-","hp":"-","text":null,"sections":[]
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

fn fixture(number: &str, eligible: bool) -> Game {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"selected","card":if eligible {"nature"} else {"other"}},
        {"id":"tail","card":"nature"}
    ]);
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "top-bp17",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (control, ..) in CONTROLS {
        game.load_internal_card(control, &empty).unwrap();
    }
    game
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed private Nature choice")
    };
    question
}

fn play(game: &mut Game, eligible: bool, take: bool) {
    let question = awaiting(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("observed Nature domain")
    };
    assert_eq!(
        domain.members.len(),
        usize::from(eligible),
        "only an observed Nature card is eligible, never the unseen Nature tail"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the printed selection may be declined"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "only one card is observed and selectable"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the observing player sees the private choice"
    );
    if eligible {
        assert_eq!(
            domain.members[0].card.as_deref(),
            Some("nature"),
            "the nine-cost spell proves no kind or cost restriction"
        );
    }
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(if take {
                vec![domain.members[0].handle]
            } else {
                vec![]
            }),
        )
        .unwrap(),
    );
}

fn assert_result(game: &Game, take: bool) {
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["hand"],
        if take { json!(["selected"]) } else { json!([]) },
        "only an accepted Nature card joins the hand"
    );
    assert_eq!(
        view["P1"]["deck"],
        if take {
            json!(["tail"])
        } else {
            json!(["selected", "tail"])
        },
        "declining or finding a non-Nature card leaves it on top, without an unprinted bottom action"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 9_u8,
        "the real one-cost spell pays exactly one PP"
    );
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["source"]),
        "the actual spell finishes in the cemetery"
    );
    let ex = view["P1"]["ex"].as_array().unwrap();
    assert_eq!(
        ex.len(),
        1_usize,
        "the Great Tree is created even if no observed card was taken"
    );
    let tree = ex[0].as_str().unwrap();
    assert_eq!(
        view["objects"][tree]["name"], "ナテラの大樹",
        "the real admitted Great Tree token is created in EX"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert_eq!(
        known.iter().any(|card| card["id"] == "selected"),
        take,
        "Q2278: a declined observed card stays private; an accepted hand card is revealed"
    );
    assert!(
        !known.iter().any(|card| card["id"] == "tail"),
        "the unseen eligible tail remains private"
    );
    assert!(
        known.iter().any(|card| card["id"] == tree),
        "the created EX token is public"
    );
}

#[test]
fn bp17_117_and_p27_reveal_only_the_taken_nature_card_then_create_the_real_tree() {
    for number in ["BP17-117", "BP17-P27"] {
        for (eligible, take) in [(true, true), (true, false), (false, false)] {
            let mut game = fixture(number, eligible);
            play(&mut game, eligible, take);
            assert_result(&game, take);
        }
    }
}
