#![expect(
    clippy::indexing_slicing,
    clippy::default_numeric_fallback,
    clippy::panic,
    reason = "Fixed synthetic coordinates and questions must fail immediately when changed."
)]

use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{Activation, Context, DecisionView};
use sve_engine::ir::lowering::ExpandedLowerer;
use sve_engine::ir::{
    self, Action, Cardinality, Expr, Nat, Node, PlayerRef, PlayerRelation, PlayerSet,
    TextQuantifier, TokenDestination, WellFormedCardProgram,
};

fn tree(input: &Value) -> Value {
    let mut tree: Value =
        serde_saphyr::from_str(include_str!("../fixtures/lowering/activated.yaml")).unwrap();
    let mut entry = tree["cards"]["BP01-041"].take();
    let mut ability = entry["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("activate");
    ability["fanfare"]=input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]["abilities"][0]["fanfare"].clone();
    entry["abilities"] = json!([ability]);
    tree["cards"] = json!({"creator":entry});
    tree
}

fn legacy_catalog(snapshot: &str, keywords: &str, creator: &str, token: &str) -> Arc<Catalog> {
    let document = json!({"format":1,"kind":"effect_set","cards":{
        (creator):{"status":"complete","review":"synthetic quantity fragment harness","abilities":[]},
        (token):{"status":"complete","review":"empty token fragment harness","abilities":[],"token_template":true}
    }}).to_string();
    Arc::new(Catalog::from_documents(snapshot, keywords, &[("fragment".into(), document)]).unwrap())
}

fn legacy_game(catalog: Arc<Catalog>, position: &Value, token: &str) -> Game {
    let mut game = Game::new(
        catalog,
        position,
        &Value::Null,
        &Value::Null,
        "token-fragment",
    )
    .unwrap();
    let empty = ir::CardProgram {
        card_play_costs: vec![],
        abilities: vec![],
    };
    game.load_internal_card(token, &WellFormedCardProgram::check(empty).unwrap())
        .unwrap();
    game
}

fn input() -> (String, String, Value) {
    let (_, keywords, mut input) = selection(1);
    let (creator, source) = author(
        "creator",
        "11111111111111111111111111111111",
        "フォロワー",
        vec![
            json!({"fanfare":{"x":{},"optional_cost":{"pp":"x","result":"paid"},
            "do":[{"create":{"token":"token","to":"field","n":"x","result":"made"}}]}}),
        ],
    );
    let (token, _) = author(
        "token",
        "22222222222222222222222222222222",
        "フォロワー・トークン",
        vec![],
    );
    input["authors"][0] = source;
    (format!("{creator}\n{token}\n"), keywords, input)
}

fn position(occupied: usize) -> Value {
    let mut position = initial(occupied);
    position["players"]["P1"]["zones"]["field"] = position["players"]["P1"]["zones"]["ex"].take();
    position["players"]["P1"]["zones"]["ex"] = json!([]);
    position
}

fn game(occupied: usize) -> Game {
    let (snapshot, keywords, input) = input();
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    Game::from_selected_effects(
        selected,
        &position(occupied),
        &Value::Null,
        &Value::Null,
        "paid-X-token",
    )
    .unwrap()
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected paid X question")
    };
    question
}

fn resolved(play: CardPlay) -> Activation {
    let CardPlay::Resolved(result) = play else {
        panic!("one template truncates without a capacity question")
    };
    result
}

