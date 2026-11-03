#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed filter scenarios."
)]
use super::*;
use core::iter::once;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

const CONTROLS: [(&str, &str, &str, u8); 7] = [
    ("dragon-three", "ドラゴン", "フォロワー", 3),
    ("dragon-five", "ドラゴン", "フォロワー", 5),
    ("dragon-seven", "ドラゴン", "スペル", 7),
    ("dragon-eight", "ドラゴン", "フォロワー", 8),
    ("other-seven", "ニュートラル", "フォロワー", 7),
    ("amulet", "ニュートラル", "アミュレット", 1),
    ("spell", "ニュートラル", "スペル", 1),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, class, kind, cost) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":number,"card_class":class,"card_type":kind,"traits":[],
                "cost":cost.to_string(),"power":if follower {"2"} else {"-"},
                "hp":if follower {"3"} else {"-"},"text":null,"sections":[]
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
        repository::select_with_rules(&snapshot, &keywords, &input, &rules.to_string(), &EngineIdentityInput::LegacyJp).unwrap()
    })
}

fn fixture(state: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        state,
        &Value::Null,
        &Value::Null,
        "observe-filter",
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

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed decision")
    };
    question
}

fn settle_entry(game: &mut Game, play: CardPlay) -> CardPlay {
    if let CardPlay::Awaiting(q) = &play
        && matches!(q.domain, InputDomain::WardEntry { .. })
    {
        return game
            .answer_internal_card(q.actor, q.token, Answer::WardEntry(false))
            .unwrap();
    }
    play
}

fn trigger(game: &mut Game, number: &str) -> DecisionView {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(settle_entry(game, play), CardPlay::Entered(_)),
        "printed permanent enters before its trigger"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare")
    };
    let handle = options.iter().find(|o| o.card == number).unwrap().handle;
    awaiting(
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
            .unwrap(),
    )
}

fn finish_order(game: &mut Game, q: &DecisionView) {
    let InputDomain::Order(cards) = &q.domain else {
        panic!("private bottom ordering")
    };
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Order(cards.iter().rev().map(|c| c.handle).collect()),
        )
        .unwrap(),
    );
}

fn reject_foreign_or_excessive_answers(game: &mut Game, q: &DecisionView) {
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("observed choice")
    };
    assert!(
        game.internal_decision(View::P2).is_none(),
        "opponent receives no private question"
    );
    let before = game.projection(View::Referee).unwrap();
    assert!(
        game.answer_internal_card(PlayerId { seat: 2 }, q.token, Answer::Targets(vec![]))
            .is_err(),
        "opponent cannot answer"
    );
    assert!(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![999]))
            .is_err(),
        "foreign handle cannot read the untouched deck tail"
    );
    if domain.cardinality.minimum.0 > 0 {
        assert!(
            game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
                .is_err(),
            "mandatory selection cannot be declined"
        );
    }
    let mut excessive: Vec<_> = domain.members.iter().map(|m| m.handle).collect();
    excessive.push(domain.members[0].handle);
    assert!(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(excessive))
            .is_err(),
        "duplicate or excessive choice is rejected"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "rejected answers preserve state"
    );
}

fn printed_look(
    number: &str,
    cards: &[(&str, &str, u8)],
    chosen: &[&str],
    min: u64,
    max: u64,
    cemetery: bool,
) {
    let mut state = draw_position();
    let zone = if number == "BP04-073" {
        "field"
    } else {
        "hand"
    };
    state["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        cards
            .iter()
            .map(|(id, card, cost)| json!({"id":id,"card":card,"state":{"cost":cost}}))
            .collect::<Vec<_>>()
    );
    let mut game = fixture(&state);
    let q = if number == "BP04-073" {
        awaiting(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        )
    } else if number == "BP13-040" {
        trigger(&mut game, number)
    } else {
        awaiting(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        )
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("observed choice")
    };
    assert_eq!(
        domain.cardinality,
        Cardinality {
            minimum: Nat(min),
            maximum: Some(Nat(max))
        },
        "printed optional or compulsory bounds survive runtime"
    );
    assert_eq!(
        domain
            .members
            .iter()
            .map(|m| m.card.as_deref().unwrap())
            .collect::<Vec<_>>(),
        chosen,
        "class, kind, cost and receipt scope exclude every distractor and unobserved tail"
    );
    reject_foreign_or_excessive_answers(&mut game, &q);
    let order = awaiting(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(max).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
        )
        .unwrap(),
    );
    finish_order(&mut game, &order);
    check_observed_outcome(&game, cards, chosen, max, cemetery);
}

