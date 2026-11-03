#![expect(
    clippy::panic,
    reason = "Unexpected declarations invalidate these printed discard scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn selected() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        snapshot.push_str(&json!({"number":"timing-base","faces":[{
            "name":"discard timing fixture base","card_class":"ナイトメア",
            "card_type":"フォロワー","traits":[],"cost":"2","power":"2","hp":"2",
            "text":null,"sections":[]
        }]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([
            "timing-base.yaml", "format: 1\nkind: effect_set\ncards:\n  timing-base:\n    status: complete\n    abilities: []\n"
        ]));
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn compile_fixture(number: &str, head: &Value) -> WellFormedCardProgram {
    let mut tree = expanded::tree().clone();
    let mut row = tree["cards"]["BP01-041"].clone();
    let mut entry = row["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("activate");
    entry
        .as_object_mut()
        .unwrap()
        .extend(head.as_object().unwrap().clone());
    row["abilities"] = json!([entry]);
    tree["cards"] = json!({number:row});
    lowerer().lower(&tree).unwrap().remove(number).unwrap()
}

fn base_program() -> &'static WellFormedCardProgram {
    static BASE: OnceLock<WellFormedCardProgram> = OnceLock::new();
    BASE.get_or_init(|| {
        compile_fixture(
            "timing-base",
            &json!({"evolve":{
                "cost":{"pp":1_u8},"into":"カオスルーラー・アイシィレンドリング"
            }}),
        )
    })
}

fn producer_program(act: bool) -> WellFormedCardProgram {
    if act {
        compile_fixture(
            "producer",
            &json!({"activate":{
                "cost":{"pp":0_u8},
                "targets":[{"bind":"chosen","from":{"who":"self","zone":"field","kind":"follower","other":true},"n":1_u8}],
                "do":[{"act":"chosen"}]
            }}),
        )
    } else {
        producer(Action::Discard {
            player: PlayerRef::Controller {},
            mode: DiscardMode::All {
                eligible: Selector::Zone {
                    players: PlayerSet {
                        relation: PlayerRelation::SelfPlayer,
                        relative_to: PlayerRef::Controller {},
                        cardinality: Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        },
                        text_quantifier: TextQuantifier::Implicit,
                    },
                    zone: ZoneKind::Hand,
                    view: SelectionView::Current {},
                    binding: Binding::new(400, ScopeId(1)),
                    predicate: Box::new(Expr::Literal(Bool(true))),
                },
            },
        })
    }
}

fn game(initial: &Value, act: bool) -> Game {
    let mut initial = initial.clone();
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let mut board = Game::from_selected_effects(
        selected().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "discard-seeds",
    )
    .unwrap();
    board
        .load_internal_card("timing-base", base_program())
        .unwrap();
    board
        .load_internal_card("producer", &producer_program(act))
        .unwrap();
    board
}

fn pending(board: &mut Game, number: &str, count: usize) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending choice");
    };
    assert_eq!(
        options.len(),
        count,
        "the complete event batch has the expected occurrences"
    );
    let option = options.iter().find(|option| option.card == number).unwrap();
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap()
}

fn finish(board: &mut Game, mut play: CardPlay, preferred: Option<&str>) -> Activation {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else {
            let (InputDomain::Targets(domain) | InputDomain::CostMaterials(domain)) =
                &question.domain
            else {
                panic!("printed targets or payment materials");
            };
            let mut members = domain.members.iter().collect::<Vec<_>>();
            if let Some(number) = preferred {
                assert!(
                    members
                        .iter()
                        .any(|member| member.card.as_deref() == Some(number)),
                    "the requested printed target must be a legal candidate"
                );
                members.sort_by_key(|member| member.card.as_deref() != Some(number));
            }
            let count = domain
                .cardinality
                .maximum
                .map_or(members.len(), |n| usize::try_from(n.0).unwrap())
                .min(members.len());
            let handles = members
                .into_iter()
                .take(count)
                .map(|member| member.handle)
                .collect();
            if matches!(question.domain, InputDomain::Targets(_)) {
                Answer::Targets(handles)
            } else {
                Answer::CostMaterials(handles)
            }
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play)
}

