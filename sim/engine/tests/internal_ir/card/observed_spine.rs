#![expect(
    clippy::panic,
    reason = "Unexpected public decisions invalidate these observation regressions."
)]
use super::*;
use std::{path::PathBuf, process::Command, sync::OnceLock};
use sve_engine::{
    EngineFailure,
    game::internal::{DecisionView, InputDomain},
};

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
import json, sys
from pathlib import Path
from sve_carddb.domains.effects.author_expansion import AuthorExpander
result = AuthorExpander(Path(sys.argv[1])).expand(Path(sys.argv[2]).read_bytes(), [])
print(json.dumps(result.tree))
",
            ])
            .arg(root.join("dsl"))
            .arg(root.join("sim/engine/tests/fixtures/lowering/observed-spine.yaml"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "C0 observation fixture expansion failed"
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

fn document(index: usize) -> Value {
    let mut value = tree().clone();
    let ability = value["cards"]["unit-follower"]["abilities"][index].clone();
    value["cards"]["unit-follower"]["abilities"] = json!([ability]);
    value
}

fn final_move(value: &mut Value) -> &mut Value {
    value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap()
        .get_mut("move")
        .unwrap()
}

fn program(value: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(value)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn catalog_for_spine() -> Arc<Catalog> {
    let (snapshot, registry, legacy) = catalog_documents(&json!("1"));
    let template: Value = serde_json::from_str(snapshot.lines().nth(1).unwrap()).unwrap();
    let mut faces = snapshot.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut effects: Value = serde_json::from_str(&legacy).unwrap();
    for (number, name, kind, cost, traits) in [
        ("spell-a", "alpha", "スペル", "1", vec![]),
        (
            "oracle",
            "oracle",
            "フォロワー",
            "2",
            vec!["オラクルシンクタンク"],
        ),
        ("spell-b", "beta", "スペル", "2", vec![]),
        ("same-source", "unit-follower", "スペル", "2", vec![]),
        (
            "follower-a",
            "宴楽の選中牌",
            "フォロワー",
            "2",
            vec!["宴楽"],
        ),
        ("follower-b", "other", "フォロワー", "3", vec![]),
    ] {
        let mut face = template.clone();
        face["number"] = json!(number);
        face["faces"][0]["name"] = json!(name);
        face["faces"][0]["card_type"] = json!(kind);
        face["faces"][0]["cost"] = json!(cost);
        face["faces"][0]["traits"] = json!(traits);
        faces.push(face.to_string());
        effects["cards"][number] = json!({"status":"complete","abilities":[]});
    }
    Arc::new(
        Catalog::from_documents(
            &faces.join("\n"),
            &registry,
            &[("spine".into(), effects.to_string())],
        )
        .unwrap(),
    )
}

fn board(program: &WellFormedCardProgram, deck: &[&str]) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        deck.iter()
            .enumerate()
            .map(|(i, card)| json!({"id":format!("d{i}"),"card":card}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(
        catalog_for_spine(),
        &initial,
        &Value::Null,
        &Value::Null,
        "observed-spine",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    for number in [
        "oracle",
        "spell-a",
        "spell-b",
        "same-source",
        "follower-a",
        "follower-b",
    ] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("expected private decision")
    };
    q
}

fn start(game: &mut Game, program: &WellFormedCardProgram) -> DecisionView {
    question(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    )
}

fn target_cards(q: &DecisionView) -> Vec<String> {
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("target decision")
    };
    domain
        .members
        .iter()
        .map(|m| m.card.clone().unwrap())
        .collect()
}

fn pick(game: &mut Game, q: &DecisionView, handles: Vec<u64>) -> CardPlay {
    game.answer_internal_card(q.actor, q.token, Answer::Targets(handles))
        .unwrap()
}

fn ordered(game: &mut Game, q: &DecisionView) -> CardPlay {
    let InputDomain::Order(domain) = &q.domain else {
        panic!("order decision")
    };
    game.answer_internal_card(
        q.actor,
        q.token,
        Answer::Order(domain.iter().map(|m| m.handle).collect()),
    )
    .unwrap()
}

fn finish_orders(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        play = ordered(game, &q);
    }
    resolved(play);
}

#[test]
fn second_look_choice_drops_moved_members_before_name_cost_and_source_predicates() {
    for query in [
        json!({"in":"seen","name":"beta"}),
        json!({"in":"seen","orig_cost_le":2}),
        json!({"in":"seen","not_same_name_as_this":true}),
    ] {
        let mut value = document(0);
        let steps = value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
            .as_array_mut()
            .unwrap();
        steps.pop();
        steps[4]["choose"]["from"] = query.clone();
        let program = program(&value);
        let mut game = board(&program, &["spell-a", "spell-b", "same-source"]);
        let first = start(&mut game, &program);
        let second = question(pick(&mut game, &first, vec![0]));
        let cards = target_cards(&second);
        assert!(
            !cards.contains(&"spell-a".to_owned()),
            "a moved Look member cannot be reused"
        );
        assert!(cards.contains(&"spell-b".to_owned()));
        if query.get("not_same_name_as_this").is_some() {
            assert!(!cards.contains(&"same-source".to_owned()));
        }
        resolved(pick(&mut game, &second, vec![0]));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["d0"])
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"],
            json!(["d1"])
        );
    }
}

