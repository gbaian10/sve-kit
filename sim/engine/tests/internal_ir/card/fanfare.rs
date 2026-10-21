use super::*;
use std::env::var_os;
use std::fs::read_to_string;
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
    let snapshot = read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let effects = json!({"format":1,"kind":"effect_set","cards":{
        (card_no):{"status":"complete","review":"candidate execution harness","abilities":[]}
    }})
    .to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("candidate".into(), effects)]).unwrap(),
    );
    let actor = if seat == 1 { "P1" } else { "P2" };
    let opponent = if seat == 1 { "P2" } else { "P1" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["pp"] = json!({"current":pp,"max":pp});
    initial["players"][actor]["zones"]["hand"] = json!([{"id":"s","card":card_no}]);
    initial["players"][opponent]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "fanfare-seed",
    )
    .unwrap();
    game.load_internal_card(card_no, program).unwrap();
    game
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