fn accept_optional(board: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = &play {
        if !matches!(question.domain, InputDomain::OptionalCost) {
            break;
        }
        play = board
            .answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
            .unwrap();
    }
    play
}

fn resolve(board: &mut Game, number: &str, count: usize, preferred: Option<&str>) {
    let play = pending(board, number, count);
    finish(board, play, preferred);
    finish_checks(board);
}

fn icy_position(form: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"timing-base"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"fee","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"g0","card":"BP01-171"},{"id":"g1","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy0","card":"BP01-171"},{"id":"enemy1","card":"BP01-166"}
    ]);
    initial["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"enemy-hand0","card":"BP01-171"},{"id":"enemy-hand1","card":"BP01-171"}
    ]);
    initial
}

fn evolve(board: &mut Game) {
    assert_eq!(
        board
            .internal_rule_action(&json!({
                "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}
            }))
            .unwrap()
            .outcome,
        "resolved",
        "the ordinary evolution must resolve before its printed trigger"
    );
}

fn icy_targets_before_payment(board: &mut Game, form: &str) {
    let play = pending(board, form, 1);
    let CardPlay::Awaiting(targets) = accept_optional(board, play) else {
        panic!("cemetery targets precede payment");
    };
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("formal cemetery targets");
    };
    assert_eq!(
        domain.members.len(),
        2,
        "the future discarded follower is not a target candidate"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(2),
        "the cemetery declaration requires two existing followers"
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["fee"]),
        "the discard fee remains in hand until target declaration completes"
    );
    let declared = board
        .answer_internal_card(
            targets.actor,
            targets.token,
            Answer::Targets(domain.members.iter().map(|member| member.handle).collect()),
        )
        .unwrap();
    let CardPlay::Awaiting(payment) = accept_optional(board, declared) else {
        panic!("discard follows target declaration");
    };
    let InputDomain::CostMaterials(materials) = &payment.domain else {
        panic!("private head payment");
    };
    assert_eq!(
        materials.members.len(),
        1,
        "exactly the original hand card is available for the fee"
    );
    let paid = board
        .answer_internal_card(
            payment.actor,
            payment.token,
            Answer::CostMaterials(vec![materials.members[0].handle]),
        )
        .unwrap();
    finish(board, paid, None);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"],
        json!(["g0", "g1"]),
        "only the two pre-payment cemetery targets return to hand"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["fee"]),
        "the paid follower remains in the cemetery"
    );
    resolve(board, form, 1, Some("BP01-171"));
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["enemy0"]["acted"],
        true,
        "the first discarded card must trigger the printed enemy act ability"
    );
}

fn icy_acted_discard(board: &mut Game, form: &str) {
    let play = board
        .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    finish(board, play, None);
    let CardPlay::Awaiting(question) = pending(board, form, 1) else {
        panic!("the opponent chooses their own discard");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private opponent discard");
    };
    assert_eq!(
        question.actor,
        PlayerId { seat: 2 },
        "the opposing player chooses their private discard"
    );
    assert_eq!(
        domain.members.len(),
        2,
        "both opposing hand cards are legal private choices"
    );
    assert!(
        board.internal_decision(View::P1).is_none(),
        "the controller cannot inspect the opposing private choice"
    );
    let before = board.projection(View::Referee).unwrap();
    board
        .answer_internal_card(
            PlayerId { seat: 1 },
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap_err();
    assert_eq!(
        board.projection(View::Referee).unwrap(),
        before,
        "the wrong player cannot mutate the pending private discard"
    );
    let discarded = board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap();
    finish(board, discarded, None);
    resolve(board, form, 1, Some("BP01-166"));
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["hand"],
        json!(["enemy-hand1"]),
        "the unselected opposing hand card stays private in hand"
    );
    assert_eq!(
        state["P2"]["cemetery"],
        json!(["enemy-hand0"]),
        "the chosen opposing hand card is discarded"
    );
    assert_eq!(
        state["objects"]["enemy1"]["acted"], true,
        "the opposing discard triggers the second enemy act"
    );
}