fn optional(game: &mut Game) -> DecisionView {
    assert!(
        matches!(
            game.play_internal_card("creator", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the follower enters before its Fanfare"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("fanfare occurrence")
    };
    assert_eq!(options.len(), 1, "only the quantity fragment triggers");
    let optional = awaiting(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        optional.domain,
        InputDomain::OptionalCost,
        "payment is declared before choosing X"
    );
    optional
}

fn created(result: &Activation) -> usize {
    result
        .progress
        .receipts
        .values()
        .filter_map(|outcome| {
            if let InstructionOutcome::Token(outcome) = outcome {
                Some(outcome.created.len())
            } else {
                None
            }
        })
        .sum()
}

fn last_count(program: &mut ir::Program) -> &mut Expr<Nat> {
    let Node::Instruction { instruction } = &mut program.steps.last_mut().unwrap().node else {
        panic!("last token instruction")
    };
    let Action::Token { tokens, .. } = &mut instruction.action else {
        panic!("token action")
    };
    &mut tokens[0].count
}

#[test]
fn optional_paid_x_decline_zero_and_capacity_keep_one_payment_and_one_answer() {
    for (accept, x, occupied, expected) in [
        (false, 0, 0, 0),
        (true, 0, 0, 0),
        (true, 3, 0, 3),
        (true, 7, 3, 1),
    ] {
        let mut game = game(occupied);
        let declaration = optional(&mut game);
        let play = game
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::OptionalCost(accept),
            )
            .unwrap();
        let result = if accept {
            let question = awaiting(play);
            let InputDomain::Integer(bounds) = question.domain else {
                panic!("X integer")
            };
            assert_eq!(bounds.maximum, Some(Nat(9)), "printed PP was already paid");
            let before = format!("{game:?}");
            for (actor, value) in [(PlayerId { seat: 2 }, 1), (question.actor, 10)] {
                game.answer_internal_card(actor, question.token, Answer::Integer(Nat(value)))
                    .unwrap_err();
                assert_eq!(
                    format!("{game:?}"),
                    before,
                    "invalid answer cannot pay or create"
                );
            }
            let result = resolved(
                game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(x)))
                    .unwrap(),
            );
            let completed = format!("{game:?}");
            game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(x)))
                .unwrap_err();
            assert_eq!(
                format!("{game:?}"),
                completed,
                "a stale answer cannot pay twice"
            );
            result
        } else {
            resolved(play)
        };
        assert_eq!(created(&result), expected);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"],
            9 - x,
            "capacity never refunds paid X"
        );
        assert_eq!(
            view["P1"]["field"].as_array().unwrap().len(),
            occupied + 1 + expected
        );
        assert!(result.progress.complete);
    }
}

#[test]
fn token_x_requires_its_own_pp_backing_and_controller_destination() {
    let (snapshot, keywords, input) = input();
    let program = ExpandedLowerer::new()
        .unwrap()
        .lower(&tree(&input))
        .unwrap()
        .remove("creator")
        .unwrap();
    let catalog = legacy_catalog(&snapshot, &keywords, "creator", "token");
    let mut positive = legacy_game(Arc::clone(&catalog), &position(0), "token");
    positive.load_internal_card("creator", &program).unwrap();
    for opponent in [false, true] {
        let mut card = program.card().clone();
        let ability = &mut card.abilities[0].node;
        let body = ability.body.as_mut().unwrap();
        if opponent {
            let token = body.steps.last_mut().unwrap();
            let Node::Instruction { instruction } = &mut token.node else {
                panic!("token instruction")
            };
            let Action::Token {
                destination:
                    TokenDestination::Single {
                        zone: Expr::Literal(zone),
                    },
                ..
            } = &mut instruction.action
            else {
                panic!("token destination")
            };
            zone.player = PlayerRef::RequireOne {
                players: Box::new(PlayerSet {
                    relation: PlayerRelation::Opponents,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::One,
                }),
            };
        } else {
            let Node::Control {
                control: ir::Control::Pay { costs, .. },
            } = &mut body.steps[0].node
            else {
                panic!("optional payment")
            };
            let ir::CostSpec::Ordered { steps } = costs else {
                panic!("ordered PP cost")
            };
            let Action::Resource { amount, .. } = &mut steps[0].instruction.action else {
                panic!("PP payment")
            };
            *amount = Expr::Literal(ir::Int(0));
        }
        let candidate = WellFormedCardProgram::check(card).unwrap();
        let mut game = legacy_game(Arc::clone(&catalog), &position(0), "token");
        let before = format!("{game:?}");
        assert!(matches!(
            game.load_internal_card("creator", &candidate),
            Err(EngineFailure::Unsupported(_))
        ));
        assert_eq!(
            format!("{game:?}"),
            before,
            "failed admission preserves all authority"
        );
    }
}