#[test]
fn bp14_selected_deck_reveal_hides_other_members_and_forgets_the_top_before_remainder() {
    let program = program(&document(3));
    let mut left = board(
        &program,
        &["follower-a", "spell-a", "follower-b", "spell-b"],
    );
    let mut right = board(
        &program,
        &["follower-a", "follower-b", "spell-b", "follower-b"],
    );
    let lq = start(&mut left, &program);
    let rq = start(&mut right, &program);
    assert_eq!(target_cards(&lq), vec!["follower-a"]);
    assert_eq!(target_cards(&rq), vec!["follower-a"]);
    assert_eq!(
        left.projection(View::P2).unwrap(),
        right.projection(View::P2).unwrap()
    );
    let lt = question(pick(&mut left, &lq, vec![0]));
    let rt = question(pick(&mut right, &rq, vec![0]));
    let shown = left.projection(View::P2).unwrap();
    assert_eq!(shown, right.projection(View::P2).unwrap());
    let known = shown["known_cards"].as_array().unwrap();
    assert!(known.iter().any(|m| m["id"] == "d0"));
    for id in ["d1", "d2", "d3"] {
        assert!(!known.iter().any(|m| m["id"] == id));
    }
    let lb = ordered(&mut left, &lt);
    let rb = ordered(&mut right, &rt);
    assert!(matches!(&lb, CardPlay::Awaiting(_)));
    let between = left.projection(View::P2).unwrap();
    assert_eq!(between, right.projection(View::P2).unwrap());
    assert!(
        between["P1"]["deck"]
            .as_array()
            .unwrap()
            .iter()
            .all(|member| member == &json!({"filler":1}))
    );
    assert!(left.internal_decision(View::P2).is_none());
    assert!(
        !between["knowledge"]["located"]
            .as_array()
            .unwrap()
            .contains(&json!("d0"))
    );
    finish_orders(&mut left, lb);
    finish_orders(&mut right, rb);
    assert_eq!(
        left.projection(View::P2).unwrap(),
        right.projection(View::P2).unwrap()
    );
    assert_eq!(
        left.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["d0", "d1", "d2", "d3"])
    );
}

#[test]
fn bp14_empty_short_or_declined_choice_reveals_only_actual_selected_members() {
    let program = program(&document(3));
    for (deck, picked) in [
        (vec![], vec![]),
        (vec!["follower-b"], vec![]),
        (vec!["follower-a"], vec![0]),
        (vec!["follower-a", "spell-a"], vec![]),
    ] {
        let mut game = board(&program, &deck);
        let q = start(&mut game, &program);
        let revealed = !picked.is_empty();
        let play = pick(&mut game, &q, picked);
        finish_orders(&mut game, play);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["deck"].as_array().unwrap().len(), deck.len());
        assert!(game.internal_decision(View::P2).is_none());
        let public = game.projection(View::P2).unwrap();
        let known = public["known_cards"].as_array().unwrap();
        for index in 0..deck.len() {
            assert_eq!(
                known
                    .iter()
                    .any(|member| member["id"] == format!("d{index}")),
                revealed && index == 0
            );
        }
    }
}

