#![expect(
    clippy::panic,
    reason = "Complete printed discard and McQueen chains require only their declared decisions."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InstructionOutcome};

const MACHINUS: [&str; 3] = ["BP12-103", "BP12-SL27", "BP12-U07"];
const AMULETS: [&str; 2] = ["ECP01-054", "ECP01-P30"];

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "complete-remaining-evo-discard",
    )
    .unwrap()
}

fn checks(board: &mut Game) {
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed decision");
    };
    q
}

fn trigger(board: &mut Game, card: &str, line: u64) -> CardPlay {
    checks(board);
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending occurrence");
    };
    let handle = options
        .iter()
        .find(|o| o.card == card && o.line == line)
        .unwrap()
        .handle;
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(handle))
        .unwrap()
}

fn enter(board: &mut Game, id: &str) {
    let mut play = board.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    while let CardPlay::Awaiting(q) = play {
        assert!(
            matches!(q.domain, InputDomain::WardEntry { .. }),
            "only the real McQueen's Ward may precede entry"
        );
        play = board
            .answer_internal_card(q.actor, q.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Entered(_)),
        "the printed card enters before its pending abilities"
    );
}

fn finish_head(board: &mut Game, mut play: CardPlay, accept_cost: bool, evolve: bool) -> CardPlay {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accept_cost),
            InputDomain::CostMaterials(d) => {
                assert_eq!(
                    d.members.len(),
                    1,
                    "only the own named tree or the one real amulet pays this cost"
                );
                Answer::CostMaterials(vec![d.members[0].handle])
            }
            InputDomain::Targets(d) => {
                if d.members
                    .iter()
                    .any(|m| m.card.as_deref() == Some("BP07-017"))
                {
                    assert_eq!(
                        d.members.len(),
                        1,
                        "the Nature spell is the only recovery target; neither Machinus printing nor the discarded source is eligible"
                    );
                    assert!(
                        d.members
                            .iter()
                            .all(|m| m.card.as_deref() == Some("BP07-017")),
                        "every offered recovery target excludes the source's name"
                    );
                }
                let is_form = d
                    .members
                    .iter()
                    .any(|m| m.card.as_deref() == Some("CP01-067"));
                if is_form {
                    assert_eq!(
                        d.cardinality.minimum,
                        Nat(0),
                        "effect evolution remains optional, Q1860"
                    );
                    assert!(
                        board.internal_decision(View::P2).is_none(),
                        "the evolution resources stay private"
                    );
                }
                let member = if d
                    .members
                    .iter()
                    .any(|m| m.card.as_deref() == Some("CP01-066"))
                {
                    d.members.last()
                } else {
                    d.members.iter().find(|m| m.card.is_some())
                };
                Answer::Targets(if is_form && !evolve {
                    vec![]
                } else {
                    member.map(|m| vec![m.handle]).unwrap_or_default()
                })
            }
            other @ (InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::WardEntry { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. }
            | InputDomain::Modes { .. }) => panic!("unexpected printed head decision: {other:?}"),
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
    play
}

fn no_pending(board: &mut Game) {
    checks(board);
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "all immediate printed occurrences have resolved"
    );
}

fn enemy(initial: &mut Value) {
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"CP03-013","state":{"hp":20_u8,"max_hp":20_u8,"power":0_u8}}
    ]);
}

