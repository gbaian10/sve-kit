#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed equipment scenarios."
)]
use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending};
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn resolve_one(board: &mut Game, printing: &str) -> Activation {
    resolve_count(board, printing, 1)
}

fn resolve_count(board: &mut Game, printing: &str, count: usize) -> Activation {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending ability");
    };
    assert_eq!(
        options.len(),
        count,
        "the expected number of printed occurrences is pending"
    );
    let option = options
        .iter()
        .find(|option| option.card == printing)
        .unwrap();
    let play = board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap();
    let activated = resolved(finish(board, play));
    finish_checks(board);
    activated
}

#[test]
fn all_ilya_printings_pay_the_fanfare_and_evolve_without_losing_nachtfang() {
    for printing in ["CP04-073", "CP04-SL17", "PCS01-029"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!("ナイトメア");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"ilya","card":printing}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP04-074"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"mill","card":printing}]);
        let mut board = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "ilya-equipment",
        )
        .unwrap();
        assert!(matches!(
            board
                .play_internal_card("ilya", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        resolve_one(&mut board, printing);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 6_u8);
        assert_eq!(after["P1"]["equipment"].as_array().unwrap().len(), 1);
        let equipped = after["P1"]["equipment"][0].as_str().unwrap();
        assert_eq!(after["objects"][equipped]["equipped_to"], "ilya");
        board
            .internal_rule_action(
                &json!({"do":"evolve","source":"ilya","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
        finish_checks(&mut board);
        let CardPlay::Awaiting(question) = pending(&mut board, "CP04-074") else {
            panic!("the printed evolution searches one Princess card");
        };
        let InputDomain::Search(domain) = &question.domain else {
            panic!("the evolution asks for a deck search");
        };
        assert_eq!(domain.cards.len(), 1);
        resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
                )
                .unwrap(),
        );
        finish_checks(&mut board);
        let evolved = board.projection(View::Referee).unwrap();
        assert_eq!(evolved["P1"]["pp"]["current"], 5_u8);
        assert_eq!(evolved["objects"]["ilya"]["power"], 3_u8);
        assert_eq!(evolved["P1"]["equipment"], after["P1"]["equipment"]);
        assert_eq!(evolved["objects"][equipped]["equipped_to"], "ilya");
        assert_eq!(evolved["P1"]["cemetery"], json!(["mill"]));
    }
}

fn actual(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "nachtfang-runtime",
    )
    .unwrap()
}

