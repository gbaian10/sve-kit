#![expect(
    clippy::panic,
    reason = "Unexpected public decisions invalidate these Look selection regressions."
)]
use super::*;
use std::{path::PathBuf, process::Command, sync::OnceLock};
use sve_engine::game::internal::{DecisionView, InputDomain};

fn tree() -> &'static Value {
    static TREE: OnceLock<Value> = OnceLock::new();
    TREE.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = Command::new("uv")
            .args(["run", "--directory"])
            .arg(root.join("carddb"))
            .args([
                "--locked",
                "--no-default-groups",
                "python",
                "-c",
                "
import json
import sys
from pathlib import Path
from sve_carddb.domains.effects.author_expansion import AuthorExpander
result = AuthorExpander(Path(sys.argv[1])).expand(Path(sys.argv[2]).read_bytes(), [])
print(json.dumps(result.tree))
",
            ])
            .arg(root.join("dsl"))
            .arg(root.join("sim/engine/tests/fixtures/lowering/look-names.yaml"))
            .output()
            .unwrap();
        assert!(output.status.success(), "C0 Look fixture expansion failed");
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

fn lowered_program(document: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(document)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn catalog_for_names() -> Arc<Catalog> {
    let (snapshot, registry, mut legacy) = catalog_documents(&json!("1"));
    let template: Value = serde_json::from_str(snapshot.lines().nth(1).unwrap()).unwrap();
    let mut faces = snapshot
        .lines()
        .filter(|line| serde_json::from_str::<Value>(line).unwrap()["number"] != "unit-follower")
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut effects: Value = serde_json::from_str(&legacy).unwrap();
    for (number, name, traits) in [
        ("unit-follower", "source", vec!["透京"]),
        ("a", "alpha", vec!["機械", "透京"]),
        ("a-print", "alpha", vec!["機械"]),
        ("b", "beta", vec!["機械"]),
        ("c", "gamma", vec!["透京"]),
        ("d", "delta", vec![]),
        ("alias", "alias", vec![]),
        ("literal", "ソニック・フォー", vec!["機械"]),
        ("literal2", "戒信の六銘伝・聖士郎", vec![]),
    ] {
        let mut face = template.clone();
        face["number"] = json!(number);
        face["faces"][0]["name"] = json!(name);
        face["faces"][0]["traits"] = json!(traits);
        face["faces"][0]["card_class"] = json!("ナイトメア");
        faces.push(face.to_string());
        effects["cards"][number] = json!({"status":"complete","abilities":[]});
    }
    legacy = effects.to_string();
    Arc::new(
        Catalog::from_documents(&faces.join("\n"), &registry, &[("names".into(), legacy)]).unwrap(),
    )
}

fn game(program: &WellFormedCardProgram, deck: &[&str], occupied: usize) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        deck.iter()
            .enumerate()
            .map(|(index, card)| { json!({"id":format!("d{index}"),"card":card}) })
            .collect::<Vec<_>>()
    );
    let mut field = vec![json!({"id":"s","card":"unit-follower"})];
    field.extend(
        (1..occupied).map(|index| json!({"id":format!("f{index}"),"card":"unit-follower"})),
    );
    initial["players"]["P1"]["zones"]["field"] = json!(field);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut board = Game::new(
        catalog_for_names(),
        &initial,
        &Value::Null,
        &Value::Null,
        "look-names",
    )
    .unwrap();
    board.load_internal_card("unit-follower", program).unwrap();
    for number in ["a", "a-print", "b", "c", "d", "literal", "literal2"] {
        board.load_internal_card(number, &card(vec![])).unwrap();
    }
    let alias = program_for_alias();
    board.load_internal_card("alias", &alias).unwrap();
    board
}

fn program_for_alias() -> WellFormedCardProgram {
    lowerer()
        .lower(&same_name::tree(
            &json!({"static":{"effects":[{"also_named":{"name":"beta"}}]}}),
            "alias",
        ))
        .unwrap()
        .remove("alias")
        .unwrap()
}