fn finish_look(board: &mut Game, mut play: CardPlay, machine: bool, take: bool) {
    let mut saw_pick = false;
    while let CardPlay::Awaiting(q) = play {
        assert!(
            board.internal_decision(View::P2).is_none(),
            "the unchosen top cards remain private"
        );
        let answer = match &q.domain {
            InputDomain::Targets(d) => {
                saw_pick = true;
                assert_eq!(
                    d.members.len(),
                    usize::from(machine),
                    "only the real Machine spell qualifies, not the Nature spell or ordinary follower"
                );
                assert!(
                    d.members
                        .iter()
                        .all(|m| m.card.as_deref() == Some("BP12-041")),
                    "the selection is filtered by printed Machine trait without requiring follower kind"
                );
                Answer::Targets(if take {
                    vec![d.members[0].handle]
                } else {
                    vec![]
                })
            }
            InputDomain::Order(cards) => {
                assert_eq!(
                    cards.len(),
                    if take { 2_usize } else { 3_usize },
                    "only the actual moved card is excluded from the bottom remainder"
                );
                Answer::Order(cards.iter().rev().map(|c| c.handle).collect())
            }
            other @ (InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::WardEntry { .. }
            | InputDomain::Quick { .. }
            | InputDomain::OptionalCost
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. }
            | InputDomain::Modes { .. }) => {
                panic!("unexpected Machinus fanfare decision: {other:?}")
            }
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
    let activation = resolved(play);
    if machine {
        assert!(
            saw_pick,
            "an eligible Machine card must be offered even when declined"
        );
    }
    let revealed: usize = activation
        .progress
        .receipts
        .values()
        .filter_map(|receipt| {
            if let InstructionOutcome::Revealed(cards) = receipt {
                Some(cards.len())
            } else {
                None
            }
        })
        .sum();
    assert_eq!(
        revealed,
        usize::from(take),
        "only the card actually taken into hand is revealed"
    );
}

// All printings exercise the same complete source through one shared repository selection.
#[test]
fn all_machinus_printings_look_reveal_order_create_repair_and_recover_three_pp() {
    for card in MACHINUS {
        for (machine, take) in [(true, true), (true, false), (false, false)] {
            let mut initial = draw_position();
            initial["players"]["P1"]["pp"] = json!({"current":8_u8,"max":8_u8});
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"seen-first","card":if machine {"BP12-041"} else {"CP03-037"}},
                {"id":"seen-second","card":"BP07-017"},
                {"id":"seen-third","card":"CP03-037"},
                {"id":"tail","card":"CP03-037"}
            ]);
            let mut board = game(&initial);
            enter(&mut board, "source");
            let play = trigger(&mut board, card, 2);
            finish_look(&mut board, play, machine, take);
            no_pending(&mut board);
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["pp"]["current"], 6_u8,
                "eight minus the printed five plus unconditional three PP"
            );
            assert_eq!(
                view["P1"]["hand"],
                if take {
                    json!(["seen-first"])
                } else {
                    json!([])
                },
                "taking the observed card is optional"
            );
            assert_eq!(
                view["P1"]["deck"],
                if take {
                    json!(["tail", "seen-third", "seen-second"])
                } else {
                    json!(["tail", "seen-third", "seen-second", "seen-first"])
                },
                "the full remainder follows the player's chosen bottom order"
            );
            let ex = view["P1"]["ex"].as_array().unwrap();
            assert_eq!(ex.len(), 1, "the fanfare always creates one Repair Mode");
            assert_eq!(
                view["objects"][ex[0].as_str().unwrap()]["name"],
                "リペアモード",
                "the existing full token template supplies Repair Mode"
            );
        }
    }
}

fn recovery_position(card: &str, tree: u8, target: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"producer","card":"BP14-018"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"source","card":card},{"id":"tail","card":"CP03-037"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = if tree == 0 {
        json!([])
    } else {
        json!([{"id":"tree","card":"BP07-T03","state":{"acted":tree == 2}}])
    };
    let mut grave = vec![json!({"id":"same-name","card":"BP12-103"})];
    if target {
        grave.push(json!({"id":"recovered","card":"BP07-017"}));
    }
    initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"opposing-tree","card":"BP07-T03"}]);
    initial
}

fn discard_with_real_taketsumi(board: &mut Game) {
    enter(board, "producer");
    let q = awaiting(trigger(board, "BP14-018", 2));
    let InputDomain::Targets(d) = &q.domain else {
        panic!("Taketsumi's real discard");
    };
    assert_eq!(
        d.members.len(),
        1,
        "the just-drawn Machinus is the only hand card"
    );
    resolved(
        board
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![d.members[0].handle]))
            .unwrap(),
    );
}

