#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these complete printed discard scenarios."
)]
use super::event_cemetery::finish_checks;
use super::event_subject_capture::prepared_program;
use super::*;

fn discard_all() -> WellFormedCardProgram {
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
                    text_quantifier: TextQuantifier::All,
                },
                zone: ZoneKind::Hand,
                view: SelectionView::Current {},
                binding: Binding::new(400, ScopeId(1)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
        },
    })
}

fn initial(number: &str, hand: bool) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["field"] = json!([]);
    value["players"]["P1"]["zones"]["hand"] = json!([]);
    value["players"]["P1"]["zones"][if hand { "hand" } else { "field" }] =
        json!([{"id":"source","card":number}]);
    value["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"subject-draw"},{"id":"spare","card":"subject-draw"}
    ]);
    value["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"subject-low"},{"id":"other","card":"subject-soldier"}
    ]);
    value
}

fn finish(board: &mut Game, mut play: CardPlay, accept_cost: bool) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(accept_cost)
        } else if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            let (InputDomain::Targets(domain) | InputDomain::CostMaterials(domain)) =
                &question.domain
            else {
                panic!("printed target or material choice");
            };
            assert_eq!(
                domain.cardinality.minimum,
                Nat(1),
                "these printed choices require exactly one object"
            );
            let handles = vec![domain.members[0].handle];
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
    play
}

fn discard(board: &mut Game) {
    let play = board
        .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    resolved(play);
    finish_checks(board);
}

fn pending(board: &mut Game, number: &str, accept_cost: bool) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("one printed pending trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "only the expected printed trigger is pending"
    );
    assert_eq!(
        options[0].card, number,
        "the complete real card supplies the trigger"
    );
    let play = board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(board, play, accept_cost)
}

fn quiet(board: &mut Game) {
    finish_checks(board);
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "all printed occurrences are consumed without an extra trigger"
    );
}

fn activated(number: &str) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

#[test]
fn tyrannosaurus_printings_keep_optional_discard_damage_and_real_tree_activation() {
    // Reprints share the immutable selection preparation but each executes both printed abilities.
    for number in ["BP07-055", "PR-334"] {
        for accept_cost in [false, true] {
            let mut board = prepared_program(&initial(number, true), &discard_all());
            discard(&mut board);
            let outcome = pending(&mut board, number, accept_cost);
            if accept_cost {
                resolved(outcome);
            } else {
                assert!(
                    matches!(outcome, CardPlay::Cancelled),
                    "declining the fee cancels the body"
                );
            }
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["hp"],
                if accept_cost { 3_u8 } else { 6_u8 },
                "only a paid discard trigger damages its selected follower"
            );
            assert_eq!(
                state["objects"]["other"]["hp"], 6_u8,
                "discard damage has only one formal target"
            );
            assert_eq!(
                state["P1"]["pp"]["current"],
                if accept_cost { 9_u8 } else { 10_u8 },
                "declining preserves PP"
            );
            assert_eq!(
                state["P1"]["cemetery"],
                json!(["source"]),
                "the printed source really leaves hand"
            );
            quiet(&mut board);
        }
        let mut position = initial(number, false);
        position["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"tree","card":"BP07-T03"}));
        let mut board = prepared_program(&position, &discard_all());
        let play = board
            .activate_internal_card("source", activated(number), PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut board, play, true));
        let damaged = board.projection(View::Referee).unwrap();
        assert_eq!(
            damaged["objects"]["enemy"]["hp"], 3_u8,
            "the activated body damages every opposing follower"
        );
        assert_eq!(
            damaged["objects"]["other"]["hp"], 3_u8,
            "the second opposing follower is also damaged"
        );
        assert!(
            damaged["objects"].get("tree").is_none(),
            "the real token is banished as the cost"
        );
        assert_eq!(
            damaged["P2"]["leader"]["life"], 20_u8,
            "the leader is outside the printed recipient set"
        );
        resolved(pending(&mut board, "BP07-T03", true));
        let after_tree = board.projection(View::Referee).unwrap();
        assert_eq!(
            after_tree["P1"]["cemetery"],
            json!(["drawn"]),
            "the tree keeps its actual draw then discard leave ability"
        );
        assert_eq!(
            after_tree["P1"]["pp"]["current"], 10_u8,
            "the tree activation has no PP payment"
        );
        quiet(&mut board);
    }
}

fn dragon(state: &Value) {
    let tokens = state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| &state["objects"][id.as_str().unwrap()])
        .filter(|object| object["card"] == "BP01-T11")
        .collect::<Vec<_>>();
    assert_eq!(tokens.len(), 1, "exactly one canonical Dragon token enters");
    assert_eq!(
        tokens[0]["name"], "ドラゴン",
        "the token keeps the printed name"
    );
    assert_eq!(tokens[0]["power"], 5_u8, "Dragon uses its printed attack");
    assert_eq!(tokens[0]["hp"], 5_u8, "Dragon uses its printed health");
}