fn start(board: &mut Game, program: &WellFormedCardProgram, index: usize) -> DecisionView {
    let play = board
        .activate_internal_card(
            "s",
            program.card().abilities[index].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap();
    let CardPlay::Awaiting(question) = play else {
        panic!("private Look choice")
    };
    assert!(
        board.internal_decision(View::P2).is_none(),
        "Look choices stay private to their controller"
    );
    question
}

fn handles(question: &DecisionView, card: &str) -> Vec<u64> {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("Look target domain")
    };
    domain
        .members
        .iter()
        .filter(|member| member.card.as_deref() == Some(card))
        .map(|member| member.handle)
        .collect()
}

fn answer(board: &mut Game, question: &DecisionView, picked: Vec<u64>) {
    let play = board
        .answer_internal_card(question.actor, question.token, Answer::Targets(picked))
        .unwrap();
    finish(board, play);
}

fn finish(board: &mut Game, play: CardPlay) {
    if let CardPlay::Awaiting(order) = play {
        let InputDomain::Order(cards) = order.domain else {
            panic!("private remainder order")
        };
        resolved(
            board
                .answer_internal_card(
                    order.actor,
                    order.token,
                    Answer::Order(cards.iter().map(|member| member.handle).collect()),
                )
                .unwrap(),
        );
    } else {
        resolved(play);
    }
}

#[test]
fn bp08_clause_picks_four_names_and_rejects_reprints_aliases_and_reuse() {
    let program = lowered_program(tree());
    let mut board = game(&program, &["a", "a-print", "b", "alias", "c", "d"], 1);
    let question = start(&mut board, &program, 0);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private names")
    };
    assert!(domain.distinct_names);
    assert_eq!(domain.cardinality.maximum, Some(Nat(4)));
    let a = handles(&question, "a")[0];
    let b = handles(&question, "b")[0];
    for invalid in [
        vec![a, a],
        vec![a, handles(&question, "a-print")[0]],
        vec![b, handles(&question, "alias")[0]],
    ] {
        board
            .answer_internal_card(question.actor, question.token, Answer::Targets(invalid))
            .unwrap_err();
    }
    answer(
        &mut board,
        &question,
        vec![a, b, handles(&question, "c")[0], handles(&question, "d")[0]],
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["s", "d0", "d2", "d4", "d5"]));
    assert_eq!(state["P1"]["deck"], json!(["d1", "d3"]));
}

#[test]
fn names_look_keeps_empty_short_and_field_capacity_resolution() {
    let program = lowered_program(tree());
    for (deck, occupied, maximum, picked_count) in [
        (vec![], 1, 4, 0),
        (vec!["a", "a-print"], 1, 4, 1),
        (vec!["a", "b", "c", "d"], 3, 2, 2),
    ] {
        let mut board = game(&program, &deck, occupied);
        let question = start(&mut board, &program, 0);
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("clamped names")
        };
        assert_eq!(domain.cardinality.maximum, Some(Nat(maximum)));
        assert_eq!(domain.cardinality.minimum, Nat(0));
        if occupied == 3 {
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(domain.members.iter().map(|member| member.handle).collect()),
                )
                .unwrap_err();
        }
        let picked = domain
            .members
            .iter()
            .take(picked_count)
            .map(|member| member.handle)
            .collect();
        answer(&mut board, &question, picked);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["field"]
                .as_array()
                .unwrap()
                .len(),
            occupied + picked_count
        );
    }
}

#[test]
fn same_look_filtered_union_names_uses_one_overlapping_population() {
    let mut document = tree().clone();
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["choose"]["from"] =
        json!({"any_of":[{"in":"seen","trait":"機械"},{"in":"seen","trait":"透京"}]});
    let program = lowered_program(&document);
    let mut board = game(&program, &["a", "a-print", "b", "c", "d"], 1);
    let question = start(&mut board, &program, 0);
    assert_eq!(handles(&question, "d"), Vec::<u64>::new());
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![
                handles(&question, "a")[0],
                handles(&question, "a-print")[0],
            ]),
        )
        .unwrap_err();
    answer(
        &mut board,
        &question,
        vec![
            handles(&question, "a")[0],
            handles(&question, "b")[0],
            handles(&question, "c")[0],
        ],
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["s", "d0", "d2", "d3"])
    );
}

