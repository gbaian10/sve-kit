#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the complete printed discard-related scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

const MANA: [&str; 2] = ["BP09-038", "BP09-P11"];
const VEDD: [&str; 2] = ["BP18-079", "BP18-SL19"];
const HARU: [&str; 3] = ["BP22-116", "BP22-SSP01", "PR-606"];

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-discard-followers",
    )
    .unwrap()
}
fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw0","card":"BP01-171"},{"id":"draw1","card":"BP01-171"},{"id":"draw2","card":"BP01-171"}
    ]);
    initial
}
fn pending(board: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(board);
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending occurrence")
    };
    let entry = options
        .iter()
        .find(|entry| entry.card == number && entry.line == line)
        .unwrap();
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(entry.handle))
        .unwrap()
}
fn finish(board: &mut Game, mut play: CardPlay, wanted: Option<&str>, accept: bool) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::DiceReroll { .. } => panic!("unexpected fixture dice reroll"),
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            InputDomain::CostMaterials(domain) => {
                if let Some(card) = wanted {
                    assert!(
                        domain
                            .members
                            .iter()
                            .all(|m| m.card.as_deref() == Some(card)),
                        "the printed material filter excludes the wrong trait or original cost"
                    );
                }
                Answer::CostMaterials(
                    domain
                        .members
                        .iter()
                        .filter(|m| wanted.is_none_or(|card| m.card.as_deref() == Some(card)))
                        .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                        .map(|m| m.handle)
                        .collect(),
                )
            }
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .filter(|m| {
                        wanted.is_none_or(|card| {
                            if card == "leader" {
                                m.card.is_none()
                            } else {
                                m.card.as_deref() == Some(card)
                            }
                        })
                    })
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[0]]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Integer(_) => Answer::Integer(Nat(u64::from(accept))),
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Order(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. } => {
                panic!("unexpected complete printed choice: {:?}", q.domain)
            }
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_) | CardPlay::Entered(_)),
        "the complete printed play either enters or finishes its ability body"
    );
    finish_checks(board);
}
fn keyword(view: &Value, id: &str, name: &str) -> bool {
    view["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(name))
}
fn ritual_discard(board: &mut Game, number: &str) {
    let entry_play = board
        .play_internal_card("ritual", PlayerId { seat: 1 })
        .unwrap();
    finish(board, entry_play, None, true);
    let ritual_play = pending(board, "BP02-062", 2);
    finish(board, ritual_play, Some(number), true);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        21_u8,
        "the real Ritual finishes its printed heal before the new discard trigger"
    );
}