#[test]
fn icy_printings_keep_all_three_abilities_and_declare_old_cemetery_targets_before_payment() {
    // Each printing executes the complete script, sharing one immutable source preparation.
    for form in ["BP11-070", "BP11-SL14", "BP11-U05", "EBD03-006"] {
        let mut board = game(&icy_position(form), true);
        evolve(&mut board);
        icy_targets_before_payment(&mut board, form);
        icy_acted_discard(&mut board, form);
        assert!(
            board.internal_pending_choice().unwrap().is_none(),
            "every expected printed trigger has resolved without an extra occurrence"
        );
    }
}

#[test]
fn icy_does_not_pay_its_head_cost_without_two_existing_cemetery_targets() {
    let mut initial = icy_position("BP11-070");
    initial["players"]["P1"]["zones"]["cemetery"]
        .as_array_mut()
        .unwrap()
        .pop();
    let mut board = game(&initial, true);
    evolve(&mut board);
    let play = pending(&mut board, "BP11-070", 1);
    let play = accept_optional(&mut board, play);
    assert!(
        !matches!(play, CardPlay::Awaiting(_)),
        "an impossible declaration must not ask to discard"
    );
    finish_checks(&mut board);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"],
        json!(["fee"]),
        "an impossible cemetery declaration must preserve its unspent fee"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["g0"]),
        "the sole old cemetery target must remain untouched"
    );
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "every expected printed trigger has resolved without an extra occurrence"
    );
}

#[test]
fn plesiosaurus_base_printings_pay_a_real_tree_and_keep_its_leave_ability() {
    for number in ["BP12-052", "BP12-SL14"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"tree","card":"BP07-T03"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"d0","card":"BP01-171"},{"id":"d1","card":"BP01-171"},
            {"id":"spare","card":"BP01-171"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut board = game(&initial, false);
        let play = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        assert!(
            matches!(play, CardPlay::Entered(_)),
            "playing the follower enters it and queues its printed fanfare"
        );
        resolve(&mut board, number, 1, None);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["d0"]),
            "the paid fanfare draws before the tree leave ability resolves"
        );
        resolve(&mut board, "BP07-T03", 1, None);
        resolve(&mut board, number, 1, None);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"],
            json!(["producer", "source"]),
            "the paid tree leaves while both followers stay on the field"
        );
        assert_eq!(
            state["P1"]["hand"],
            json!(["d1"]),
            "the tree draws a second card and discards the first draw"
        );
        assert_eq!(
            state["P1"]["cemetery"],
            json!(["d0"]),
            "the real tree discards the selected first draw"
        );
        assert_eq!(
            state["P1"]["deck"],
            json!(["spare"]),
            "the fanfare and tree each draw exactly once"
        );
        assert_eq!(
            state["P1"]["pp"]["current"], 5_u8,
            "playing the base pays its printed five PP"
        );
        assert_eq!(
            state["P2"]["leader"]["life"], 18_u8,
            "one discard batch causes exactly two leader damage"
        );
        assert!(
            board.internal_pending_choice().unwrap().is_none(),
            "every expected printed trigger has resolved without an extra occurrence"
        );
    }
}