fn check_observed_outcome(
    game: &Game,
    cards: &[(&str, &str, u8)],
    chosen: &[&str],
    max: u64,
    cemetery: bool,
) {
    let selected_ids: Vec<_> = cards
        .iter()
        .filter(|(_, card, _)| {
            chosen
                .iter()
                .take(usize::try_from(max).unwrap())
                .any(|name| name == card)
        })
        .map(|(id, _, _)| *id)
        .filter(|id| *id != "tail")
        .collect();
    let view = game.projection(View::Referee).unwrap();
    let dest = if cemetery { "cemetery" } else { "hand" };
    for id in &selected_ids {
        assert!(
            view["P1"][dest].as_array().unwrap().contains(&json!(id)),
            "selected observed card reaches printed destination"
        );
    }
    let expected_deck = once("tail")
        .chain(
            cards[..cards.len().checked_sub(1).unwrap()]
                .iter()
                .rev()
                .map(|(id, _, _)| *id)
                .filter(|id| !selected_ids.contains(id)),
        )
        .collect::<Vec<_>>();
    assert_eq!(
        view["P1"]["deck"],
        json!(expected_deck),
        "only the unchosen observed remainder moves to the bottom in chosen order"
    );
    let opponent = game.projection(View::P2).unwrap();
    let known = opponent["known_cards"].as_array().unwrap();
    for id in &selected_ids {
        assert!(
            known.iter().any(|card| card["id"] == *id),
            "selected cards are public after reveal or cemetery movement"
        );
    }
    assert!(
        !known
            .iter()
            .any(|c| c["id"] == "tail" || c["id"] == "wrong"),
        "unchosen cards stay private"
    );
}

#[test]
fn bp01_100_filters_dragon_class_and_effective_cost() {
    printed_look(
        "BP01-100",
        &[
            ("selected", "dragon-three", 5),
            ("wrong", "other-seven", 7),
            ("cheap", "dragon-five", 4),
            ("dragon-spell", "dragon-seven", 7),
            ("spell", "spell", 1),
            ("tail", "dragon-three", 5),
        ],
        &["dragon-three", "dragon-seven"],
        0,
        1,
        false,
    );
}
#[test]
fn bp04_073_pays_then_filters_dragon_followers_by_effective_cost() {
    printed_look(
        "BP04-073",
        &[
            ("selected", "dragon-five", 3),
            ("wrong", "other-seven", 1),
            ("spell", "dragon-seven", 1),
            ("tail", "dragon-five", 3),
        ],
        &["dragon-five"],
        0,
        1,
        false,
    );
}
#[test]
fn bp11_062_takes_two_dragon_cards_using_original_cost() {
    printed_look(
        "BP11-062",
        &[
            ("first", "dragon-seven", 1),
            ("second", "dragon-eight", 1),
            ("wrong", "other-seven", 8),
            ("cheap", "dragon-five", 8),
            ("amulet", "amulet", 1),
            ("tail", "dragon-eight", 8),
        ],
        &["dragon-seven", "dragon-eight"],
        0,
        2,
        false,
    );
}
#[test]
fn bp01_148_takes_only_an_observed_amulet() {
    printed_look(
        "BP01-148",
        &[
            ("selected", "amulet", 1),
            ("wrong", "spell", 1),
            ("f1", "dragon-three", 3),
            ("f2", "other-seven", 7),
            ("f3", "dragon-five", 5),
            ("tail", "amulet", 1),
        ],
        &["amulet"],
        0,
        1,
        false,
    );
}
#[test]
fn bp05_051_takes_only_an_observed_spell() {
    printed_look(
        "BP05-051",
        &[
            ("selected", "spell", 1),
            ("wrong", "amulet", 1),
            ("tail", "spell", 1),
        ],
        &["spell"],
        0,
        1,
        false,
    );
}
#[test]
fn bp13_040_requires_one_observed_card_to_cemetery() {
    printed_look(
        "BP13-040",
        &[
            ("selected", "amulet", 1),
            ("s", "spell", 1),
            ("f", "dragon-three", 3),
            ("o", "other-seven", 7),
            ("d", "dragon-five", 5),
            ("tail", "spell", 1),
        ],
        &[
            "amulet",
            "spell",
            "dragon-three",
            "other-seven",
            "dragon-five",
        ],
        1,
        1,
        true,
    );
}

