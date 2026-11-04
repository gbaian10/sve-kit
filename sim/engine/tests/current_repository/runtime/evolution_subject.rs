#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these complete printed subject evolution scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InstructionOutcome, SearchAnswer};

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-subject-evolution",
    )
    .unwrap()
}

fn position(base: &str, form: &str, current: u8, maximum: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":current,"max":maximum});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"source-form","card":form}]);
    initial
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed choice");
    };
    question
}

fn evolve_source(board: &mut Game) {
    board
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"source-form","pay":{"pp":1_u8}}),
        )
        .unwrap();
}

fn trigger(board: &mut Game, card: &str) -> CardPlay {
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed evolution trigger");
    };
    let handle = options
        .iter()
        .find(|option| option.card == card && option.line == 1)
        .unwrap()
        .handle;
    board
        .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap()
}

fn target_cards(question: &DecisionView) -> Vec<String> {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("printed object selection");
    };
    let mut cards = domain
        .members
        .iter()
        .map(|member| member.card.clone().unwrap())
        .collect::<Vec<_>>();
    cards.sort();
    cards
}

fn choose(board: &mut Game, question: &DecisionView, card: Option<&str>) -> CardPlay {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("printed object selection");
    };
    let handles = card
        .map(|number| {
            vec![
                domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some(number))
                    .unwrap()
                    .handle,
            ]
        })
        .unwrap_or_default();
    board
        .answer_internal_card(question.actor, question.token, Answer::Targets(handles))
        .unwrap()
}

fn finish_orders(board: &mut Game, mut play: CardPlay) -> Activation {
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::Order(cards) = &question.domain else {
            panic!("printed remainder order");
        };
        play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
            )
            .unwrap();
    }
    resolved(play)
}

fn checks(board: &mut Game) {
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "all actual immediate abilities have resolved"
    );
}

fn revealed(activation: &Activation) -> usize {
    activation
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
        .sum()
}

fn nilr_position(form: &str) -> Value {
    let mut initial = position("BP16-059", form, 1, 7);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP16-059"},
        {"id":"chef","card":"BP11-109"},
        {"id":"same-name","card":"BP16-059"},
        {"id":"wrong-trait","card":"BP03-020"},
        {"id":"no-evolution","card":"BP11-115"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"source-form","card":form},
        {"id":"chef-form","card":"BP11-110"},
        {"id":"wrong-form","card":"BP03-021"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"picked","card":"BP11-109"},
        {"id":"other-a","card":"CP03-013"},
        {"id":"other-b","card":"CP03-013"},
        {"id":"other-c","card":"CP03-013"},
        {"id":"tail","card":"CP03-013"}
    ]);
    initial
}

fn chef_look(board: &mut Game) {
    let question = awaiting(trigger(board, "BP11-110"));
    assert_eq!(
        target_cards(&question),
        vec!["BP11-109"],
        "the real evolved chef looks at four and keeps only a Wasteland card"
    );
    let play = choose(board, &question, Some("BP11-109"));
    let activation = finish_orders(board, play);
    assert_eq!(revealed(&activation), 1, "the chef publicly reveals its pick");
}

#[test]
fn both_nilr_printings_filter_the_recipient_and_resolve_the_real_chefs_evolution() {
    for form in ["BP16-060", "BP16-SL18"] {
        for accept in [false, true] {
            let mut board = game(&nilr_position(form));
            evolve_source(&mut board);
            let question = awaiting(trigger(&mut board, form));
            assert_eq!(
                target_cards(&question),
                vec!["BP11-109"],
                "same name, non-Wasteland and no-evolution followers are excluded"
            );
            let resource = awaiting(choose(&mut board, &question, Some("BP11-109")));
            assert_eq!(
                target_cards(&resource),
                vec!["BP11-110"],
                "the recipient uses its own name instead of Nilr's"
            );
            let InputDomain::Targets(domain) = &resource.domain else {
                panic!("private evolution resource");
            };
            assert_eq!(domain.cardinality.minimum, Nat(0), "evolution may be declined");
            assert!(board.internal_decision(View::P2).is_none(), "resources remain private");
            resolved(choose(&mut board, &resource, accept.then_some("BP11-110")));
            if accept {
                chef_look(&mut board);
            }
            checks(&mut board);
            let after = board.projection(View::Referee).unwrap();
            assert_eq!(after["objects"]["source"]["power"], 5_u8, "the actual Nilr form entered");
            assert_eq!(
                after["objects"]["chef"]["power"],
                if accept { 3_u8 } else { 2_u8 },
                "only an accepted recipient evolution changes the chef"
            );
            assert_eq!(after["P1"]["pp"]["current"], 0_u8, "no PP remains to pay the chef's intrinsic cost");
            assert_eq!(after["P1"]["ep"], 0_u8, "no EP was supplied");
            assert_eq!(after["P1"]["sep"], 0_u8, "no SEP was supplied");
            assert_eq!(
                after["P1"]["hand"],
                if accept { json!(["picked"]) } else { json!([]) },
                "the actual recipient's evolution trigger runs only after accepting"
            );
            if accept {
                assert_eq!(after["P1"]["deck"], json!(["tail","other-c","other-b","other-a"]), "the chef orders all unmoved observed cards below the untouched tail");
            }
        }
    }
}