#[test]
fn plesiosaurus_evolved_printings_pay_before_healing_and_group_each_later_discard_action() {
    for (base, form) in [
        ("BP12-052", "BP12-053"),
        ("BP12-SL14", "BP12-SL15"),
        ("BP12-052", "BP12-U04"),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(12_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"h0","card":"BP01-171"},{"id":"h1","card":"BP01-171"},{"id":"h2","card":"BP01-171"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut board = game(&initial, false);
        evolve(&mut board);
        resolve(&mut board, form, 1, None);
        let healed = board.projection(View::Referee).unwrap();
        assert_eq!(
            healed["P1"]["leader"]["life"], 14_u8,
            "OnEvolve heals two after the hand payment"
        );
        assert_eq!(
            healed["P1"]["cemetery"],
            json!(["h0"]),
            "the evolution fee is discarded before healing"
        );
        resolve(&mut board, form, 1, None);
        let play = board
            .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, play, None);
        resolve(&mut board, form, 1, None);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 14_u8, "only OnEvolve heals");
        assert_eq!(
            state["P1"]["cemetery"],
            json!(["h0", "h1", "h2"]),
            "the fee and the later two-card batch all reach the cemetery"
        );
        assert_eq!(
            state["P1"]["hand"],
            json!([]),
            "the later discard-all action removes both remaining cards"
        );
        assert_eq!(
            state["P1"]["pp"]["current"], 9_u8,
            "ordinary evolution spends exactly one PP"
        );
        assert_eq!(
            state["P2"]["leader"]["life"], 16_u8,
            "one cost discard and one later two-card action each trigger once"
        );
        assert!(
            board.internal_pending_choice().unwrap().is_none(),
            "every expected printed trigger has resolved without an extra occurrence"
        );
    }
}

fn end_position(light: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP12-052"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"drop0","card":if light { "BP01-170" } else { "BP01-171" }},
        {"id":"drop1","card":"BP01-171"}
    ]);
    for index in 0_u8..7 {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("keep{index}"),"card":"BP01-171"}));
    }
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"enemy-draw","card":"BP01-171"}]);
    initial
}

fn end_phase(board: &mut Game) {
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    while let Some(question) = board.internal_quick_choice().unwrap() {
        board
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish_checks(board);
    }
}

#[test]
fn one_real_end_limit_selection_of_two_cards_has_one_batch_occurrence() {
    let mut board = game(&end_position(false), false);
    end_phase(&mut board);
    board
        .internal_rule_action(&json!({"do":"end-discard","select":["drop0","drop1"]}))
        .unwrap();
    assert_eq!(
        board.projection(View::Referee).unwrap()["turn"]["active"],
        "P1",
        "the end-discard trigger must finish before the turn advances"
    );
    resolve(&mut board, "BP12-052", 1, None);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"].as_array().unwrap().len(),
        7,
        "end discard restores the seven-card hand limit"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["drop0", "drop1"]),
        "the selected pair is discarded together"
    );
    assert_eq!(
        state["P2"]["leader"]["life"], 18_u8,
        "one discard batch causes exactly two leader damage"
    );
    assert_eq!(
        state["turn"]["active"], "P2",
        "the turn advances only after all discard triggers resolve"
    );
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "every expected printed trigger has resolved without an extra occurrence"
    );
}

#[test]
fn a_light_path_draw_then_a_new_end_discard_produces_a_second_real_batch() {
    let mut board = game(&end_position(true), false);
    end_phase(&mut board);
    board
        .internal_rule_action(&json!({"do":"end-discard","select":["drop0","drop1"]}))
        .unwrap();
    resolve(&mut board, "BP12-052", 2, None);
    resolve(&mut board, "BP01-170", 1, None);
    let after_draw = board.projection(View::Referee).unwrap();
    assert_eq!(
        after_draw["P1"]["hand"].as_array().unwrap().len(),
        8,
        "Light Path draws above the limit after the first batch"
    );
    assert_eq!(
        after_draw["turn"]["active"], "P1",
        "the extra drawn card requires another end discard"
    );
    board
        .internal_rule_action(&json!({"do":"end-discard","select":["drawn"]}))
        .unwrap();
    assert_eq!(
        board.projection(View::Referee).unwrap()["turn"]["active"],
        "P1",
        "the end-discard trigger must finish before the turn advances"
    );
    resolve(&mut board, "BP12-052", 1, None);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"].as_array().unwrap().len(),
        7,
        "end discard restores the seven-card hand limit"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["drop0", "drop1", "drawn"]),
        "the later end discard forms a new batch"
    );
    assert_eq!(
        state["P2"]["leader"]["life"], 16_u8,
        "the two distinct batches each deal two damage"
    );
    assert_eq!(
        state["turn"]["active"], "P2",
        "the turn advances only after all discard triggers resolve"
    );
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "every expected printed trigger has resolved without an extra occurrence"
    );
}