#[test]
fn selected_deck_reveal_rechecks_location_knowledge_after_an_order() {
    let mut value = document(3);
    value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .swap(2, 3);
    let program = program(&value);
    let mut game = board(&program, &["follower-a", "spell-a"]);
    let q = start(&mut game, &program);
    let top = question(pick(&mut game, &q, vec![0]));
    let error = game
        .answer_internal_card(top.actor, top.token, Answer::Order(vec![0]))
        .unwrap_err();
    assert!(matches!(error, EngineFailure::AuthorizationIntegrity(_)));
}

#[test]
fn raw_order_keeps_bound_members_in_union_and_difference() {
    for difference in [false, true] {
        let mut value = document(3);
        value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
            .as_array_mut()
            .unwrap()
            .remove(2);
        let mut raw = program(&value).card().clone();
        let steps = &mut raw.abilities[0].node.body.as_mut().unwrap().steps;
        let Node::Instruction { instruction: look } = &steps[0].node else {
            panic!("Look")
        };
        let looked = Selector::ReceiptMembers {
            receipt: ObjectReceipt::Look {
                receipt: ReceiptReference::new(look.receipt),
            },
            projection: MemberProjection::Before,
        };
        let Node::Instruction { instruction } = &mut steps[2].node else {
            panic!("Order")
        };
        let Action::Order {
            cards,
            destinations,
            ..
        } = &mut instruction.action
        else {
            panic!("Order action")
        };
        *cards = if difference {
            Selector::Difference {
                input: Box::new(looked),
                except: Box::new(cards.clone()),
            }
        } else {
            Selector::Union {
                sets: vec![cards.clone(), looked],
            }
        };
        destinations[0].cards = cards.clone();
        let checked = WellFormedCardProgram::check(raw).unwrap();
        let mut game = board(&checked, &["follower-a", "spell-a", "follower-b"]);
        let q = start(&mut game, &checked);
        let order = question(pick(&mut game, &q, vec![0]));
        let InputDomain::Order(domain) = &order.domain else {
            panic!("Order domain")
        };
        let names = domain
            .iter()
            .map(|member| member.card.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names.len(), if difference { 2 } else { 3 });
        assert_eq!(
            names.iter().filter(|name| **name == "follower-a").count(),
            usize::from(!difference)
        );
        let play = ordered(&mut game, &order);
        finish_orders(&mut game, play);
    }
}

#[test]
fn stale_private_hand_targets_still_skip_reveal_before_deck_hand_classification() {
    let mut value = document(3);
    value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"choose":{"bind":"picked","from":{"who":"self","zone":"hand"},"n":1}},
        {"move":{"ref":"picked","to":"ex","result":"moved"}},
        {"reveal":{"ref":"picked","bind":"shown"}}
    ]);
    let program = program(&value);
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"h","card":"spell-a"}]);
    let mut game = Game::new(
        catalog_for_spine(),
        &initial,
        &Value::Null,
        &Value::Null,
        "hand-expiry",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("spell-a", &card(vec![])).unwrap();
    let q = start(&mut game, &program);
    resolved(pick(&mut game, &q, vec![0]));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["h"])
    );
}

#[test]
fn a_public_target_moved_into_the_deck_does_not_gain_look_reveal_authority() {
    let mut value = document(3);
    value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"choose":{"bind":"picked","from":{"who":"self","zone":"field","kind":"follower"},"n":1}},
        {"move":{"ref":"picked","to":"deck","position":"bottom"}},
        {"reveal":{"ref":"picked","bind":"shown"}}
    ]);
    let program = program(&value);
    let mut game = board(&program, &["spell-a"]);
    let q = start(&mut game, &program);
    let error = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![0]))
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::Invalid(message) if message == "foreign hand reveal member")
    );
}

