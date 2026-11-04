#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these complete printed-card sequences."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::DecisionView;

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("the printed ability has its declared decision");
    };
    q
}
fn pending_card(board: &mut Game, number: &str) -> CardPlay {
    finish_checks(board);
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("a printed pending ability");
    };
    assert_eq!(
        options.len(),
        1_usize,
        "one event produces one occurrence, including dual traits"
    );
    assert_eq!(
        options[0].card, number,
        "the matching printed source must own the occurrence"
    );
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}
fn one_target(board: &mut Game, play: CardPlay) -> Activation {
    let q = awaiting(play);
    let InputDomain::Targets(domain) = q.domain else {
        panic!("one printed target");
    };
    assert_eq!(
        domain.members.len(),
        1_usize,
        "the board exposes exactly the printed target"
    );
    resolved(
        board
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
    )
}
fn producer_rules() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (snapshot, keywords, input) = producer_inputs();
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

const fn own_players() -> PlayerSet {
    PlayerSet {
        relation: PlayerRelation::SelfPlayer,
        relative_to: PlayerRef::Controller {},
        cardinality: Cardinality {
            minimum: Nat(0),
            maximum: None,
        },
        text_quantifier: TextQuantifier::Implicit,
    }
}
fn discount() -> Action {
    let duration = Duration {
        end: DurationEnd::CurrentTurnEnd {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    };
    Action::InstallEffect {
        subjects: InstallationSubjects::Players {
            players: own_players(),
            affected: Binding::new(1, ScopeId(90)),
        },
        capture: InstallationTime::InstructionStart,
        effect: Box::new(Effect::CostModifier {
            kind: CostKind::Play,
            subjects: Selector::Union {
                sets: [ZoneKind::Hand, ZoneKind::Ex]
                    .into_iter()
                    .zip([91_u64, 92])
                    .map(|(zone, scope)| Selector::Zone {
                        players: own_players(),
                        zone,
                        view: SelectionView::Current {},
                        binding: Binding::new(1, ScopeId(scope)),
                        predicate: Box::new(Expr::Literal(Bool(true))),
                    })
                    .collect(),
            },
            change: CostChange::Delta {
                amount: Expr::Literal(Int(-2)),
            },
            duration: duration.clone(),
            usage: None,
        }),
        duration,
    }
}

#[test]
fn both_frederica_prints_observe_the_discounted_six_cost_spell_after_its_body_and_draw() {
    for number in ["CP02-044", "CP02-P35"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"source","card":number},{"id":"spell","card":"BP01-115"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"revived","card":"BP01-171"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"drawn","card":"BP01-034"},{"id":"tail","card":"BP01-171"}]);
        let mut board = with_selected_producer(&initial, discount(), producer_rules());
        assert!(matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        finish_checks(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            8_u8
        );
        resolved(
            board
                .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        finish_checks(&mut board);
        let play = board
            .play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap();
        one_target(&mut board, play);
        finish_checks(&mut board);
        let before = board.projection(View::Referee).unwrap();
        assert_eq!(
            before["P1"]["pp"]["current"], 4_u8,
            "the real six-cost Spell pays four after the independent modifier"
        );
        assert!(
            before["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("spell")),
            "the Spell has left the committed Play generation before the observer resolves"
        );
        assert!(
            before["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("revived"))
        );
        let triggered = pending_card(&mut board, number);
        one_target(&mut board, triggered);
        finish_checks(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["leader"]["life"], 17_u8);
        assert_eq!(view["P1"]["hand"], json!(["drawn"]));
        assert_eq!(view["P1"]["deck"], json!(["tail"]));
    }
}

fn look_two(board: &mut Game, play: CardPlay) {
    let q = awaiting(play);
    let InputDomain::Targets(domain) = q.domain else {
        panic!("the complete real spell's Look");
    };
    assert_eq!(
        domain.members.len(),
        4_usize,
        "the printed spell looks at four cards"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(2),
        "the printed spell requires two cards"
    );
    resolved(
        board
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(domain.members.iter().take(2).map(|m| m.handle).collect()),
            )
            .unwrap(),
    );
}
#[test]
fn every_yukishima_print_shares_one_next_discount_and_observes_each_dual_trait_spell_once() {
    for number in ["BP14-036", "BP14-SL08", "BP14-U03", "EBD02-001"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number},{"id":"first","card":"BP14-042"},{"id":"second","card":"BP14-042"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!(
            (0_u8..8)
                .map(|i| json!({"id":format!("deck{i}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8}}]);
        let mut board = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "played-original-cost-real",
        )
        .unwrap();
        assert!(matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let fanfare = pending_card(&mut board, number);
        resolved(fanfare);
        finish_checks(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            6_u8
        );
        for (id, pp, hp) in [("first", 7_u8, 8_u8), ("second", 5, 6)] {
            let play = board.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
            look_two(&mut board, play);
            finish_checks(&mut board);
            let before = board.projection(View::Referee).unwrap();
            assert_eq!(
                before["P1"]["pp"]["current"], pp,
                "only the first three-cost spell is free; both complete bodies refund one PP"
            );
            assert!(
                before["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(id))
            );
            let triggered = pending_card(&mut board, number);
            one_target(&mut board, triggered);
            finish_checks(&mut board);
            assert_eq!(
                board.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
                hp
            );
            assert!(
                board.internal_pending_choice().unwrap().is_none(),
                "a card carrying both traits produces one occurrence"
            );
        }
    }
}