#[test]
fn an_unrelated_integer_answer_is_not_a_paid_x_in_a_raw_program() {
    use sve_engine::ir::{Binding, ChoiceDomain, DecisionKind};
    let (snapshot, keywords, input) = input();
    let program = ExpandedLowerer::new()
        .unwrap()
        .lower(&tree(&input))
        .unwrap()
        .remove("creator")
        .unwrap();
    let catalog = legacy_catalog(&snapshot, &keywords, "creator", "token");
    let mut card = program.card().clone();
    let ability = &mut card.abilities[0].node;
    let mut decision = ability.play.as_ref().unwrap().parameters[0].clone();
    decision.id = ir::DecisionId(999);
    let body = ability.body.as_mut().unwrap();
    let binding = Binding::new(999, body.scope);
    let bounds = Cardinality {
        minimum: Nat(0),
        maximum: Some(Nat(2)),
    };
    decision.domain = ChoiceDomain::Integer {
        cardinality: bounds,
        binding,
    };
    decision.kind = DecisionKind::ResolutionChoice {
        may: ir::Declinable::Required,
        cardinality: bounds,
        precondition: None,
    };
    let token = body.steps.last().unwrap();
    let source = token.source.clone();
    *last_count(body) = Expr::Bound(binding);
    body.steps.insert(
        1,
        ir::Located {
            source: source.clone(),
            node: Node::Decision { decision },
        },
    );
    let mut raw = body.clone();
    raw.steps.remove(0);
    let Node::Instruction { instruction } = &mut raw.steps.last_mut().unwrap().node else {
        panic!("raw token instruction")
    };
    if let ir::AbilityOrigin::Keyword { origin, .. } = &instruction.source.origin {
        instruction.source.origin = (**origin).clone();
    }
    let mut game = legacy_game(Arc::clone(&catalog), &position(0), "token");
    game.load_internal_card(
        "creator",
        &WellFormedCardProgram::check(ir::CardProgram {
            card_play_costs: vec![],
            abilities: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    let before = format!("{game:?}");
    let raw = ir::WellFormedProgram::check(raw).unwrap();
    let context = Context {
        source: "creator".into(),
        generation: ir::Generation(0),
        controller: PlayerId { seat: 1 },
    };
    assert!(matches!(
        game.execute_internal(&raw, &source, context),
        Err(EngineFailure::Unsupported(_))
    ));
    assert_eq!(format!("{game:?}"), before);
    let mut positive = raw.program().clone();
    *last_count(&mut positive) = Expr::Literal(Nat(1));
    let positive = ir::WellFormedProgram::check(positive).unwrap();
    let pending = game
        .execute_internal(
            &positive,
            &source,
            Context {
                source: "creator".into(),
                generation: ir::Generation(0),
                controller: PlayerId { seat: 1 },
            },
        )
        .unwrap();
    assert!(!pending.complete);
    let question = game.internal_decision(View::Referee).unwrap();
    let progress = game
        .answer_internal(question.actor, question.token, Answer::Integer(Nat(1)))
        .unwrap();
    assert!(progress.complete);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn a_captured_natural_does_not_inherit_the_cards_paid_x_proof() {
    use sve_engine::ir::{Binding, Capture, CaptureRef, CaptureTime, Located};
    let (snapshot, keywords, input) = input();
    let program = ExpandedLowerer::new()
        .unwrap()
        .lower(&tree(&input))
        .unwrap()
        .remove("creator")
        .unwrap();
    let catalog = legacy_catalog(&snapshot, &keywords, "creator", "token");
    for captured in [false, true] {
        let mut card = program.card().clone();
        let body = card.abilities[0].node.body.as_mut().unwrap();
        let reference = CaptureRef {
            binding: Binding::new(999, body.scope),
        };
        let token = body.steps.last_mut().unwrap();
        let source = token.source.clone();
        let Node::Instruction { instruction } = &mut token.node else {
            panic!("token instruction")
        };
        let Action::Token { tokens, .. } = &mut instruction.action else {
            panic!("token action")
        };
        tokens[0].count = if captured {
            Expr::Captured(reference)
        } else {
            Expr::Literal(Nat(1))
        };
        body.steps.insert(
            1,
            Located {
                source,
                node: Node::Capture {
                    capture: Capture::Natural {
                        at: CaptureTime::AtResolution { scope: body.scope },
                        value: Expr::Literal(Nat(1)),
                        into: reference,
                    },
                },
            },
        );
        let checked = WellFormedCardProgram::check(card).unwrap();
        let mut game = legacy_game(Arc::clone(&catalog), &position(0), "token");
        let before = format!("{game:?}");
        let result = game.load_internal_card("creator", &checked);
        if captured {
            assert!(matches!(result, Err(EngineFailure::Unsupported(_))));
            assert_eq!(format!("{game:?}"), before);
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn paid_x_count_composition_does_not_authorize_each_player_scopes() {
    for (count, expected) in [
        (json!({"plus":["x",0]}), 3),
        (json!({"per":{"of":"x","unit":2}}), 1),
    ] {
        let (snapshot, keywords, mut input) = input();
        let body = &mut input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
            ["abilities"][0]["fanfare"];
        body["do"][0]["create"]["n"] = count;
        let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
        let mut game = Game::from_selected_effects(
            selected,
            &position(0),
            &Value::Null,
            &Value::Null,
            "X-composition",
        )
        .unwrap();
        let question = optional(&mut game);
        let question = awaiting(
            game.answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
                .unwrap(),
        );
        let result = resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(3)))
                .unwrap(),
        );
        assert_eq!(created(&result), expected);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            6
        );
    }
    let (snapshot, keywords, mut input) = input();
    input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
        ["abilities"][0]["fanfare"]["do"] = json!([
        {"each_player":{"as":"p","stages":[[{"create":{"token":"token","to":"field","by":"p","n":"x"}}]]}}
    ]);
    let rejected_selection = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    assert!(matches!(
        Game::from_selected_effects(
            rejected_selection,
            &position(0),
            &Value::Null,
            &Value::Null,
            "X-stage"
        ),
        Err(EngineFailure::Unsupported(_))
    ));
    input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
        ["abilities"][0]["fanfare"]["do"][0]["each_player"]["stages"][0][0]["create"]["n"] =
        json!(1);
    let accepted_selection = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    Game::from_selected_effects(
        accepted_selection,
        &position(0),
        &Value::Null,
        &Value::Null,
        "literal-stage",
    )
    .unwrap();
}

#[cfg(feature = "runner")]
#[test]
fn bp11_032_fanfare_quantity_fragment_keeps_real_printed_cost_and_steel_knight() {
    use std::{env::var_os, fs::read_to_string};
    let snapshot = read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap();
    let mut creator = None;
    let mut token = None;
    for line in snapshot.lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        if record["number"] == "BP11-032" {
            creator = Some(record.clone());
        }
        if token.is_none()
            && record["faces"][0]["name"] == "スティールナイト"
            && record["faces"][0]["card_type"]
                .as_str()
                .is_some_and(|kind| kind.contains("トークン"))
        {
            token = Some(record);
        }
    }
    let creator = creator.unwrap();
    let token = token.unwrap();
    let token_no = token["number"].as_str().unwrap();
    let (_, keywords, input) = input();
    let mut tree = tree(&input);
    let mut entry = tree["cards"]["creator"].take();
    entry["abilities"][0]["fanfare"]["do"][0]["create"]["token"] = json!("スティールナイト");
    tree["cards"] = json!({"BP11-032":entry});
    let program = ExpandedLowerer::new()
        .unwrap()
        .lower(&tree)
        .unwrap()
        .remove("BP11-032")
        .unwrap();
    let catalog = legacy_catalog(
        &format!("{creator}\n{token}\n"),
        &keywords,
        "BP11-032",
        token_no,
    );
    for (accept, x, occupied, expected) in [(false, 0, 0, 0), (true, 0, 0, 0), (true, 7, 3, 1)] {
        let mut position = position(occupied);
        position["players"]["P1"]["zones"]["hand"][0]["card"] = json!("BP11-032");
        for card in position["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
        {
            card["card"] = json!(token_no);
        }
        let mut game = legacy_game(Arc::clone(&catalog), &position, token_no);
        game.load_internal_card("BP11-032", &program).unwrap();
        let declaration = optional(&mut game);
        let play = game
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::OptionalCost(accept),
            )
            .unwrap();
        let play = if accept {
            let question = awaiting(play);
            let InputDomain::Integer(bounds) = question.domain else {
                panic!("printed PP-backed X")
            };
            assert_eq!(bounds.maximum, Some(Nat(8)));
            game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(x)))
                .unwrap()
        } else {
            play
        };
        assert_eq!(created(&resolved(play)), expected);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 8 - x);
        assert_eq!(view["objects"]["creator"]["power"], 2);
        assert_eq!(view["objects"]["creator"]["hp"], 3);
        let new = view["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|id| {
                let id = id.as_str().unwrap();
                id != "creator" && !id.starts_with("old-")
            })
            .collect::<Vec<_>>();
        assert_eq!(new.len(), expected);
        for id in new {
            assert_eq!(view["objects"][id.as_str().unwrap()]["card"], token_no);
        }
    }
}