#[test]
fn cloud_rider_creates_its_real_dragon_after_paid_discard_or_ward_entry_fanfare() {
    for discarded in [true, false] {
        let mut board = prepared_program(&initial("BP16-072", true), &discard_all());
        if discarded {
            discard(&mut board);
        } else {
            let play = board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            assert!(
                matches!(finish(&mut board, play, true), CardPlay::Entered(_)),
                "the real Ward follower enters before its fanfare"
            );
        }
        resolved(pending(&mut board, "BP16-072", true));
        let state = board.projection(View::Referee).unwrap();
        dragon(&state);
        assert_eq!(
            state["P1"]["pp"]["current"],
            if discarded { 8_u8 } else { 3_u8 },
            "the discard fee and normal play cost stay distinct"
        );
        if discarded {
            assert_eq!(
                state["P1"]["cemetery"],
                json!(["source"]),
                "token creation does not return the discarded source"
            );
        } else {
            assert!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("守護")),
                "the whole card retains Ward"
            );
        }
        quiet(&mut board);
    }
}

#[test]
fn dark_eclipse_printings_keep_discard_damage_and_the_princess_connect_spell() {
    for number in ["CP04-054", "CP04-P42", "PCS01-021"] {
        let mut board = prepared_program(&initial(number, true), &discard_all());
        discard(&mut board);
        resolved(pending(&mut board, number, true));
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            5_u8,
            "each printing damages the selected enemy after actual discard"
        );
        quiet(&mut board);

        let mut position = initial(number, true);
        position["players"]["P1"]["zones"]["field"] =
            json!([{"id":"friend","card":"CP04-043"},{"id":"wrong-trait","card":"subject-low"}]);
        let mut spell_board = prepared_program(&position, &discard_all());
        let power = spell_board.projection(View::Referee).unwrap()["objects"]["friend"]["power"]
            .as_i64()
            .unwrap();
        let CardPlay::Awaiting(question) = spell_board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("formal Princess Connect target");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("formal spell target domain");
        };
        assert_eq!(
            domain.members.len(),
            1,
            "the non-Princess Connect follower is excluded"
        );
        let play = spell_board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
        resolved(finish(&mut spell_board, play, true));
        let state = spell_board.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["friend"]["power"],
            power + 3,
            "the spell grants its printed attack bonus"
        );
        for keyword in ["突進", "指定攻撃"] {
            assert!(
                state["objects"]["friend"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(keyword)),
                "the complete spell grants {keyword}"
            );
        }
        assert_eq!(
            state["objects"]["enemy"]["hp"], 6_u8,
            "normal spell resolution does not publish a discard event"
        );
        assert_eq!(
            state["P1"]["pp"]["current"], 8_u8,
            "normal play pays the printed two PP"
        );
        quiet(&mut spell_board);
    }
}

#[test]
fn unexpected_stumble_acts_after_discard_and_draws_even_if_the_spell_target_is_already_acted() {
    let mut board = prepared_program(&initial("BP07-034", true), &discard_all());
    discard(&mut board);
    resolved(pending(&mut board, "BP07-034", true));
    let discarded = board.projection(View::Referee).unwrap();
    assert_eq!(
        discarded["objects"]["enemy"]["acted"], true,
        "discarding the real spell acts its enemy target"
    );
    assert_eq!(
        discarded["P1"]["hand"],
        json!([]),
        "the discard trigger does not draw"
    );
    quiet(&mut board);

    let mut position = initial("BP07-034", true);
    position["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(true);
    let mut spell_board = prepared_program(&position, &discard_all());
    assert_eq!(
        spell_board.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
        true,
        "Q1000 starts with the explicitly selected follower already acted"
    );
    let CardPlay::Awaiting(question) = spell_board
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("the spell declares its already acted target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("the spell uses formal follower targets");
    };
    let target = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("subject-low"))
        .unwrap();
    let play = spell_board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![target.handle]),
        )
        .unwrap();
    resolved(finish(&mut spell_board, play, true));
    let state = spell_board.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["enemy"]["acted"], true,
        "an already acted follower stays acted"
    );
    assert_eq!(
        state["objects"]["other"]["acted"], false,
        "the other standing follower was not selected"
    );
    assert_eq!(
        state["P1"]["hand"],
        json!(["drawn"]),
        "Q1000 still draws when acting changes nothing"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 9_u8,
        "normal play pays the printed one PP"
    );
    quiet(&mut spell_board);
}