fn fencer_position(form: &str) -> Value {
    let mut initial = position("BP03-020", form, 1, 1);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP03-020"},
        {"id":"ally","card":"BP03-020"},
        {"id":"mach","card":"BP03-028"},
        {"id":"wrong-trait","card":"BP11-109"},
        {"id":"no-evolution","card":"BP03-036"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"source-form","card":form},
        {"id":"ally-form","card":form},
        {"id":"mach-form","card":"BP03-029"}
    ]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"hero-a","card":"BP03-036"},{"id":"hero-b","card":"BP03-036"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"CP03-013","state":{"power":0_u8,"hp":10_u8,"max_hp":10_u8}}
    ]);
    initial
}

fn fencer_chain(board: &mut Game, form: &str) {
    let target = awaiting(trigger(board, form));
    assert_eq!(target_cards(&target), vec!["BP03-028"], "already evolved fencers and a Hero without evolution are excluded");
    let resource = awaiting(choose(board, &target, Some("BP03-028")));
    assert_eq!(target_cards(&resource), vec!["BP03-029"], "the second recipient has its own evolution resource");
    resolved(choose(board, &resource, Some("BP03-029")));
    let enemy = awaiting(trigger(board, "BP03-029"));
    assert_eq!(target_cards(&enemy), vec!["CP03-013"], "the actual Mach Knight evolution trigger chooses the enemy");
    resolved(choose(board, &enemy, Some("CP03-013")));
}

#[test]
fn both_fencer_printings_allow_refusal_and_chain_through_another_fencer_to_mach_knight() {
    for form in ["BP03-021", "BP03-SL06"] {
        for accept in [false, true] {
            let mut board = game(&fencer_position(form));
            evolve_source(&mut board);
            let target = awaiting(trigger(&mut board, form));
            assert_eq!(target_cards(&target), vec!["BP03-020","BP03-028"], "only Hero followers with evolution are eligible");
            let resource = awaiting(choose(&mut board, &target, Some("BP03-020")));
            assert_eq!(target_cards(&resource), vec![form], "the unused same-name fencer form is offered");
            resolved(choose(&mut board, &resource, accept.then_some(form)));
            if accept {
                fencer_chain(&mut board, form);
            }
            checks(&mut board);
            let after = board.projection(View::Referee).unwrap();
            assert_eq!(after["objects"]["source"]["power"], 5_u8, "ordinary evolution reached the real source form");
            assert_eq!(after["objects"]["ally"]["power"], if accept {5_u8} else {2_u8}, "the first recipient can decline even with a form available");
            assert_eq!(after["objects"]["mach"]["power"], if accept {4_u8} else {3_u8}, "the next real evolution trigger can evolve another recipient");
            assert_eq!(after["objects"]["enemy"]["hp"], if accept {6_u8} else {10_u8}, "Mach Knight deals four with two actual Heroes in the cemetery");
            assert_eq!(after["P1"]["pp"]["current"], 0_u8, "the chain pays neither recipient's intrinsic PP cost");
        }
    }
}

fn yui_look_position(form: &str) -> Value {
    let mut initial = position("ECP02-042", form, 1, 1);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"picked","card":"CP02-062"},
        {"id":"other","card":"BP11-109"},
        {"id":"tail","card":"CP03-013"}
    ]);
    initial
}

fn yui_mode(board: &mut Game, form: &str, mode: u64) -> CardPlay {
    let question = awaiting(trigger(board, form));
    assert!(matches!(question.domain, InputDomain::Modes { .. }), "the real Yui declares one printed mode");
    board
        .answer_internal_card(question.actor, question.token, Answer::Modes(vec![ModeId(mode)]))
        .unwrap()
}