#[test]
fn bp17_clause_assigns_literal_and_trait_groups_once_and_reveals_only_picked_cards() {
    let program = lowered_program(tree());
    let mut board = game(&program, &["literal", "a", "b", "d"], 1);
    let question = start(&mut board, &program, 1);
    let literal = handles(&question, "literal")[0];
    for invalid in [
        vec![literal, literal],
        vec![handles(&question, "a")[0], handles(&question, "b")[0]],
    ] {
        board
            .answer_internal_card(question.actor, question.token, Answer::Targets(invalid))
            .unwrap_err();
    }
    answer(
        &mut board,
        &question,
        vec![literal, handles(&question, "a")[0]],
    );
    let public = board.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    for id in ["d0", "d1"] {
        assert!(known.iter().any(|card| card["id"] == id));
    }
    for id in ["d2", "d3"] {
        assert!(!known.iter().any(|card| card["id"] == id));
    }
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d0", "d1"])
    );
}

#[test]
fn literal_groups_keep_zero_and_missing_group_candidates_legal() {
    let program = lowered_program(tree());
    for deck in [vec![], vec!["a"], vec!["d"]] {
        let mut board = game(&program, &deck, 1);
        let question = start(&mut board, &program, 1);
        answer(&mut board, &question, vec![]);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["hand"],
            json!([])
        );
    }
    let mut board = game(&program, &["a", "b"], 1);
    let question = start(&mut board, &program, 1);
    answer(&mut board, &question, vec![handles(&question, "a")[0]]);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d0"])
    );
}

#[test]
fn bp18_clause_freezes_source_name_before_departure_and_excludes_it_from_all_groups() {
    let mut document = tree().clone();
    document["cards"]["unit-follower"]["abilities"][2]["activate"]["cost"] =
        json!({"self_to_cemetery":true});
    let program = lowered_program(&document);
    let mut board = game(&program, &["unit-follower", "a", "literal2", "d"], 1);
    let question = start(&mut board, &program, 2);
    assert_eq!(handles(&question, "unit-follower"), Vec::<u64>::new());
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["s"])
    );
    answer(
        &mut board,
        &question,
        vec![
            handles(&question, "a")[0],
            handles(&question, "literal2")[0],
        ],
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d1", "d2"])
    );

    let mut document = tree().clone();
    document["cards"]["unit-follower"]["abilities"][2]["activate"]["do"][1]["choose"]["groups"]
        [1]["name"] = json!("source");
    let program = lowered_program(&document);
    let mut board = game(&program, &["unit-follower", "a"], 1);
    let question = start(&mut board, &program, 2);
    assert!(
        handles(&question, "unit-follower").is_empty(),
        "global exclusion also applies to the literal group"
    );
    answer(&mut board, &question, vec![handles(&question, "a")[0]]);
}

#[test]
fn private_literal_groups_do_not_change_opponent_packets_before_or_after_selected_reveal() {
    let program = lowered_program(tree());
    let mut left = game(&program, &["literal", "a", "b", "d"], 1);
    let mut right = game(&program, &["literal", "a", "d", "d"], 1);
    let lq = start(&mut left, &program, 1);
    let rq = start(&mut right, &program, 1);
    assert_ne!(handles(&lq, "b"), handles(&rq, "b"));
    assert_eq!(
        left.projection(View::P2).unwrap(),
        right.projection(View::P2).unwrap()
    );
    let lp = left
        .answer_internal_card(
            lq.actor,
            lq.token,
            Answer::Targets(vec![handles(&lq, "literal")[0], handles(&lq, "a")[0]]),
        )
        .unwrap();
    let rp = right
        .answer_internal_card(
            rq.actor,
            rq.token,
            Answer::Targets(vec![handles(&rq, "literal")[0], handles(&rq, "a")[0]]),
        )
        .unwrap();
    assert!(matches!(&lp, CardPlay::Awaiting(_)));
    assert!(matches!(&rp, CardPlay::Awaiting(_)));
    assert!(left.internal_decision(View::P2).is_none());
    assert!(right.internal_decision(View::P2).is_none());
    let public = left.projection(View::P2).unwrap();
    assert_eq!(public, right.projection(View::P2).unwrap());
    let known = public["known_cards"].as_array().unwrap();
    for id in ["d0", "d1"] {
        assert!(known.iter().any(|card| card["id"] == id));
    }
    for id in ["d2", "d3"] {
        assert!(!known.iter().any(|card| card["id"] == id));
    }
    finish(&mut left, lp);
    finish(&mut right, rp);
    assert_eq!(
        left.projection(View::P2).unwrap(),
        right.projection(View::P2).unwrap()
    );
}

