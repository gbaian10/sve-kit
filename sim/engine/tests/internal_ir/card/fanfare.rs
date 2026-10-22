use super::*;
use std::env::var_os;
use std::fs::read_to_string;
use sve_engine::game::internal::{InputDomain, SearchAnswer};
use sve_engine::ir::lowering::ExpandedLowerer;

#[path = "../../support/expanded.rs"]
pub(super) mod expanded;

fn pending(game: &mut Game) -> Result<CardPlay, sve_engine::EngineFailure> {
    let question = game
        .internal_pending_choice()?
        .ok_or_else(|| sve_engine::EngineFailure::Invalid("no pending occurrence".into()))?;
    game.answer_internal_card(question.actor, question.token, Answer::Pending(1))
}

fn position(seat: u32, program: &WellFormedCardProgram) -> Game {
    position_for_card(seat, program, "BP01-118", 2)
}

pub(super) fn position_for_card(
    seat: u32,
    program: &WellFormedCardProgram,
    card_no: &str,
    pp: i64,
) -> Game {
    let mut game = fragment_position(seat, card_no, pp, json!([]));
    game.load_internal_card(card_no, program).unwrap();
    game
}

fn fragment_position(seat: u32, card_no: &str, pp: i64, deck: Value) -> Game {
    let snapshot = read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let mut effects = json!({"format":1,"kind":"effect_set","cards":{
        (card_no):{"status":"complete","review":"candidate execution harness","abilities":[]}
    }});
    for card in deck.as_array().unwrap() {
        effects["cards"][card["card"].as_str().unwrap()] =
            json!({"status":"complete","review":"candidate execution harness","abilities":[]});
    }
    let effects = effects.to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("candidate".into(), effects)]).unwrap(),
    );
    let actor = if seat == 1 { "P1" } else { "P2" };
    let opponent = if seat == 1 { "P2" } else { "P1" };
    let mut initial = setup(seat, deck);
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["pp"] = json!({"current":pp,"max":pp});
    initial["players"][actor]["zones"]["hand"] = json!([{"id":"s","card":card_no}]);
    initial["players"][opponent]["zones"]["field"] = json!([]);
    Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "fanfare-seed",
    )
    .unwrap()
}

#[test]
fn bp03_002_lowered_search_runs_in_synthetic_harness() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower(expanded::tree())
        .unwrap();
    let mut game = fragment_position(1, "BP03-002", 5, json!([{"id":"d0","card":"BP03-009"}]));
    assert!(
        matches!(game.load_internal_card("BP03-002", &cards["BP03-002"]), Err(sve_engine::EngineFailure::Unsupported(message)) if message.contains("whole-card synchronous action"))
    );
    let ability = &cards["BP03-002"].card().abilities[0];
    let program = synthetic_body(&ability.node);
    game.execute_internal(&program, &ability.source, context(1))
        .unwrap();
    let question = game.internal_decision(View::P1).unwrap();
    assert!(matches!(question.domain, InputDomain::Search(_)));
    game.answer_internal(
        question.actor,
        question.token,
        Answer::Search(SearchAnswer::Ungrouped(vec![0])),
    )
    .unwrap();
    let question = game.internal_decision(View::P1).unwrap();
    assert_eq!(question.domain, InputDomain::SearchReplacement);
    let result = game
        .answer_internal(
            question.actor,
            question.token,
            Answer::SearchReplacement(SearchReplacementAnswer::Replacement),
        )
        .unwrap();
    assert!(result.complete);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["d0"])
    );
    assert_eq!(
        result
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Shuffled { .. }))
            .count(),
        1
    );
}

#[expect(
    clippy::panic,
    reason = "A fixture outside the synthetic harness's instruction subset must fail."
)]
fn synthetic_body(ability: &Ability) -> WellFormedProgram {
    let original = ability.body.as_ref().unwrap();
    let mut body = original.clone();
    // D1's fragment harness has no keyword producer; production admission remains closed.
    for step in &mut body.steps {
        let Node::Instruction { instruction } = &mut step.node else {
            panic!("instruction body");
        };
        let AbilityOrigin::Keyword { origin, .. } = &instruction.source.origin else {
            panic!("keyword source");
        };
        instruction.source.origin = *origin.clone();
    }
    let mut restored = body.clone();
    for step in &mut restored.steps {
        let Node::Instruction { instruction } = &mut step.node else {
            panic!("instruction body");
        };
        instruction.source.origin = ability.source.origin.clone();
    }
    assert_eq!(
        &restored, original,
        "the harness must change only the keyword origin"
    );
    WellFormedProgram::check(body).unwrap()
}