#[test]
fn bp01_149_granted_ward_is_eligible_but_plain_follower_is_not() {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP01-149"}]);
    state["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ally","card":"BP01-037"},{"id":"plain","card":"BP01-034"}]);
    let mut game = fixture(&state);
    let fanfare_question = trigger(&mut game, "BP01-149");
    let InputDomain::Targets(fanfare_targets) = &fanfare_question.domain else {
        panic!("fanfare target")
    };
    let handle = fanfare_targets
        .members
        .iter()
        .find(|m| m.card.as_deref() == Some("BP01-037"))
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(
            fanfare_question.actor,
            fanfare_question.token,
            Answer::Targets(vec![handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let activation_question = awaiting(
        game.activate_internal_card("source", AbilityId(4), PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(ward_targets) = &activation_question.domain else {
        panic!("ward target")
    };
    assert_eq!(
        ward_targets.members.len(),
        1,
        "effective granted ward qualifies and plain follower does not"
    );
    assert_eq!(ward_targets.members[0].card.as_deref(), Some("BP01-037"));
    resolved(
        game.answer_internal_card(
            activation_question.actor,
            activation_question.token,
            Answer::Targets(vec![ward_targets.members[0].handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["ally"]["power"], 7_u8);
    assert_eq!(view["objects"]["ally"]["hp"], 7_u8);
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["source"]),
        "source sacrifice is paid"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 7_u8,
        "one PP to play and two PP activation"
    );
}

#[test]
fn bp06_093_pays_three_printed_ward_followers_from_cemetery() {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP06-093"}]);
    state["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"w1","card":"BP01-086"},{"id":"w2","card":"BP01-086"},{"id":"w3","card":"BP01-086"},{"id":"plain","card":"BP01-037"}]);
    state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = fixture(&state);
    let target_question = awaiting(
        game.activate_internal_card("source", AbilityId(4), PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(targets) = &target_question.domain else {
        panic!("enemy target")
    };
    let cost_question = awaiting(
        game.answer_internal_card(
            target_question.actor,
            target_question.token,
            Answer::Targets(vec![targets.members[0].handle]),
        )
        .unwrap(),
    );
    let InputDomain::CostMaterials(materials) = &cost_question.domain else {
        panic!("cemetery cost")
    };
    assert_eq!(
        materials.members.len(),
        3,
        "only printed ward followers qualify for the cost"
    );
    resolved(
        game.answer_internal_card(
            cost_question.actor,
            cost_question.token,
            Answer::CostMaterials(materials.members.iter().map(|m| m.handle).collect()),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["cemetery"], json!(["plain"]));
    assert_eq!(
        view["P2"]["banish"],
        json!(["enemy"]),
        "paid activation banishes the enemy"
    );
}

#[test]
fn bp01_149_cannot_select_a_silenced_printed_ward() {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP01-149"},
        {"id":"silenced","card":"BP01-086","state":{"silenced":true}}
    ]);
    let mut game = fixture(&state);
    let before = game.projection(View::Referee).unwrap();
    assert!(
        game.activate_internal_card("source", AbilityId(4), PlayerId { seat: 1 })
            .is_err(),
        "silenced printed Ward is not an effective keyword"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "failed feasibility pays no PP or sacrifice"
    );
}

#[test]
fn bp13_040_empty_deck_uses_the_shared_as_much_as_possible_rule() {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP13-040"}]);
    state["players"]["P1"]["zones"]["deck"] = json!([]);
    let mut game = fixture(&state);
    let q = trigger(&mut game, "BP13-040");
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("empty observed choice")
    };
    assert!(
        domain.members.is_empty(),
        "an empty Look creates no candidates"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "CR 1.3.2 lowers resolution minimum to the available capacity"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "printed maximum remains one"
    );
    let result = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
        .unwrap();
    if let CardPlay::Awaiting(order) = result {
        finish_order(&mut game, &order);
    } else {
        resolved(result);
    }
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["cemetery"],
        json!([]),
        "no unavailable card is moved"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!([]),
        "the empty remainder stays empty"
    );
}

#[test]
fn bp01_149_filters_static_ward_and_tracks_the_provider_being_silenced() {
    for silenced in [false, true] {
        let mut state = draw_position();
        state["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":"BP01-149"},
            {"id":"provider","card":"BP20-072","state":{"silenced":silenced}},
            {"id":"orca","card":"BP02-T05"},
            {"id":"plain","card":"BP01-034"}
        ]);
        let mut game = fixture(&state);
        let before = game.projection(View::Referee).unwrap();
        let play = game.activate_internal_card("source", AbilityId(4), PlayerId { seat: 1 });
        if silenced {
            assert!(
                play.is_err(),
                "without its provider the Orca has no Ward and cannot be selected"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap(),
                before,
                "failed target feasibility pays no activation cost"
            );
            continue;
        }
        let q = awaiting(play.unwrap());
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("effective Ward targets")
        };
        assert_eq!(
            domain.members.len(),
            1_usize,
            "only the follower receiving static Ward qualifies"
        );
        assert_eq!(domain.members[0].card.as_deref(), Some("BP02-T05"));
        resolved(
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["orca"]["power"], 4_u8);
        assert_eq!(view["objects"]["orca"]["hp"], 4_u8);
    }
}
