#![expect(clippy::panic, reason = "Unexpected questions invalidate the printed draw scenarios.")]
use super::*;
use super::event_cemetery::finish_checks;

const fn actor() -> PlayerId { PlayerId { seat: 1 } }

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"experiment","card":"BP05-094"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-first"),"card":"BP01-037"},
            {"id":format!("{seat}-second"),"card":"BP01-037"},
            {"id":format!("{seat}-third"),"card":"BP01-037"}
        ]);
    }
    initial
}

const fn draw(player: PlayerRef, count: i64) -> Action {
    Action::Draw { player, count: Expr::Literal(Int(count)), mode: StepwiseMode::Exact }
}

fn resolve_draws(board: &mut Game, printing: &str, count: usize) {
    finish_checks(board);
    for remaining in (1..=count).rev() {
        let question = board.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else { panic!("draw pending"); };
        assert_eq!(options.len(), remaining, "Q756: one occurrence per card actually drawn");
        assert!(options.iter().all(|option| option.card == printing && option.event == "Draw"), "pending labels reveal no drawn identity");
        let handle = options[0].handle;
        resolved(board.answer_internal_card(question.actor, question.token, Answer::Pending(handle)).unwrap());
        finish_checks(board);
    }
    assert!(board.internal_pending_choice().unwrap().is_none(), "all expected draw occurrences have resolved");
}

#[test]
fn base_experiment_draws_twice_and_grows_once_per_draw() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = initial["players"]["P1"]["zones"]["field"].take();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    let mut board = with_producer(&initial, draw(PlayerRef::Controller {}, 2));
    assert!(matches!(board.play_internal_card("experiment", actor()).unwrap(), CardPlay::Entered(_)));
    finish_checks(&mut board);
    assert_eq!(board.projection(View::Referee).unwrap()["P1"]["pp"]["current"], 5_u8);
    activate(&mut board, "P1");
    resolve_draws(&mut board, "BP05-094", 2);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["experiment"]["power"], 5_u8);
    assert_eq!(state["objects"]["experiment"]["hp"], 9_u8);
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 2);
    let opponent = board.projection(View::P2).unwrap().to_string();
    assert!(!opponent.contains("P1-first") && !opponent.contains("P1-second"), "drawn physical identities stay private");
}

#[test]
fn evolved_experiment_observes_its_own_two_draws_once_each() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP05-095"}]);
    let mut board = game(&initial);
    assert_eq!(board.internal_rule_action(&json!({"do":"evolve","source":"experiment","evolve_card":"form","pay":{"pp":2_u8}})).unwrap().outcome, "resolved");
    finish_checks(&mut board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else { panic!("evolve time pending"); };
    assert_eq!(options.len(), 1, "evolution first queues only its printed evolve-time draw");
    resolved(board.answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle)).unwrap());
    resolve_draws(&mut board, "BP05-095", 2);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["experiment"]["power"], 5_u8);
    assert_eq!(state["objects"]["experiment"]["hp"], 9_u8);
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
}

#[test]
fn opposing_draws_and_non_draw_hand_entries_do_not_grow_the_experiment() {
    for opposing in [true, false] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"returned","card":"BP01-037"}]);
        if opposing { initial["turn"]["active"] = json!("P2"); }
        let mut action = if opposing { draw(PlayerRef::Controller {}, 2) } else { movement(OwnerZone::Hand) };
        if let Action::Move {objects, ..} = &mut action { *objects = subjects_in(ZoneKind::Cemetery); }
        let mut board = with_producer(&initial, action);
        activate(&mut board, if opposing { "P2" } else { "P1" });
        resolve_draws(&mut board, "BP05-094", 0);
        let state = board.projection(View::Referee).unwrap();
        if opposing {
            assert_eq!(state["P2"]["hand"].as_array().unwrap().len(), 2, "the opposing player really drew two cards");
        } else {
            assert_eq!(state["P1"]["hand"], json!(["returned"]), "the non-draw movement really added the card to hand");
        }
        assert_eq!(state["objects"]["experiment"]["power"], 3_u8);
        assert_eq!(state["objects"]["experiment"]["hp"], 7_u8);
    }
}

#[test]
fn the_normal_start_phase_draw_adds_a_card_without_growing_the_experiment() {
    let mut initial = position();
    initial["turn"]["active"] = json!("P2");
    let mut board = game(&initial);
    board.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
    finish_checks(&mut board);
    if let Some(question) = board.internal_quick_choice().unwrap() {
        board.answer_internal_card(question.actor, question.token, Answer::Quick(None)).unwrap();
        finish_checks(&mut board);
    }
    if board.projection(View::Referee).unwrap()["turn"]["active"] == "P2" {
        board.internal_rule_action(&json!({"do":"end-discard","select":[]})).unwrap();
        finish_checks(&mut board);
    }
    resolve_draws(&mut board, "BP05-094", 0);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["turn"]["active"], "P1");
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(state["objects"]["experiment"]["power"], 3_u8);
    assert_eq!(state["objects"]["experiment"]["hp"], 7_u8);
}

#[test]
fn an_empty_deck_draw_does_not_queue_a_growth_before_deck_out_checks() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["deck"] = json!([]);
    let mut board = with_producer(&initial, draw(PlayerRef::Controller {}, 1));
    let activation = activate(&mut board, "P1");
    assert!(!activation.progress.facts.iter().any(|fact| matches!(fact, Fact::Draw(_))), "only cards actually drawn produce a fact");
    finish_checks(&mut board);
    assert!(board.internal_pending_choice().unwrap().is_none(), "all expected draw occurrences have resolved");
    assert_eq!(board.projection(View::Referee).unwrap()["objects"]["experiment"]["power"], 3_u8);
}

#[test]
fn prototype_draw_cards_cannot_bypass_the_existing_typed_authority_boundary() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"prototype-spell","card":"BP06-117"}]);
    let mut board = game(&initial);
    let before = format!("{board:?}");
    let error = board.decide(&json!({"do":"play","card":"prototype-spell"}), "prototype-draw").unwrap_err();
    assert!(error.to_string().contains("typed execution owns the input point"));
    assert!(format!("{board:?}") == before, "the rejected prototype input preserves all authority state");
}

#[test]
fn a_later_instruction_failure_restores_the_draw_and_its_queued_growth() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"].as_array_mut().unwrap().push(json!({"id":"producer","card":"producer"}));
    let mut board = Game::from_selected_effects(producer_selection().clone(), &initial, &Value::Null, &Value::Null, "draw-rollback").unwrap();
    let damage = Action::Damage {
        recipients: RecipientSet::Leaders { players: PlayerSet {
            relation: PlayerRelation::SelfPlayer, relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {minimum: Nat(1), maximum: Some(Nat(1))},
            text_quantifier: TextQuantifier::One,
        } },
        amount: DamageAmount::Uniform {amount: Expr::Literal(Int(i64::MAX))},
        flags: DamageFlags::Ability, source: ObjectView::SourceInformation {}, batch: BatchPolicy::Instruction {},
    };
    board.load_internal_card("producer", &producer_steps(vec![draw(PlayerRef::Controller {}, 1), damage.clone(), damage])).unwrap();
    let before = board.projection(View::Referee).unwrap();
    let error = board.activate_internal_card("producer", AbilityId(1), actor()).unwrap_err();
    assert!(error.to_string().contains("damage state arithmetic overflow"), "failure must occur after the successful draw, in the later damage instruction");
    assert_eq!(board.projection(View::Referee).unwrap(), before, "the actual draw and earlier damage roll back together");
    assert!(board.internal_pending_choice().unwrap().is_none(), "the failed execution leaves no growth occurrence");
}