#[test]
fn bp01_and_bp13_two_optional_spells_subtract_the_union_of_successful_moves() {
    let program = program(&document(0));
    for count in 0..=3 {
        for first in [false, true] {
            for second in [false, true] {
                let cards = ["spell-a", "spell-b", "follower-b"];
                let mut game = board(&program, &cards[..count]);
                let one = start(&mut game, &program);
                let take_one = first && count > 0;
                let two = question(pick(
                    &mut game,
                    &one,
                    if take_one { vec![0] } else { vec![] },
                ));
                let available = count.min(2) - usize::from(take_one);
                assert_eq!(target_cards(&two).len(), available);
                let take_two = second && available > 0;
                let play = pick(&mut game, &two, if take_two { vec![0] } else { vec![] });
                finish_orders(&mut game, play);
                let mut expected_hand = Vec::new();
                if take_one {
                    expected_hand.push("d0".to_owned());
                }
                let expected_cemetery = if take_two {
                    vec![format!("d{}", usize::from(take_one))]
                } else {
                    Vec::new()
                };
                let expected_deck = (0..count)
                    .map(|i| format!("d{i}"))
                    .filter(|id| !expected_hand.contains(id) && !expected_cemetery.contains(id))
                    .collect::<Vec<_>>();
                let view = game.projection(View::Referee).unwrap();
                assert_eq!(view["P1"]["hand"], json!(expected_hand));
                assert_eq!(view["P1"]["cemetery"], json!(expected_cemetery));
                assert_eq!(view["P1"]["deck"], json!(expected_deck));
            }
        }
    }
}

#[test]
fn bp04_short_look_resolves_hand_cemetery_top_bottom_without_fixed_four_picks() {
    let program = program(&document(1));
    for count in 0..=4 {
        let cards = ["spell-a", "spell-b", "follower-a", "follower-b"];
        let mut game = board(&program, &cards[..count]);
        let mut q = start(&mut game, &program);
        for consumed in 0..3 {
            let play = pick(
                &mut game,
                &q,
                if count > consumed { vec![0] } else { vec![] },
            );
            if consumed < 2 {
                q = question(play);
            } else {
                finish_orders(&mut game, play);
            }
        }
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["hand"],
            if count > 0 { json!(["d0"]) } else { json!([]) }
        );
        assert_eq!(
            view["P1"]["cemetery"],
            if count > 1 { json!(["d1"]) } else { json!([]) }
        );
        assert_eq!(
            view["P1"]["deck"],
            json!((2..count).map(|i| format!("d{i}")).collect::<Vec<_>>())
        );
        let public = game.projection(View::P2).unwrap();
        assert!(
            public["P1"]["hand"]
                .as_array()
                .unwrap()
                .iter()
                .all(|member| member == &json!({"filler":1}))
        );
        assert!(
            public["P1"]["deck"]
                .as_array()
                .unwrap()
                .iter()
                .all(|member| member == &json!({"filler":1}))
        );
    }
}

#[test]
fn cp03_remainder_subtracts_both_hand_moves_and_optional_order_before_members() {
    let program = program(&document(2));
    for count in 0..=3 {
        for take_hand in [false, true] {
            let take_hand = take_hand && count > 0;
            let remaining_count = count - usize::from(take_hand);
            for top_count in 0..=remaining_count {
                let cards = ["oracle", "spell-a", "follower-b"];
                let mut game = board(&program, &cards[..count]);
                let first = start(&mut game, &program);
                assert_eq!(
                    target_cards(&first),
                    if count > 0 {
                        vec!["oracle"]
                    } else {
                        Vec::new()
                    }
                );
                let second = question(pick(
                    &mut game,
                    &first,
                    if take_hand { vec![0] } else { vec![] },
                ));
                let InputDomain::Targets(domain) = &second.domain else {
                    panic!("top subset")
                };
                assert_eq!(domain.members.len(), remaining_count);
                let chosen = domain
                    .members
                    .iter()
                    .take(top_count)
                    .map(|m| m.handle)
                    .collect();
                let mut play = pick(&mut game, &second, chosen);
                if top_count > 0 {
                    let top = question(play);
                    let InputDomain::Order(domain) = &top.domain else {
                        panic!("top Order")
                    };
                    assert_eq!(domain.len(), top_count);
                    play = game
                        .answer_internal_card(
                            top.actor,
                            top.token,
                            Answer::Order(domain.iter().rev().map(|m| m.handle).collect()),
                        )
                        .unwrap();
                }
                if remaining_count > top_count {
                    let bottom = question(play);
                    let InputDomain::Order(domain) = &bottom.domain else {
                        panic!("bottom Order")
                    };
                    assert_eq!(domain.len(), remaining_count - top_count);
                    play = game
                        .answer_internal_card(
                            bottom.actor,
                            bottom.token,
                            Answer::Order(domain.iter().rev().map(|m| m.handle).collect()),
                        )
                        .unwrap();
                }
                resolved(play);
                let remaining = (usize::from(take_hand)..count)
                    .map(|i| format!("d{i}"))
                    .collect::<Vec<_>>();
                let expected = remaining[..top_count]
                    .iter()
                    .rev()
                    .chain(remaining[top_count..].iter().rev())
                    .collect::<Vec<_>>();
                let view = game.projection(View::Referee).unwrap();
                assert_eq!(
                    view["P1"]["hand"],
                    if take_hand { json!(["d0"]) } else { json!([]) }
                );
                assert_eq!(view["P1"]["deck"], json!(expected));
                let public = game.projection(View::P2).unwrap();
                for index in 0..count {
                    assert_eq!(
                        public["known_cards"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|m| m["id"] == format!("d{index}")),
                        take_hand && index == 0
                    );
                }
            }
        }
    }
}