// Reusing the real producer for every printing covers source capture and all payment outcomes.
#[test]
fn all_machinus_printings_recover_only_other_named_nature_cards_and_pay_only_own_standing_trees() {
    for card in MACHINUS {
        for (pay, tree, target) in [
            (true, 1_u8, true),
            (false, 1, true),
            (true, 0, true),
            (true, 2, true),
            (true, 1, false),
        ] {
            let mut board = game(&recovery_position(card, tree, target));
            discard_with_real_taketsumi(&mut board);
            checks(&mut board);
            assert!(
                board.internal_pending_choice().unwrap().is_some(),
                "{card}, tree={tree}, target={target}, pay={pay}: the committed self discard queues the occurrence before checking payment"
            );
            let play = trigger(&mut board, card, 1);
            let outcome = finish_head(&mut board, play, pay, false);
            if pay && tree == 1 && target {
                resolved(outcome);
            } else {
                assert!(
                    matches!(outcome, CardPlay::Cancelled),
                    "{card}, tree={tree}, target={target}, pay={pay}: declining or an impossible declaration must cancel"
                );
            }
            no_pending(&mut board);
            let view = board.projection(View::Referee).unwrap();
            let recovered = pay && tree == 1 && target;
            assert_eq!(
                view["P1"]["hand"],
                if recovered {
                    json!(["recovered"])
                } else {
                    json!([])
                },
                "only a paid declaration with another named Nature card recovers it"
            );
            assert!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("same-name")),
                "another printing with the same name is never eligible"
            );
            assert!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source")),
                "the discarded Machinus cannot recover itself"
            );
            if tree != 0 {
                assert_eq!(
                    view["objects"]["tree"]["acted"],
                    tree == 2 || recovered,
                    "declining or lacking a legal target cannot pay the tree cost, Q1705"
                );
            }
            assert_eq!(
                view["objects"]["opposing-tree"]["acted"], false,
                "an opponent's tree can never pay this cost"
            );
            assert_eq!(
                view["P1"]["pp"]["current"], 8_u8,
                "only Taketsumi's printed two PP were paid"
            );
        }
    }
}

#[test]
fn machinus_end_hand_limit_recovery_requires_a_second_discard_before_next_turn_q1704() {
    let mut initial = recovery_position("BP12-103", 1, true);
    let mut hand = vec![json!({"id":"source","card":"BP12-103"})];
    hand.extend((0_u8..7).map(|n| json!({"id":format!("hand-{n}"),"card":"CP03-037"})));
    initial["players"]["P1"]["zones"]["hand"] = json!(hand);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"tail","card":"CP03-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"next-draw","card":"CP03-037"}]);
    let mut board = game(&initial);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    checks(&mut board);
    while let Some(q) = board.internal_quick_choice().unwrap() {
        let continuation = board
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        assert!(
            matches!(continuation, CardPlay::Continued(_)),
            "a legal Quick Pass resumes the end-phase procedure"
        );
        checks(&mut board);
    }
    let discarded = board
        .internal_rule_action(&json!({"do":"end-discard","select":["source"]}))
        .unwrap();
    assert_eq!(
        discarded.outcome, "resolved",
        "the end-step rule really discards Machinus"
    );
    let play = trigger(&mut board, "BP12-103", 1);
    resolved(finish_head(&mut board, play, true, false));
    no_pending(&mut board);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(
        view["turn"]["active"], "P1",
        "recovering to eight cards prevents the next turn, Q1704"
    );
    assert_eq!(
        view["P1"]["hand"].as_array().unwrap().len(),
        8,
        "the recovered Nature card puts the hand over its limit again"
    );
    assert_eq!(
        view["objects"]["tree"]["acted"], true,
        "the rule discard permits the same printed tree payment"
    );
    let second = board
        .internal_rule_action(&json!({"do":"end-discard","select":["recovered"]}))
        .unwrap();
    assert_eq!(
        second.outcome, "resolved",
        "the repeated end-step discard is legal"
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["turn"]["active"],
        "P2",
        "the next turn starts only after the second discard"
    );
}