#[test]
fn marshal_discard_creates_the_complete_armored_vehicle_in_ex() {
    let mut initial = position("BP11-044");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ritual","card":"BP02-062"}));
    let mut board = game(&initial);
    ritual_discard(&mut board, "BP11-044");
    let pending_play = pending(&mut board, "BP11-044", 1);
    finish(&mut board, pending_play, None, true);
    let view = board.projection(View::Referee).unwrap();
    let ex = view["P1"]["ex"].as_array().unwrap();
    assert_eq!(
        ex.len(),
        1,
        "self discard creates exactly one actual vehicle"
    );
    assert_eq!(
        view["objects"][ex[0].as_str().unwrap()]["name"],
        "魔導装甲車",
        "the real printed token template is used"
    );
    assert!(
        view["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "discard does not return the Golem itself"
    );
}

#[test]
fn marshal_revives_only_a_cheap_wilderness_follower_and_its_real_vehicle_grants_ward_and_life() {
    let mut initial = position("BP11-044");
    initial["players"]["P1"]["pp"] = json!({"current":9_u8,"max":9_u8});
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"revived","card":"BP11-008","state":{"cost":99_u8}},
        {"id":"expensive","card":"BP11-044","state":{"cost":0_u8}},
        {"id":"wrong-trait","card":"BP01-171"}
    ]);
    let mut board = game(&initial);
    let entry_play = board
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut board, entry_play, None, true);
    let CardPlay::Awaiting(q) = pending(&mut board, "BP11-044", 2) else {
        panic!("Golem's printed cemetery target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("cemetery target")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "original cost and Wilderness both restrict the real target"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("BP11-008"),
        "only the printed eligible follower is selectable"
    );
    let answered_play = board
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap();
    finish(&mut board, answered_play, None, true);
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(
        before["P1"]["pp"]["current"], 0_u8,
        "normal Golem play pays exactly nine PP"
    );
    assert!(
        before["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("revived")),
        "the actual cemetery follower enters"
    );
    let vehicle = before["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .find(|id| before["objects"][*id]["name"] == "魔導装甲車")
        .unwrap()
        .to_owned();
    let source = board
        .internal_activated_sources(&vehicle)
        .unwrap()
        .remove(0);
    let activation = board
        .activate_internal_card_instance(&vehicle, source.instance, PlayerId { seat: 1 })
        .unwrap();
    finish(&mut board, activation, Some("BP11-008"), true);
    let after = board.projection(View::Referee).unwrap();
    assert!(
        keyword(&after, "revived", "守護"),
        "the full real token grants Ward"
    );
    assert_eq!(
        after["objects"]["revived"]["hp"], 4_u8,
        "the real Wilderness follower receives the conditional life point"
    );
    assert!(
        !after["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!(vehicle)),
        "the real vehicle pays its own movement cost"
    );
}

#[test]
fn mana_fanfare_uses_the_paid_discard_receipt_or_moves_itself_to_cemetery() {
    for number in MANA {
        for accept in [false, true] {
            let mut initial = position(number);
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"academy","card":"BP02-046"}),
                    json!({"id":"wrong","card":"BP01-171"}),
                ]);
            let mut board = game(&initial);
            let entry_play = board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut board, entry_play, None, true);
            let pending_play = pending(&mut board, number, 2);
            finish(&mut board, pending_play, Some("BP02-046"), accept);
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source")),
                accept,
                "only paying the printed Academy discard preserves the follower"
            );
            assert_eq!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source")),
                !accept,
                "decline executes the separate self-to-cemetery instruction"
            );
            assert_eq!(
                view["P1"]["hand"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("draw0")),
                accept,
                "the paid branch alone draws a card"
            );
            assert!(
                view["P1"]["hand"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("wrong")),
                "the non-Academy card is never discarded"
            );
            assert_eq!(
                view["P1"]["pp"]["current"], 8_u8,
                "Fanfare does not add a PP payment to the normal cost"
            );
        }
    }
}

#[test]
fn mana_both_real_evolution_faces_require_five_cemetery_academy_cards() {
    for number in MANA {
        for count in [4_u8, 5_u8] {
            for face in [0_u8, 1_u8] {
                let mut initial = position(number);
                initial["players"]["P1"]["zones"]["hand"] = json!([]);
                initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
                initial["players"]["P1"]["zones"]["evolve_deck"] =
                    json!([{"id":"form","card":"BP09-039"}]);
                initial["players"]["P1"]["zones"]["cemetery"]=json!((0..count).map(|n|
                    json!({"id":format!("academy{n}"),"card":if n==4 {"BP16-045"} else {"BP02-046"}})
                ).collect::<Vec<_>>());
                let mut board = game(&initial);
                let before = board.projection(View::Referee).unwrap();
                let definition = selection_with_rules().current[number]
                    .program
                    .card()
                    .abilities[0]
                    .node
                    .id
                    .0;
                let result=board.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","definition":definition,"face":face,"pay":{"pp":2_u8}})).unwrap();
                assert_eq!(
                    result.outcome,
                    if count == 5 {
                        "resolved"
                    } else {
                        "cannot-evolve"
                    },
                    "both explicit named faces share the printed Academy threshold"
                );
                if count == 4 {
                    assert_eq!(
                        board.projection(View::Referee).unwrap(),
                        before,
                        "unavailable evolution spends no resource or object"
                    );
                    continue;
                }
                let pending_play = pending(&mut board, "BP09-039", 2);
                finish(&mut board, pending_play, None, true);
                let after = board.projection(View::Referee).unwrap();
                assert_eq!(
                    after["P1"]["pp"]["current"], 8_u8,
                    "either named evolution pays the printed two PP"
                );
                assert_eq!(
                    after["objects"]["source"]["name"],
                    if face == 0 {
                        "マナリアの白竜"
                    } else {
                        "マナリアの黒竜"
                    },
                    "the selected real face determines the evolution"
                );
                assert!(
                    keyword(
                        &after,
                        "source",
                        if face == 0 { "守護" } else { "指定攻撃" }
                    ),
                    "the true evolved face retains its keyword"
                );
                assert_eq!(
                    after["P1"]["leader"]["life"],
                    if face == 0 { 23_u8 } else { 20_u8 },
                    "the white face's complete evolution heal resolves"
                );
                assert_eq!(
                    after["P2"]["leader"]["life"],
                    if face == 1 { 17_u8 } else { 20_u8 },
                    "the black face's complete evolution damage resolves"
                );
            }
        }
    }
}