#[test]
fn except_union_accepts_atoms_with_nested_differences_and_preserves_exclusion_position() {
    let mut value = document(2);
    final_move(&mut value)["ref"]["except"] = json!({"any_of":[
        {"in":{"objects_of":"taken1","view":"before"}},
        {"in":{"objects_of":"placed","view":"before"},"except":{"in":{"objects_of":"taken1","view":"before"}}}
    ]});
    let program = program(&value);
    let mut game = board(&program, &["oracle", "follower-a", "follower-b"]);
    let first = start(&mut game, &program);
    let second = question(pick(&mut game, &first, vec![0]));
    let top = question(pick(&mut game, &second, vec![0]));
    let bottom = question(ordered(&mut game, &top));
    let InputDomain::Order(domain) = &bottom.domain else {
        panic!("remaining card")
    };
    assert_eq!(domain.len(), 1);
    assert_eq!(domain[0].card, "follower-b");
    resolved(ordered(&mut game, &bottom));
}

#[test]
fn except_union_keeps_empty_nested_union_unknown_and_shuffle_shapes_closed() {
    for except in [
        json!({"any_of":[]}),
        json!({"any_of":[{"any_of":[{"in":"seen"}]}]}),
        json!({"any_of":[{"who":"self","zone":"deck"}]}),
        json!({"any_of":[{"in":"missing"}]}),
        json!({"any_of":[{"in":{"same_as":"seen"}}]}),
    ] {
        let mut value = document(0);
        final_move(&mut value)["ref"]["except"] = except;
        lowerer().lower(&value).unwrap_err();
    }
    let mut value = document(0);
    final_move(&mut value)["order"] = json!("shuffle");
    let union = final_move(&mut value)["ref"]["except"].clone();
    final_move(&mut value)["ref"]["except"] = union["any_of"][0].clone();
    lowerer().lower(&value).unwrap();
    final_move(&mut value)["ref"]["except"] = union;
    lowerer().lower(&value).unwrap_err();
}

#[test]
fn a_difference_that_adds_back_a_moved_member_still_fails_retained_freshness() {
    let mut value = document(0);
    final_move(&mut value)["ref"]["except"] = json!({
        "in":{"objects_of":"taken1","view":"before"},"except":{"in":{"objects_of":"taken1","view":"before"}}
    });
    let program = program(&value);
    let mut game = board(&program, &["spell-a", "spell-b"]);
    let one = start(&mut game, &program);
    let two = question(pick(&mut game, &one, vec![0]));
    let error = game
        .answer_internal_card(two.actor, two.token, Answer::Targets(vec![]))
        .unwrap_err();
    assert!(
        matches!(&error,EngineFailure::Unsupported(message) if message=="internal D1 does not support order requires current generations in the chooser's deck"),
        "{error:?}"
    );
}

