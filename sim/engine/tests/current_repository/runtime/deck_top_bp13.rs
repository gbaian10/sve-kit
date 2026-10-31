#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Hero and Ocean Look scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, &str); 5] = [
    ("hero-spell", "英雄の魔法", "スペル", "ヒーロー"),
    ("hero-amulet", "英雄の護符", "アミュレット", "ヒーロー"),
    ("hero-follower", "別の英雄", "フォロワー", "ヒーロー"),
    ("ocean-spell", "海洋の魔法", "スペル", "海洋"),
    ("other", "別の仲間", "フォロワー", "兵士"),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, family) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,
                "traits":[family],"cost":"9","power":if follower {"1"} else {"-"},
                "hp":if follower {"9"} else {"-"},"text":null,"sections":[]
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

fn fixture(state: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        state,
        &Value::Null,
        &Value::Null,
        "top-bp13",
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
        panic!("printed decision")
    };
    question
}

fn pending(game: &mut Game, number: &str) -> DecisionView {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("real pending ability")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number)
        .unwrap()
        .handle;
    awaiting(
        game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
            .unwrap(),
    )
}

fn target(game: &mut Game, question: &DecisionView, number: &str) -> CardPlay {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("printed target domain")
    };
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(number))
        .unwrap()
        .handle;
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![handle]),
    )
    .unwrap()
}

fn choose_observed(game: &mut Game, question: &DecisionView, number: &str) -> CardPlay {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("observed trait choice")
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "trait and observation receipt admit only the intended card"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some(number),
        "the observed nine-cost spell proves no follower or cost restriction"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the printed choice is optional"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "only one observed card may move to EX"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the observer sees hidden candidates"
    );
    target(game, question, number)
}

fn hero_state(number: &str, ally: Option<&str>) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":number},{"id":"removal","card":"BP01-116"}
    ]);
    state["players"]["P1"]["zones"]["field"] =
        ally.map_or_else(|| json!([]), |card| json!([{"id":"ally","card":card}]));
    state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"hero-follower"}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw-one","card":"other"},{"id":"draw-two","card":"other"},
        {"id":"selected","card":"hero-spell"},{"id":"tail","card":"hero-spell"}
    ]);
    state
}

fn fanfare_damage(game: &mut Game, number: &str, damage: u8) {
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real two-cost Knight enters before its fanfare"
    );
    let question = pending(game, number);
    resolved(target(game, &question, "hero-follower"));
    game.resume_internal_card().unwrap();
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["objects"]["enemy"]["hp"],
        9_u8.checked_sub(damage).unwrap(),
        "only another own Hero follower changes one damage to four"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 8_u8,
        "the printed two PP is paid for the source"
    );
}

fn lastword_hero(game: &mut Game, number: &str) {
    let removal_question = awaiting(
        game.play_internal_card("removal", PlayerId { seat: 1 })
            .unwrap(),
    );
    resolved(target(game, &removal_question, number));
    game.resume_internal_card().unwrap();
    let before = game.projection(View::Referee).unwrap();
    assert!(
        before["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "real Soul Conversion destroys the Knight before its Lastword"
    );
    assert_eq!(
        before["P1"]["deck"],
        json!(["selected", "tail"]),
        "Soul Conversion draws two before the pending Lastword observes the new top"
    );
    let lastword_question = pending(game, number);
    resolved(choose_observed(game, &lastword_question, "hero-spell"));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["ex"],
        json!(["selected"]),
        "the actual Lastword moves the observed Hero spell to EX"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail"]),
        "the unobserved Hero is untouched and no unprinted remainder action occurs"
    );
    assert_public_selection(game);
}

fn assert_public_selection(game: &Game) {
    let view = game.projection(View::P2).unwrap();
    let known = view["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "selected"),
        "the selected EX arrival becomes public"
    );
    assert!(
        !known.iter().any(|card| card["id"] == "tail"
            || card["id"] == "wrong"
            || card["id"] == "second"
            || card["id"] == "third"),
        "the unobserved tail and unselected observed cards remain private"
    );
}

#[test]
fn bp13_034_and_p08_use_other_own_hero_fanfare_then_the_real_lastword() {
    for number in ["BP13-034", "BP13-P08"] {
        for ally in [
            None,
            Some("other"),
            Some("hero-amulet"),
            Some("hero-follower"),
        ] {
            let mut game = fixture(&hero_state(number, ally));
            let other_hero = ally == Some("hero-follower");
            fanfare_damage(&mut game, number, if other_hero { 4 } else { 1 });
            if other_hero {
                lastword_hero(&mut game, number);
            }
        }
    }
}

#[test]
fn bp13_066_real_base_evolves_then_observes_ocean_cards_for_ex() {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP13-065"}]);
    state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-066"}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"selected","card":"ocean-spell"},{"id":"wrong","card":"hero-spell"},
        {"id":"second","card":"other"},{"id":"third","card":"other"},
        {"id":"tail","card":"ocean-spell"}
    ]);
    let mut game = fixture(&state);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the existing real base evolves for its printed one PP"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        9_u8,
        "real evolution pays exactly one PP"
    );
    let question = pending(&mut game, "BP13-066");
    let order = awaiting(choose_observed(&mut game, &question, "ocean-spell"));
    let InputDomain::Order(cards) = &order.domain else {
        panic!("three private remainder cards")
    };
    assert_eq!(
        cards.len(),
        3_usize,
        "only the observed unselected cards are ordered"
    );
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["ex"],
        json!(["selected"]),
        "the real evolution trigger moves the Ocean spell into EX"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "third", "second", "wrong"]),
        "the unobserved tail stays above the reversed observed remainder"
    );
    assert_public_selection(&game);
}