#[test]
fn vedd_filters_original_cost_and_uses_the_tenth_real_cemetery_card_before_its_once_per_turn_damage()
 {
    for number in VEDD {
        for count in [8_u8, 9_u8] {
            for leader in [false, true] {
                let mut initial = position(number);
                initial["players"]["P1"]["zones"]["hand"]
                    .as_array_mut()
                    .unwrap()
                    .extend([
                        json!({"id":"material","card":"BP01-018","state":{"cost":99_u8}}),
                        json!({"id":"wrong","card":"BP01-171","state":{"cost":2_u8}}),
                    ]);
                initial["players"]["P1"]["zones"]["cemetery"] = json!(
                    (0..count)
                        .map(|n| json!({"id":format!("old{n}"),"card":"BP01-018"}))
                        .collect::<Vec<_>>()
                );
                initial["players"]["P2"]["zones"]["field"] = json!([
                    {"id":"enemy","card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}}
                ]);
                let mut board = game(&initial);
                let entry_play = board
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap();
                finish(&mut board, entry_play, None, true);
                let pending_play = pending(&mut board, number, 1);
                finish(&mut board, pending_play, Some("BP01-018"), true);
                let paid = board.projection(View::Referee).unwrap();
                assert!(
                    paid["P1"]["hand"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("draw0")),
                    "the actual original-two discard enables the Fanfare draw"
                );
                assert!(
                    paid["P1"]["hand"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("wrong")),
                    "effective cost two does not satisfy original cost two"
                );
                let ability = board
                    .internal_activated_sources("source")
                    .unwrap()
                    .remove(0);
                let before = format!("{board:?}");
                let activation = board.activate_internal_card_instance(
                    "source",
                    ability.instance,
                    PlayerId { seat: 1 },
                );
                if count == 8 {
                    activation.unwrap_err();
                    assert_eq!(
                        format!("{board:?}"),
                        before,
                        "nine matching cemetery cards cannot activate and spend nothing"
                    );
                    continue;
                }
                finish(
                    &mut board,
                    activation.unwrap(),
                    Some(if leader { "leader" } else { "BP01-171" }),
                    true,
                );
                let after = board.projection(View::Referee).unwrap();
                assert_eq!(
                    after["P1"]["pp"]["current"], 8_u8,
                    "the activated ability's printed PP cost is zero"
                );
                assert_eq!(
                    after["P2"]["leader"]["life"],
                    if leader { 17_u8 } else { 20_u8 },
                    "the selected opposing leader receives exactly three"
                );
                assert_eq!(
                    after["objects"]["enemy"]["hp"],
                    if leader { 10_u8 } else { 7_u8 },
                    "the selected opposing follower receives exactly three"
                );
                let before_reuse = format!("{board:?}");
                board
                    .activate_internal_card_instance(
                        "source",
                        ability.instance,
                        PlayerId { seat: 1 },
                    )
                    .unwrap_err();
                assert_eq!(
                    format!("{board:?}"),
                    before_reuse,
                    "the complete real activation cannot be reused in the same turn"
                );
            }
        }
    }
}

#[test]
fn vedd_declining_the_fanfare_discard_moves_itself_without_drawing() {
    for number in VEDD {
        let mut board = game(&position(number));
        let entry_play = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, entry_play, None, true);
        let pending_play = pending(&mut board, number, 1);
        finish(&mut board, pending_play, None, false);
        let view = board.projection(View::Referee).unwrap();
        assert!(
            view["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "decline executes Vedd's complete self-to-cemetery branch"
        );
        assert!(
            view["P1"]["hand"].as_array().unwrap().is_empty(),
            "declining the optional cost does not draw"
        );
    }
}

#[test]
fn every_haru_printing_keeps_the_real_fanfare_and_meal() {
    for number in HARU {
        let mut initial = position(number);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"CP01-022"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"carrot-0","card":"CSD01-032"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"foreign","card":"CP01-022"}]);
        let mut board = game(&initial);
        let before = board.projection(View::Referee).unwrap();
        let entry_play = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, entry_play, None, true);
        let CardPlay::Awaiting(q) = pending(&mut board, number, 3) else {
            panic!("Haru's printed other-Uma target")
        };
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("Haru's target domain")
        };
        assert_eq!(
            domain.members.len(),
            1,
            "self and the opposing Uma are excluded from other own Uma"
        );
        let answered_play = board
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
        finish(&mut board, answered_play, None, true);
        meal_race::feed(&mut board, number, 0, false);
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        for stat in ["power", "hp"] {
            assert_eq!(
                after["objects"]["ally"][stat].as_i64().unwrap(),
                before["objects"]["ally"][stat]
                    .as_i64()
                    .unwrap()
                    .checked_add(1)
                    .unwrap(),
                "the real Fanfare adds one to the selected ally's stat"
            );
            assert_eq!(
                after["objects"]["foreign"][stat], before["objects"]["foreign"][stat],
                "the opposing Uma is unchanged"
            );
        }
        assert_eq!(
            after["P1"]["pp"]["current"], 7_u8,
            "normal PP2 plus the true Meal PP1 are both paid"
        );
        assert_eq!(
            after["P1"]["race"],
            json!(["carrot-0"]),
            "Meal executes the existing complete race entry"
        );
    }
}