#[test]
fn a_remainder_never_brings_unobserved_deck_members_into_its_order() {
    let program = program(&document(0));
    let mut game = board(
        &program,
        &["spell-a", "spell-b", "follower-b", "follower-a", "spell-a"],
    );
    let first = start(&mut game, &program);
    let second = question(pick(&mut game, &first, vec![0]));
    let bottom = question(pick(&mut game, &second, vec![0]));
    let InputDomain::Order(domain) = &bottom.domain else {
        panic!("observed remainder")
    };
    assert_eq!(domain.len(), 1);
    assert_eq!(domain[0].card, "follower-b");
    resolved(ordered(&mut game, &bottom));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["d3", "d4", "d2"])
    );
}

#[test]
fn observed_move_accepts_exclusion_union_including_a_single_atom() {
    for (destination, single, repeat, expected_ex, expected_cemetery) in [
        ("ex", false, false, vec!["d2"], vec!["d1"]),
        ("ex", true, false, vec!["d1", "d2"], vec![]),
        ("ex", false, true, vec!["d1", "d2"], vec![]),
        ("cemetery", false, false, vec![], vec!["d1", "d2"]),
        ("cemetery", true, false, vec![], vec!["d1", "d2"]),
        ("cemetery", false, true, vec![], vec!["d1", "d2"]),
    ] {
        let mut value = document(0);
        let movement = final_move(&mut value);
        movement["to"] = json!(destination);
        movement.as_object_mut().unwrap().remove("position");
        movement.as_object_mut().unwrap().remove("order");
        if single {
            movement["ref"]["except"]["any_of"]
                .as_array_mut()
                .unwrap()
                .truncate(1);
        }
        if repeat {
            let atom = movement["ref"]["except"]["any_of"][0].clone();
            movement["ref"]["except"]["any_of"] = json!([atom.clone(), atom]);
        }
        let program = program(&value);
        let mut game = board(&program, &["spell-a", "spell-b", "follower-b"]);
        let first = start(&mut game, &program);
        let second = question(pick(&mut game, &first, vec![0]));
        resolved(pick(
            &mut game,
            &second,
            if single || repeat { vec![] } else { vec![0] },
        ));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["ex"], json!(expected_ex));
        assert_eq!(view["P1"]["cemetery"], json!(expected_cemetery));
    }
}

#[test]
fn failed_ex_moves_are_not_removed_by_an_exclusion_union() {
    let mut value = document(0);
    value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][2]["move"]["to"] =
        json!("ex");
    let program = program(&value);
    let mut initial = setup(
        1,
        json!([{"id":"d0","card":"spell-a"},{"id":"d1","card":"spell-b"},{"id":"d2","card":"follower-b"}]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..5)
            .map(|i| json!({"id":format!("e{i}"),"card":"follower-b"}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(
        catalog_for_spine(),
        &initial,
        &Value::Null,
        &Value::Null,
        "full-ex",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    for number in ["spell-a", "spell-b", "follower-b"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let first = start(&mut game, &program);
    let second = question(pick(&mut game, &first, vec![0]));
    assert_eq!(target_cards(&second), vec!["spell-a", "spell-b"]);
    let bottom = question(pick(&mut game, &second, vec![]));
    let InputDomain::Order(domain) = &bottom.domain else {
        panic!("all three remain")
    };
    assert_eq!(domain.len(), 3);
    resolved(ordered(&mut game, &bottom));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), 5);
    assert_eq!(view["P1"]["deck"], json!(["d0", "d1", "d2"]));
}

#[test]
fn mixed_hand_and_deck_reveal_keeps_the_old_invalid_error_and_rolls_back() {
    let mut value = document(3);
    value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"look":{"top":{"who":"self","n":3},"bind":"seen"}},
        {"choose":{"bind":"picked","from":{"in":"seen"},"upto":2}},
        {"reveal":{"top":{"who":"self","n":1},"bind":"published"}},
        {"move":{"ref":{"in":"published"},"to":"hand","result":"taken"}},
        {"reveal":{"ref":"picked","bind":"mixed"}}
    ]);
    let program = program(&value);
    for picked in [vec![0, 1], vec![1, 0]] {
        let mut game = board(&program, &["spell-a", "spell-b", "follower-a"]);
        let before = game.projection(View::P2).unwrap();
        let q = start(&mut game, &program);
        let error = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(picked))
            .unwrap_err();
        assert!(
            matches!(error,EngineFailure::Invalid(message) if message=="foreign hand reveal member")
        );
        assert_eq!(game.projection(View::P2).unwrap(), before);
    }
}