#[test]
fn author_look_names_keep_unsupported_combinations_closed() {
    for (index, mutation) in [
        (0, json!({"distinct_by":"orig_cost"})),
        (0, json!({"cost_sum_le":6})),
        (
            0,
            json!({"from":{"who":"self","zone":"hand","kind":"follower"}}),
        ),
        (0, json!({"from":{"in":"unknown"}})),
        (1, json!({"distinct_by":"name"})),
        (1, json!({"groups":[{"name":""},{"trait":"機械"}]})),
        (
            1,
            json!({"groups":[{"name":{"same_as":"this"}},{"trait":"機械"}]}),
        ),
        (
            1,
            json!({"groups":[{"name":"ソニック・フォー","not_same_name_as_this":true},{"trait":"機械"}]}),
        ),
        (
            1,
            json!({"groups":[{"class":"ナイトメア"},{"trait":"機械"}]}),
        ),
        (1, json!({"reveal":true})),
    ] {
        let mut document = tree().clone();
        document["cards"]["unit-follower"]["abilities"][index]["activate"]["do"][1]["choose"]
            .as_object_mut()
            .unwrap()
            .extend(mutation.as_object().unwrap().clone());
        lowerer().lower(&document).unwrap_err();
    }
    let mut document = tree().clone();
    let choose =
        &mut document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["choose"];
    choose.as_object_mut().unwrap().remove("distinct_by");
    choose["cost_sum_le"] = json!(6);
    lowered_program(&document);
}

fn decision(raw: &mut CardProgram, index: usize) -> &mut Decision {
    raw.abilities[index]
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .iter_mut()
        .find_map(|step| {
            if let Node::Decision { decision } = &mut step.node {
                Some(decision)
            } else {
                None
            }
        })
        .unwrap()
}

#[test]
fn raw_look_names_reject_public_foreign_unbound_and_multigroup_authority() {
    let good = lowered_program(tree());
    for case in 0..7 {
        let mut raw = good.card().clone();
        let index = usize::from(case == 6);
        let choice = decision(&mut raw, index);
        if case == 0 {
            choice.visibility = Visibility::Public {};
        }
        if case == 1 {
            choice.actor = PlayerRef::Active {};
        }
        let ChoiceDomain::Objects { constraint, .. } = &mut choice.domain else {
            panic!("objects")
        };
        match case {
            2 => constraint.distinct_by = Some(ObjectProperty::Traits),
            3 => constraint.aggregate_bounds.push(AggregateBound {
                property: NumericProperty::OriginalCost,
                comparison: Comparison::LessEqual,
                value: Expr::Literal(Int(6)),
            }),
            4 | 5 => {
                let Selector::Filter { input, .. } = &mut constraint.groups[0].selector else {
                    panic!("Look filter")
                };
                if case == 4 {
                    **input = Selector::Bound {
                        reference: Binding::new(999, ScopeId(999)),
                    };
                } else {
                    **input = Selector::Zone {
                        players: PlayerSet {
                            relation: PlayerRelation::SelfPlayer,
                            relative_to: PlayerRef::Controller {},
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                            text_quantifier: TextQuantifier::Implicit,
                        },
                        zone: ZoneKind::Deck,
                        view: SelectionView::Current {},
                        binding: Binding::new(999, ScopeId(999)),
                        predicate: Box::new(Expr::Literal(Bool(true))),
                    };
                }
            }
            6 => constraint.distinct_by = Some(ObjectProperty::Names),
            _ => (),
        }
        if let Ok(forged) = WellFormedCardProgram::check(raw) {
            let initial = setup(1, json!([]));
            let mut board = Game::new(
                catalog_for_names(),
                &initial,
                &Value::Null,
                &Value::Null,
                "forged-look",
            )
            .unwrap();
            assert!(
                board.load_internal_card("unit-follower", &forged).is_err(),
                "forged private authority case {case}"
            );
        }
    }
}