#[test]
fn both_yui_printings_look_at_two_and_may_reveal_a_passion_spell_then_order_the_remainder() {
    for form in ["ECP02-043", "ECP02-P22"] {
        for take in [false, true] {
            let mut board = game(&yui_look_position(form));
            evolve_source(&mut board);
            let pick = awaiting(yui_mode(&mut board, form, 1));
            assert_eq!(target_cards(&pick), vec!["CP02-062"], "a Passion spell qualifies while a non-Passion follower does not");
            assert!(board.internal_decision(View::P2).is_none(), "the looked-at cards stay private until a pick is revealed");
            let play = choose(&mut board, &pick, take.then_some("CP02-062"));
            let activation = finish_orders(&mut board, play);
            assert_eq!(revealed(&activation), usize::from(take), "only a chosen card is publicly revealed");
            checks(&mut board);
            let after = board.projection(View::Referee).unwrap();
            assert_eq!(after["objects"]["source"]["power"], 2_u8, "ordinary evolution uses the actual Yui form");
            assert_eq!(after["P1"]["hand"], if take {json!(["picked"])} else {json!([])}, "the selected spell alone enters the hand");
            assert_eq!(after["P1"]["deck"], if take {json!(["tail","other"])} else {json!(["tail","other","picked"])}, "all remaining observations follow the chosen bottom order");
            assert_eq!(after["P1"]["pp"]["current"], 0_u8, "only the intrinsic source evolution cost one PP");
        }
    }
}

fn yui_entry_position(form: &str, found: bool) -> Value {
    let mut initial = position("ECP02-042", form, 4, 4);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"source-form","card":form},{"id":"ally-form","card":form},
        {"id":"wrong-form","card":"BP03-021"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"found","card":if found {"ECP02-042"} else {"CP03-013"}},
        {"id":"passion","card":"CP02-062"},
        {"id":"wrong-name","card":"CP03-013"},
        {"id":"tail","card":"CP03-013"}
    ]);
    initial
}

fn yui_entry(board: &mut Game, payment: &DecisionView, form: &str, found: bool, accept: bool) {
    let search = awaiting(
        board.answer_internal_card(
            payment.actor,
            payment.token,
            Answer::OptionalCost(true),
        ).unwrap(),
    );
    let InputDomain::Search(domain) = &search.domain else { panic!("printed Yui search"); };
    assert_eq!(domain.cards.len(), usize::from(found), "search excludes wrong names and non-followers");
    let play = board.answer_internal_card(search.actor, search.token, Answer::Search(SearchAnswer::Ungrouped(domain.cards.iter().map(|card|card.handle).collect()))).unwrap();
    if !found {
        resolved(play);
        return;
    }
    let resource = awaiting(play);
    assert_eq!(target_cards(&resource), vec![form], "only the unused recipient-named form is selectable after the producer left");
    assert!(board.internal_decision(View::P2).is_none(), "the recipient's evolution resources remain private");
    resolved(choose(board, &resource, accept.then_some(form)));
    if accept {
        let pick = awaiting(yui_mode(board, form, 1));
        let look_play = choose(board, &pick, None);
        finish_orders(board, look_play);
    }
}

#[test]
fn both_yui_printings_pay_then_follow_the_actual_entry_and_trigger_its_real_form() {
    for form in ["ECP02-043", "ECP02-P22"] {
        for (pay, found, accept) in [(false,true,false),(true,false,false),(true,true,false),(true,true,true)] {
            let mut board = game(&yui_entry_position(form, found));
            evolve_source(&mut board);
            let payment = awaiting(yui_mode(&mut board, form, 2));
            assert_eq!(payment.domain, InputDomain::OptionalCost, "mode two declares its full three-PP plus self movement cost");
            if pay {
                yui_entry(&mut board, &payment, form, found, accept);
            } else {
                resolved(board.answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(false)).unwrap());
            }
            checks(&mut board);
            let after = board.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["pp"]["current"], if pay {0_u8} else {3_u8}, "payment spends exactly three PP and evolution of the recipient spends none");
            assert_eq!(after["P1"]["cemetery"], if pay {json!(["source"])} else {json!([])}, "the producer leaves only after its cost is accepted");
            if pay && found {
                assert_eq!(after["P1"]["field"], json!(["found"]), "only the actual searched follower enters");
                assert_eq!(after["objects"]["found"]["generation"], 1_u8, "effect evolution preserves the actual entry generation");
                assert_eq!(after["objects"]["found"]["power"], if accept {2_u8} else {1_u8}, "the entered follower can decline or evolve and resolve its own true form");
            } else if pay {
                assert_eq!(after["P1"]["field"], json!([]), "no successful entry skips the evolution question");
            } else {
                assert_eq!(after["P1"]["field"], json!(["source"]), "declining cost skips search and evolution together");
            }
        }
    }
}