#[test]
fn both_mcqueen_amulet_printings_count_themselves_and_other_mejiro_card_kinds() {
    for card in AMULETS {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"amulet","card":card}]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"mcqueen","card":"CP01-066"},{"id":"other","card":"CP03-037"}]);
        enemy(&mut initial);
        let mut board = game(&initial);
        enter(&mut board, "amulet");
        let play = trigger(&mut board, card, 2);
        resolved(finish_head(&mut board, play, true, true));
        no_pending(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["enemy"]["hp"], 16_u8,
            "two Mejiro cards include the source amulet and the follower, but not another trait"
        );
        assert_eq!(
            view["P1"]["pp"]["current"], 8_u8,
            "the amulet costs its printed two PP"
        );
    }
}

// Both printings and both evolution answers require separate live chains, sharing only frozen inputs.
#[test]
fn both_mcqueen_amulets_complete_the_real_cost_lastword_evolution_return_and_old_trigger_chain_q1862()
 {
    for card in AMULETS {
        for accept in [true, false] {
            let mut initial = draw_position();
            initial["players"]["P1"]["pp"] = json!({"current":5_u8,"max":5_u8});
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"amulet","card":card},{"id":"existing","card":"CP01-066"}]);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"mcqueen","card":"CP01-066"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"CP01-067"},{"id":"other-form","card":"CP01-067"}]);
            enemy(&mut initial);
            let mut board = game(&initial);
            enter(&mut board, "mcqueen");
            let fanfare = trigger(&mut board, "CP01-066", 4);
            resolved(finish_head(&mut board, fanfare, true, true));
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["cemetery"],
                json!(["amulet"]),
                "the real McQueen fanfare pays by moving the amulet to the cemetery"
            );
            let lastword = trigger(&mut board, card, 3);
            resolved(finish_head(&mut board, lastword, true, accept));
            if accept {
                let evolved = trigger(&mut board, "CP01-067", 2);
                resolved(finish_head(&mut board, evolved, true, true));
            }
            let original = trigger(&mut board, card, 1);
            resolved(finish_head(&mut board, original, true, true));
            no_pending(&mut board);
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["objects"]["enemy"]["hp"], 14_u8,
                "McQueen's fanfare and the original amulet occurrence each deal three, even after the amulet returns to hand"
            );
            assert_eq!(
                view["objects"]["mcqueen"]["power"],
                if accept { 5_u8 } else { 4_u8 },
                "the selected follower can evolve or decline, Q1860"
            );
            assert_eq!(
                view["P1"]["hand"],
                if accept { json!(["amulet"]) } else { json!([]) },
                "the true evolved McQueen returns the actual paid amulet"
            );
            assert_eq!(
                view["P1"]["pp"]["current"], 1_u8,
                "effect evolution does not pay the intrinsic one-PP cost, Q1859"
            );
            assert_eq!(
                view["P1"]["ep"], 0_u8,
                "effect evolution does not require an EP"
            );
            if accept {
                let ordinary = board.internal_rule_action(&json!({"do":"evolve","source":"existing","evolve_card":"other-form","pay":{"pp":1_u8}})).unwrap();
                assert_eq!(
                    ordinary.outcome, "resolved",
                    "effect evolution leaves the ordinary evolution quota available, Q1861"
                );
                let returned = trigger(&mut board, "CP01-067", 2);
                resolved(finish_head(&mut board, returned, true, true));
                no_pending(&mut board);
            }
        }
    }
}

#[test]
fn two_copies_of_each_amulet_independently_observe_the_same_mcqueen_entry_q1858() {
    for card in AMULETS {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"first","card":card},{"id":"second","card":card}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"mcqueen","card":"CP01-066"}]);
        enemy(&mut initial);
        let mut board = game(&initial);
        enter(&mut board, "mcqueen");
        let fanfare = trigger(&mut board, "CP01-066", 4);
        let declined = finish_head(&mut board, fanfare, false, false);
        assert!(
            matches!(declined, CardPlay::Cancelled),
            "declining the real McQueen fanfare cancels only that occurrence"
        );
        for _ in 0_u8..2 {
            let observed = trigger(&mut board, card, 1);
            resolved(finish_head(&mut board, observed, true, true));
        }
        no_pending(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            14_u8,
            "each actual amulet copy contributes its own three-damage occurrence, Q1858"
        );
    }
}
