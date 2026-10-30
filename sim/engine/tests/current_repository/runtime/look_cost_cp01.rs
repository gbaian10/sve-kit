#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed amulet and Look scenario."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, u8); 3] = [
    ("eight-spell", "八費の魔法", "スペル", 8),
    ("seven-follower", "七費の仲間", "フォロワー", 7),
    ("enemy", "相手の仲間", "フォロワー", 1),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":[],
                "cost":cost.to_string(),"power":if follower {"1"} else {"-"},
                "hp":if follower {"7"} else {"-"},"text":null,"sections":[]
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

fn fixture(candidate: &str, cost: u8) -> Game {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP01-073"}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top","card":candidate,"state":{"cost":cost}},
        {"id":"tail","card":"eight-spell","state":{"cost":7_u8}}
    ]);
    state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"enemy"}]);
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        &state,
        &Value::Null,
        &Value::Null,
        "look-cp01",
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
        panic!("printed choice")
    };
    question
}

fn play_and_destroy(game: &mut Game) {
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real four-cost amulet enters play"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("real fanfare is pending")
    };
    let handle = options
        .iter()
        .find(|option| option.card == "CP01-073")
        .unwrap()
        .handle;
    let question = awaiting(
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("fanfare declares an opposing follower")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "the opposing follower is the unique formal target"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("enemy"),
        "the formal target is the enemy follower"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"]),
        "the printed fanfare actually destroys its declared target"
    );
    game.resume_internal_card().unwrap();
}

fn activate(game: &mut Game) -> DecisionView {
    let ability = prepared().current["CP01-073"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let question = awaiting(
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["source"]),
        "the source is sacrificed as a cost before the Look choice"
    );
    assert_eq!(
        view["P1"]["field"],
        json!([]),
        "the paid source leaves the field before observing"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 6_u8,
        "the activation has no unprinted PP fee after paying four to play"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the observer can answer the Look"
    );
    question
}

fn optional_card(question: &DecisionView, eligible: bool) -> Vec<u64> {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private observed card choice")
    };
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "taking an eligible card may be declined under Q397"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "only the single observed card may be taken"
    );
    assert_eq!(
        domain.members.len(),
        usize::from(eligible),
        "current cost seven and the one-card Look receipt jointly define eligibility"
    );
    domain.members.iter().map(|member| member.handle).collect()
}

fn declined(eligible: bool) {
    let mut game = if eligible {
        fixture("eight-spell", 7)
    } else {
        fixture("seven-follower", 8)
    };
    play_and_destroy(&mut game);
    let question = activate(&mut game);
    let _offered = optional_card(&question, eligible);
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["deck"],
        json!(["top", "tail"]),
        "Q397 leaves the unselected card on top without an unprinted remainder move"
    );
    assert_eq!(
        view["P1"]["hand"],
        json!([]),
        "declining or lacking an eligible card adds nothing to hand"
    );
    let public = game.projection(View::P2).unwrap();
    assert!(
        !public["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["id"] == "top" || card["id"] == "tail"),
        "the looked but untaken card is not publicly revealed"
    );
}

#[test]
fn cp01_073_destroys_then_sacrifices_to_look_at_effective_cost_seven_or_decline() {
    let mut game = fixture("eight-spell", 7);
    play_and_destroy(&mut game);
    let question = activate(&mut game);
    let handles = optional_card(&question, true);
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["hand"],
        json!(["top"]),
        "an original eight-cost spell qualifies at effective cost seven without a kind restriction"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail"]),
        "only the single looked card leaves the deck"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "top"),
        "the selected card is publicly revealed after reaching hand"
    );
    assert!(
        !known.iter().any(|card| card["id"] == "tail"),
        "the untouched eligible tail stays private"
    );
    declined(true);
    declined(false);
}