#[test]
fn bp11_099_author_activation_pays_before_modifying_the_holder() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower(expanded::tree())
        .unwrap();
    let program = &cards["BP11-099"];
    let mut game = position_for_card(1, program, "BP11-099", 8);
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    game.resume_internal_card().unwrap();
    let result = resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    assert!(result.progress.complete);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 0);
    assert_eq!(view["objects"]["s"]["power"], 6);
    assert_eq!(view["objects"]["s"]["hp"], 5);
    game.resume_internal_card().unwrap();
    let before = exact_authority(&game);
    game.activate_internal_card(
        "s",
        program.card().abilities[0].node.id,
        PlayerId { seat: 1 },
    )
    .unwrap_err();
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn bp01_118_author_yaml_reaches_authority_fanfare_once_after_entry() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower(expanded::tree())
        .unwrap();
    let program = &cards["BP01-118"];
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let mut game = position(seat, program);
        let before = exact_authority(&game);
        pending(&mut game).unwrap_err();
        game.play_internal_card("s", PlayerId { seat: 3 - seat })
            .unwrap_err();
        game.submit(&json!({"do":"play","card":"s"})).unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let CardPlay::Entered(step) = game.play_internal_card("s", PlayerId { seat }).unwrap()
        else {
            panic!("follower entry");
        };
        assert!(
            step.step
                .events
                .iter()
                .any(|event| event["kind"] == "場に出す")
        );
        assert_eq!(game.internal_card_stage(), Some(ResumeStage::Pending));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view[actor]["pp"]["current"], 1);
        assert_eq!(view["P1"]["leader"]["life"], 20);
        assert_eq!(game.legal().unwrap(), Vec::<Value>::new());
        game.activate_internal_card("s", program.card().abilities[0].node.id, PlayerId { seat })
            .unwrap_err();
        let result = resolved(pending(&mut game).unwrap());
        assert_eq!(result.source.holder.generation, Generation(2));
        assert_eq!(result.source.controller, PlayerId { seat });
        assert_eq!(result.progress.facts.len(), 4);
        assert!(
            matches!(result.progress.facts.first(), Some(Fact::AbilityPlayed { source, execution }) if *source == result.source && *execution == result.execution)
        );
        assert!(
            matches!(result.progress.facts.last(), Some(Fact::AbilityResolved { source, execution }) if *source == result.source && *execution == result.execution)
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], 19);
        assert_eq!(view["P2"]["leader"]["life"], 19);
        pending(&mut game).unwrap_err();
        game.resume_internal_card().unwrap();
        assert_eq!(game.internal_card_stage(), None);
        assert_eq!(
            game.projection(View::Referee).unwrap()["turn"]["phase"],
            "main"
        );
    }
}

#[test]
fn failed_fanfare_keeps_paid_card_and_pending_producer_without_partial_damage() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower(expanded::tree())
        .unwrap();
    let mut program = cards["BP01-118"].card().clone();
    let ability = &mut program.abilities[0];
    let Node::Instruction { instruction } = &mut ability.node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("damage instruction");
    };
    let Action::Damage { amount, .. } = &mut instruction.action else {
        panic!("damage");
    };
    *amount = DamageAmount::Uniform {
        amount: Expr::Compute(Box::new(IntOperation::Arithmetic {
            operation: Arithmetic::Add,
            arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
        })),
    };
    let program = WellFormedCardProgram::check(program).unwrap();
    let mut game = position(1, &program);
    // An otherwise valid synthetic body still cannot bypass the loaded fanfare producer.
    game.execute_internal(&checked(vec![]), &source("synthetic"), context(1))
        .unwrap_err();
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    game.internal_pending_choice().unwrap().unwrap();
    let before = exact_authority(&game);
    let views = [View::P1, View::P2, View::Referee].map(|view| game.projection(view).unwrap());
    pending(&mut game).unwrap_err();
    assert_eq!(before, exact_authority(&game));
    assert_eq!(
        views,
        [View::P1, View::P2, View::Referee].map(|view| game.projection(view).unwrap())
    );
    assert_eq!(game.internal_card_stage(), Some(ResumeStage::Pending));
}

#[test]
fn bp08_067_author_fragment_destroys_itself_at_own_end_phase() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower(expanded::tree())
        .unwrap();
    let mut game = position_for_card(1, &cards["BP08-067"], "BP08-067", 10);
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    game.resume_internal_card().unwrap();
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let result = resolved(pending(&mut game).unwrap());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["s"])
    );
    assert!(
        result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Movement(m) if m.method == MoveMethod::Destroy))
    );
}