fn q_position(host: &str, axe: &str, flight: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!("ナイトメア");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ilya","card":host}]);
    initial["players"]["P1"]["zones"]["equipment"] =
        json!([{"id":"axe","card":axe,"state":{"equipped_to":"ilya"}}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"flight","card":flight}]);
    initial["players"]["P2"]["zones"]["field"] = json!(
        (0_u8..5)
            .map(|n| json!({"id":format!("enemy{n}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn play_flight(board: &mut Game) {
    let play = board
        .play_internal_card("flight", PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(board, play));
    finish_checks(board);
}

fn attack_leader(board: &mut Game) {
    board
        .internal_rule_action(&json!({"do":"attack","attacker":"ilya","target":"P2.leader"}))
        .unwrap();
    finish_checks(board);
}

fn finish_combat(board: &mut Game) {
    for _ in 0_u8..4 {
        let Some(question) = board.internal_quick_choice().unwrap() else {
            return;
        };
        board
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish_checks(board);
    }
    panic!("combat must close its Quick window");
}

fn damage_facts(activation: &Activation) -> Vec<&DamageEvent> {
    activation
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            let Fact::Damage(event) = fact else {
                return None;
            };
            Some(event)
        })
        .collect()
}

#[test]
fn q2549_all_eight_printings_damage_five_followers_but_trigger_the_axe_only_once() {
    for (host, axe, flight) in [
        ("CP04-073", "CP04-T11", "BP15-PR14"),
        ("CP04-SL17", "CP04-PR25", "BP15-PR24"),
        ("PCS01-029", "CP04-PR37", "BP15-PR14"),
    ] {
        let mut board = actual(&q_position(host, axe, flight));
        play_flight(&mut board);
        let cast = board.projection(View::Referee).unwrap();
        assert_eq!(cast["P1"]["pp"]["current"], 9_u8);
        assert_eq!(cast["P1"]["leader"]["life"], 19_u8);
        assert!(
            cast["objects"].get("flight").is_none(),
            "the spell token is gone before its granted ability is used"
        );
        attack_leader(&mut board);
        let granted = resolve_one(&mut board, flight);
        let hits = damage_facts(&granted);
        assert_eq!(hits.len(), 5);
        assert!(
            hits.iter()
                .all(|hit| hit.amount.0.get() == 1 && hit.source.object == granted.source.holder)
        );
        let burst = resolve_one(&mut board, axe);
        let burst_hits = damage_facts(&burst);
        assert_eq!(burst_hits.len(), 1);
        assert_eq!(burst_hits[0].amount.0.get(), 3);
        assert!(matches!(
            burst_hits[0].recipient,
            RuntimeDamageRecipient::Leader {
                player: PlayerId { seat: 2 }
            }
        ));
        assert_eq!(
            board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            17_u8
        );
        assert_eq!(burst_hits[0].source.object, burst.source.holder);
        finish_combat(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            15_u8
        );
        assert!(board.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn all_ilya_printings_pay_five_cemetery_cards_for_their_printed_union_burst() {
    for host in ["CP04-073", "CP04-SL17", "PCS01-029"] {
        let mut initial = q_position(host, "CP04-T11", "BP15-PR14");
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["ex"] = json!([]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..5)
                .map(|n| json!({"id":format!("fee{n}"),"card":"CP04-001"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
        let definition = selection_with_rules().current[host]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| {
                ability.node.kind == AbilityKind::Activated
                    && ability.node.qualification == AbilityQualification::UnionBurst
            })
            .unwrap()
            .node
            .id;
        let mut board = actual(&initial);
        let play = board
            .activate_internal_card("ilya", definition, PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut board, play));
        finish_checks(&mut board);
        let used = board.projection(View::Referee).unwrap();
        assert_eq!(used["objects"]["ilya"]["acted"], true);
        assert_eq!(used["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(used["P1"]["leader"]["life"], 13_u8);
        assert_eq!(used["P1"]["pp"]["current"], 10_u8);
        assert_eq!(used["P1"]["cemetery"], json!([]));
        assert_eq!(used["P1"]["banish"].as_array().unwrap().len(), 5);
        resolve_one(&mut board, "CP04-T11");
        assert_eq!(
            board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            17_u8
        );
    }
}

#[test]
fn losing_the_hosts_or_axes_abilities_stops_the_axe_while_a_later_flight_grant_still_works() {
    for silenced in ["ilya", "axe"] {
        let mut initial = q_position("CP04-073", "CP04-T11", "BP15-PR14");
        let zone = if silenced == "ilya" {
            "field"
        } else {
            "equipment"
        };
        initial["players"]["P1"]["zones"][zone][0]["state"]["silenced"] = json!(true);
        let mut board = actual(&initial);
        play_flight(&mut board);
        attack_leader(&mut board);
        let granted = resolve_one(&mut board, "BP15-PR14");
        assert_eq!(damage_facts(&granted).len(), 5);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20_u8
        );
        assert!(board.internal_pending_choice().unwrap().is_none());
        finish_combat(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            18_u8
        );
    }
}

fn step_position(axes: &[&str], host: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["equipment"] = json!(
        axes.iter()
            .enumerate()
            .map(
                |(i, card)| json!({"id":format!("axe{i}"),"card":card,"state":{"equipped_to":host}})
            )
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!(
        (0_u8..5)
            .map(|i| json!({"id":format!("enemy{i}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn step_board(mut initial: Value, steps: Vec<Action>) -> Game {
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let mut board = Game::from_selected_effects(
        producer_selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "nachtfang-producer",
    )
    .unwrap();
    board
        .load_internal_card("producer", &producer_steps(steps))
        .unwrap();
    board
}

const fn side(own: bool) -> PlayerSet {
    PlayerSet {
        relation: if own {
            PlayerRelation::SelfPlayer
        } else {
            PlayerRelation::Opponents
        },
        relative_to: PlayerRef::Controller {},
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        },
        text_quantifier: TextQuantifier::Implicit,
    }
}

const fn hit(recipients: RecipientSet, amount: i64) -> Action {
    Action::Damage {
        recipients,
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(amount)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}

fn field_hit(own: bool, amount: i64) -> Action {
    let mut targets = subjects();
    let Selector::Zone { players, .. } = &mut targets else {
        panic!("field query");
    };
    *players = side(own);
    hit(RecipientSet::Objects { objects: targets }, amount)
}

#[test]
fn two_damage_instructions_trigger_twice_but_each_five_target_instruction_triggers_once() {
    let mut second_hit = field_hit(false, 1);
    let Action::Damage {
        recipients:
            RecipientSet::Objects {
                objects: Selector::Zone { binding, .. },
            },
        ..
    } = &mut second_hit
    else {
        panic!("field damage fixture");
    };
    *binding = Binding::new(51, ScopeId(1));
    let mut board = step_board(
        step_position(&["CP04-T11"], "producer"),
        vec![field_hit(false, 1), second_hit],
    );
    let damage = activate(&mut board, "P1");
    assert_eq!(damage_facts(&damage).len(), 10);
    finish_checks(&mut board);
    let first = resolve_count(&mut board, "CP04-T11", 2);
    let second = resolve_count(&mut board, "CP04-T11", 1);
    assert_eq!(first.source.instance, second.source.instance);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        14_u8
    );
    assert!(board.internal_pending_choice().unwrap().is_none());
}

#[test]
fn two_copies_of_the_same_axe_each_observe_one_actual_damage_batch() {
    let mut board = step_board(
        step_position(&["CP04-T11", "CP04-T11"], "producer"),
        vec![field_hit(false, 1)],
    );
    let damage = activate(&mut board, "P1");
    assert_eq!(damage_facts(&damage).len(), 5);
    finish_checks(&mut board);
    let first = resolve_count(&mut board, "CP04-T11", 2);
    let second = resolve_count(&mut board, "CP04-T11", 1);
    assert_ne!(first.source.instance, second.source.instance);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        14_u8
    );
}

#[test]
fn zero_damage_own_followers_other_sources_and_leader_hits_do_not_trigger_the_axe() {
    for (host, action, expected_hits, expected_life) in [
        ("producer", field_hit(false, 0), 0_usize, 20_u8),
        ("producer", field_hit(true, 1), 2, 20),
        ("ally", field_hit(false, 1), 5, 20),
        (
            "producer",
            hit(
                RecipientSet::Leaders {
                    players: side(false),
                },
                1,
            ),
            1,
            19,
        ),
    ] {
        let mut board = step_board(step_position(&["CP04-T11"], host), vec![action]);
        let damage = activate(&mut board, "P1");
        assert_eq!(damage_facts(&damage).len(), expected_hits);
        finish_checks(&mut board);
        assert!(board.internal_pending_choice().unwrap().is_none());
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P2"]["leader"]["life"], expected_life);
        assert_eq!(after["P1"]["equipment"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn a_later_instruction_failure_restores_damage_and_the_queued_axe_trigger() {
    let overflow = Action::Life {
        player: PlayerRef::Controller {},
        change: NumericChange::Add {
            amount: Expr::Compute(Box::new(IntOperation::Arithmetic {
                operation: Arithmetic::Add,
                arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
            })),
        },
    };
    let mut board = step_board(
        step_position(&["CP04-T11"], "producer"),
        vec![field_hit(false, 1), overflow],
    );
    let before = board.projection(View::Referee).unwrap();
    let error = board
        .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
        .unwrap_err();
    assert!(error.to_string().contains("overflow"));
    assert_eq!(board.projection(View::Referee).unwrap(), before);
    assert!(board.internal_pending_choice().unwrap().is_none());
}

#[test]
fn lethal_damage_still_queues_one_axe_trigger_before_the_victims_leave() {
    let mut board = step_board(
        step_position(&["CP04-T11"], "producer"),
        vec![field_hit(false, 7)],
    );
    let damage = activate(&mut board, "P1");
    assert_eq!(damage_facts(&damage).len(), 5);
    finish_checks(&mut board);
    let dead = board.projection(View::Referee).unwrap();
    assert_eq!(dead["P2"]["field"], json!([]));
    assert_eq!(dead["P2"]["cemetery"].as_array().unwrap().len(), 5);
    resolve_one(&mut board, "CP04-T11");
    assert_eq!(
        board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        17_u8
    );
}

#[test]
fn combat_damage_to_an_opposing_follower_does_not_satisfy_ability_damage() {
    let mut initial = q_position("CP04-073", "CP04-T11", "BP15-PR14");
    initial["players"]["P1"]["zones"]["ex"] = json!([]);
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"hp":10_u8,"max_hp":10_u8});
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut board = actual(&initial);
    board
        .internal_rule_action(&json!({"do":"attack","attacker":"ilya","target":"enemy"}))
        .unwrap();
    finish_checks(&mut board);
    finish_combat(&mut board);
    let fought = board.projection(View::Referee).unwrap();
    assert_eq!(fought["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(fought["objects"]["ilya"]["damage"], 5_u8);
    assert_eq!(fought["P2"]["leader"]["life"], 20_u8);
    assert!(board.internal_pending_choice().unwrap().is_none());
}

#[test]
fn q2450_an_ilya_union_burst_without_a_target_neither_pays_nor_heals() {
    let mut initial = q_position("CP04-073", "CP04-T11", "BP15-PR14");
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["ex"] = json!([]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0_u8..5)
            .map(|n| json!({"id":format!("fee{n}"),"card":"CP04-001"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let definition = selection_with_rules().current["CP04-073"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            ability.node.kind == AbilityKind::Activated
                && ability.node.qualification == AbilityQualification::UnionBurst
        })
        .unwrap()
        .node
        .id;
    let mut board = actual(&initial);
    let before = board.projection(View::Referee).unwrap();
    let error = board
        .activate_internal_card("ilya", definition, PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::IllegalPlay(_)),
        "the printed target is mandatory: {error}"
    );
    assert_eq!(board.projection(View::Referee).unwrap(), before);
}

#[test]
fn preventing_every_positive_hit_queues_no_axe_trigger() {
    for acted in [false, true] {
        let mut initial = step_position(&["CP04-T11"], "producer");
        initial["players"]["P2"]["zones"]["field"] = json!(
            (0_u8..5)
                .map(|i| json!({"id":format!("protected{i}"),"card":"BP22-061","state":{"acted":acted}}))
                .collect::<Vec<_>>()
        );
        let mut board = step_board(initial, vec![field_hit(false, 1)]);
        let damage = activate(&mut board, "P1");
        assert_eq!(damage_facts(&damage).len(), if acted { 0 } else { 5 });
        finish_checks(&mut board);
        if acted {
            assert!(board.internal_pending_choice().unwrap().is_none());
        } else {
            resolve_one(&mut board, "CP04-T11");
        }
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(
            after["P2"]["leader"]["life"],
            if acted { 20_u8 } else { 17 }
        );
        for i in 0_u8..5 {
            assert_eq!(
                after["objects"][format!("protected{i}")]["damage"],
                u8::from(!acted)
            );
        }
        assert_eq!(after["P1"]["equipment"].as_array().unwrap().len(), 1);
    }
}
