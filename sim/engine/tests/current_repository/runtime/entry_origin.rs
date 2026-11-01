#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these complete damage-cap scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn all_other_followers() -> RecipientSet {
    let mut objects = subjects();
    if let Selector::Zone {
        binding, predicate, ..
    } = &mut objects
    {
        **predicate = Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding: *binding }),
                },
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(CardKind::Follower),
        }));
    }
    RecipientSet::Objects {
        objects: Selector::Difference {
            input: Box::new(objects),
            except: Box::new(Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            }),
        },
    }
}

fn damage() -> Action {
    Action::Damage {
        recipients: all_other_followers(),
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(4)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}

fn strike_with_producer(game: &mut Game) -> Activation {
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    )
}

fn pending(game: &mut Game, number: &str) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("real printed trigger")
    };
    assert_eq!(
        options[0].card, number,
        "the effective printing supplies its complete trigger"
    );
    game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}

#[test]
fn evolved_sofia_all_printings_search_the_real_amulet_and_cap_damage_after_evolution() {
    for number in ["BP10-093", "BP10-SL23"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":"BP10-092"},{"id":"ally","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"arms","card":"BP10-098"},{"id":"remaining","card":"BP01-037"}
        ]);
        let mut game = with_producer(&initial, damage());
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real base evolves using its printed cost"
        );
        let CardPlay::Awaiting(q) = pending(&mut game, number) else {
            panic!("named search")
        };
        let InputDomain::Search(domain) = &q.domain else {
            panic!("search domain")
        };
        assert_eq!(
            domain.cards.len(),
            1,
            "only the real named amulet matches the search"
        );
        resolved(
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let CardPlay::Awaiting(entry_question) = pending(&mut game, "BP10-098") else {
            panic!("the searched amulet's fanfare")
        };
        assert!(
            matches!(q.domain, InputDomain::Targets(_)),
            "the searched real amulet offers up to two followers"
        );
        resolved(
            game.answer_internal_card(
                entry_question.actor,
                entry_question.token,
                Answer::Targets(vec![]),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        strike_with_producer(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["source"]["evolved_with"], "form",
            "the evolved resource supplies the static cap"
        );
        assert_eq!(
            view["objects"]["source"]["damage"], 3_u8,
            "the evolved provider also receives its own cap"
        );
        assert_eq!(
            view["objects"]["ally"]["damage"], 3_u8,
            "the allied follower receives the same cap"
        );
        assert!(
            view["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("arms")),
            "the real searched amulet remains on field"
        );
        assert_eq!(
            view["P1"]["pp"]["current"], 9_u8,
            "evolution pays exactly one PP"
        );
    }
}

#[test]
fn star_priestess_returns_a_cemetery_amulet_and_its_fanfare_runs() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP04-106"},{"id":"fee","card":"BP10-098"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP04-107"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"arms","card":"BP10-098"},
        {"id":"wrong-kind","card":"BP01-037"}
    ]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "star-priestess",
    )
    .unwrap();
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":0_u8},"costs":{"materials":["fee"]}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    let CardPlay::Awaiting(q) = pending(&mut game, "BP04-107") else {
        panic!("cemetery target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("amulet target")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the follower in the cemetery is not eligible"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let CardPlay::Awaiting(entry_question) = pending(&mut game, "BP10-098") else {
        panic!("returned amulet fanfare")
    };
    let InputDomain::Targets(buff_targets) = &entry_question.domain else {
        panic!("buff target")
    };
    resolved(
        game.answer_internal_card(
            entry_question.actor,
            entry_question.token,
            Answer::Targets(vec![buff_targets.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let view = game.projection(View::Referee).unwrap();
    assert!(
        view["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("arms"))
    );
    assert_eq!(view["objects"]["arms"]["generation"], 1_u8);
    assert_eq!(view["objects"]["source"]["power"], 5_u8);
    assert_eq!(view["objects"]["source"]["hp"], 5_u8);
    assert_eq!(view["P1"]["cemetery"], json!(["wrong-kind"]));
    assert!(game.internal_pending_choice().unwrap().is_none());
}
