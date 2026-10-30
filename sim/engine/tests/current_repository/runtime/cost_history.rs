#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::{InstructionOutcome, SearchAnswer};

fn initial(card: &str) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    value
}

fn play(game: &mut Game, id: &str) {
    assert!(
        matches!(
            game.play_internal_card(id, PlayerId { seat: 1 }).unwrap(),
            CardPlay::Resolved(_) | CardPlay::Entered(_)
        ),
        "the printed card finishes its play without a choice"
    );
    finish_checks(game);
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn trigger(game: &mut Game) -> Activation {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending Fanfare")
    };
    assert_eq!(
        options.len(),
        1,
        "the printed Fanfare is the sole pending effect"
    );
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    finish_checks(game);
    result
}

fn created(result: &Activation) -> Vec<ObjectRef> {
    result
        .progress
        .receipts
        .values()
        .filter_map(|outcome| {
            if let InstructionOutcome::Token(outcome) = outcome {
                Some(outcome.created.clone())
            } else {
                None
            }
        })
        .flatten()
        .collect()
}

fn installed(result: &Activation) -> Vec<ObjectRef> {
    result
        .progress
        .receipts
        .values()
        .filter_map(|outcome| {
            if let InstructionOutcome::Installed(objects) = outcome {
                Some(objects.clone())
            } else {
                None
            }
        })
        .flatten()
        .collect()
}

#[test]
fn bp15_073_discounts_its_created_dragon_only_when_awakened() {
    for maximum in [6_u8, 7_u8] {
        let mut value = initial("BP15-073");
        value["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
        let mut game = game(&value);
        play(&mut game, "source");
        let result = trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], maximum - 2);
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
        let dragon = state["P1"]["ex"][0].as_str().unwrap();
        assert_eq!(state["objects"][dragon]["name"], "ドラゴン");
        let created = created(&result);
        assert_eq!(
            created.len(),
            1,
            "one native Dragon was successfully created"
        );
        assert_eq!(
            installed(&result),
            if maximum == 7 { created } else { vec![] },
            "only the awakened Fanfare installs a modifier on its successful creation"
        );
    }
}

#[test]
fn a_creation_discount_installs_only_on_successfully_created_generations() {
    for existing in [4_usize, 5] {
        let mut value = initial("BP15-073");
        value["players"]["P1"]["zones"]["ex"] = json!(
            (0..existing)
                .map(|i| json!({"id":format!("old-{i}"),"card":"BP13-T10"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&value);
        play(&mut game, "source");
        let result = trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 5);
        let created = state["P1"]["ex"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|id| id.as_str())
            .find(|id| !id.starts_with("old-"));
        assert_eq!(
            created.is_some(),
            existing == 4,
            "only successful creation enters the output binding"
        );
        assert_eq!(
            installed(&result),
            self::created(&result),
            "the modifier subjects are exactly the successful creations, including no creation when EX is full"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            8_u8,
            "creation and installation consume only the real follower's two PP"
        );
    }
}

fn search_cat(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::Search(domain) = &question.domain else {
            panic!("named deck search")
        };
        assert_eq!(
            domain.cards.len(),
            1,
            "only the native Lightning Cat matches"
        );
        assert_eq!(
            domain.cards[0].card, "BP19-050",
            "the search keeps its printed name filter"
        );
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
            )
            .unwrap();
    }
    resolved(play);
    finish_checks(game);
}

#[test]
fn bp19_054_installs_the_sc_seven_discount_on_the_searched_native_cat_only() {
    for count in [6_usize, 7] {
        let mut value = initial("BP19-054");
        value["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("grave-{i}"),"card":"SD03-005"}))
                .collect::<Vec<_>>()
        );
        value["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"wanted","card":"BP19-050"},{"id":"wrong","card":"BP01-037"}]);
        value["players"]["P1"]["zones"]["ex"] = json!([{"id":"old","card":"BP19-050"}]);
        let mut game = game(&value);
        let source = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        search_cat(&mut game, source);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("wanted"))
        );
        assert_eq!(state["P1"]["deck"], json!(["wrong"]));
        let before = state["P1"]["pp"]["current"].as_u64().unwrap();
        play(&mut game, "wanted");
        let after = before - if count == 7 { 1 } else { 2 };
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            after,
            "the resolving spell does not count toward its own SC threshold"
        );
        play(&mut game, "old");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            after - 2,
            "only the successful Search After gets the discount"
        );
    }
}

#[test]
fn created_bound_cost_modifiers_reduce_actual_amulet_token_payment() {
    let output = Binding::new(50, ScopeId(1));
    let create = Action::Token {
        tokens: vec![TokenSpec {
            template: TokenTemplateId("ナテラの大樹".into()),
            name: Expr::Literal(CardName("ナテラの大樹".into())),
            count: Expr::Literal(Nat(1)),
        }],
        destination: TokenDestination::Single {
            zone: Expr::Literal(ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Ex,
            }),
        },
        output,
        initial_state: None,
        initial_posture: Posture::Standing,
        entry_effects: vec![],
    };
    let affected = Binding::new(90, ScopeId(90));
    let duration = Duration {
        end: DurationEnd::CurrentTurnEnd {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    };
    let discount = Action::InstallEffect {
        subjects: InstallationSubjects::Objects {
            objects: Selector::Bound { reference: output },
            affected,
        },
        capture: InstallationTime::InstructionStart,
        effect: Box::new(Effect::CostModifier {
            subjects: Selector::Bound {
                reference: affected,
            },
            change: CostChange::Delta {
                amount: Expr::Literal(Int(-1)),
            },
            duration: duration.clone(),
            usage: None,
        }),
        duration,
    };
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["field"] = json!([]);
    value["players"]["P1"]["zones"]["ex"] = json!([{"id":"old","card":"BP07-T03"}]);
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
    let mut game = Game::from_selected_effects(
        producer_selection().clone(),
        &value,
        &Value::Null,
        &Value::Null,
        "created-cost",
    )
    .unwrap();
    game.load_internal_card("producer", &producer_steps(vec![create, discount]))
        .unwrap();
    activate(&mut game, "P1");
    finish_checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    let new = state["P1"]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| *id != "old")
        .unwrap();
    for (id, remaining) in [(new, 10_u8), ("old", 9_u8)] {
        let mut attempt = game.clone();
        assert!(
            matches!(
                attempt
                    .play_internal_card(id, PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the real amulet token enters through normal card payment"
        );
        assert_eq!(
            attempt.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            remaining,
            "only the successful created Bound receives the one-PP discount"
        );
    }
}