#[test]
fn every_haru_printing_draws_after_a_real_uma_ability_discard_but_not_the_ritual() {
    for number in HARU {
        let mut initial = position(number);
        initial["players"]["P1"]["pp"] = json!({"current":6_u8,"max":6_u8});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"neo","card":"ECP01-028"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"ECP01-029"}]);
        let mut board = game(&initial);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"neo","evolve_card":"form","pay":{"pp":2_u8}})
                )
                .unwrap()
                .outcome,
            "resolved",
            "the true Uma producer normally evolves"
        );
        let producer_play = pending(&mut board, "ECP01-029", 2);
        finish(&mut board, producer_play, Some(number), true);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["pp"]["max"],
            7_u8,
            "the producer's complete effect finishes before Haru's trigger"
        );
        let pending_play = pending(&mut board, number, 1);
        finish(&mut board, pending_play, None, true);
        assert!(
            board.projection(View::Referee).unwrap()["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("draw0")),
            "the actual own Uma ability source authorizes the draw"
        );
        let mut wrong_source_position = position(number);
        wrong_source_position["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"ritual","card":"BP02-062"}));
        let mut other = game(&wrong_source_position);
        ritual_discard(&mut other, number);
        assert!(
            other.internal_pending_choice().unwrap().is_none(),
            "a real non-Uma ability discards Haru without subscribing"
        );
        assert!(
            other.projection(View::Referee).unwrap()["P1"]["hand"]
                .as_array()
                .unwrap()
                .is_empty(),
            "a wrong source trait cannot draw"
        );
    }
}
