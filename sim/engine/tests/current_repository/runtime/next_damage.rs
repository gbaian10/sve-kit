#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed next-damage scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn finish(game: &mut Game, mut play: CardPlay, mode: usize) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[mode]]),
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::OptionalCost
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. } => panic!("unexpected printed card question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn end_turn(game: &mut Game) {
    let active = game.projection(View::Referee).unwrap()["turn"]["active"].clone();
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    finish_checks(game);
    if game.projection(View::Referee).unwrap()["turn"]["active"] == active {
        if let Some(question) = game.internal_quick_choice().unwrap() {
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap();
            finish_checks(game);
        }
        if game.projection(View::Referee).unwrap()["turn"]["active"] == active {
            game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
                .unwrap();
            finish_checks(game);
        }
    }
}

const fn own_leader_damage(amount: i64) -> Action {
    Action::Damage {
        recipients: RecipientSet::Leaders {
            players: PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::One,
            },
        },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(amount)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}

fn hit(game: &mut Game) {
    resolved(
        game.activate_internal_card("producer", AbilityId(1), actor())
            .unwrap(),
    );
    finish_checks(game);
}

fn activate_line(
    game: &mut Game,
    source: &str,
    printing: &str,
    line: u64,
) -> Result<CardPlay, EngineFailure> {
    let definition=selection().current[printing].program.card().abilities.iter().find(|ability| {
        ability.node.kind == AbilityKind::Activated && matches!(&ability.node.source.origin, AbilityOrigin::Printed { line: printed, .. } if printed.0 == line)
    }).unwrap().node.id;
    game.activate_internal_card(source, definition, actor())
}

#[test]
fn both_spear_printings_leave_as_cost_and_protect_only_the_selected_rowen_once() {
    for printing in ["BP17-062", "BP17-P16"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"spear","card":printing},{"id":"rowen","card":"BP17-055"}]);
        let action = Action::Damage {
            recipients: RecipientSet::Objects {
                objects: damage_prevent::field_name("竜槍の戦士・ローウェン"),
            },
            amount: DamageAmount::Uniform {
                amount: Expr::Literal(Int(1)),
            },
            flags: DamageFlags::Ability,
            source: ObjectView::SourceInformation {},
            batch: BatchPolicy::Instruction {},
        };
        let mut game = with_producer(&initial, action);
        let play = activate_line(&mut game, "spear", printing, 2).unwrap();
        resolved(finish(&mut game, play, 0));
        finish_checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"],
            json!(["spear"])
        );
        hit(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["rowen"]["damage"],
            0_u8
        );
        hit(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["rowen"]["damage"],
            1_u8
        );
    }
}

#[test]
fn spear_without_an_own_rowen_cannot_pay_its_departure_cost() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"spear","card":"BP17-062"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy-rowen","card":"BP17-055"}]);
    let mut game = with_producer(&initial, own_leader_damage(1));
    let before = game.projection(View::Referee).unwrap();
    activate_line(&mut game, "spear", "BP17-062", 2).unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}

#[test]
fn both_gordon_printings_choose_damage_or_a_single_reduction() {
    for printing in ["CP03-040", "CP03-P28"] {
        for mode in [0_usize, 1] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"gordon","card":printing}]);
            initial["players"]["P1"]["zones"]["field"] = json!([]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
            let mut game = with_producer(&initial, own_leader_damage(7));
            let step = game.play_internal_card("gordon", actor()).unwrap();
            resolved(finish(&mut game, step, mode));
            finish_checks(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if mode == 0 { 3_u8 } else { 0_u8 }
            );
            hit(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                if mode == 0 { 13_u8 } else { 18_u8 }
            );
            hit(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                if mode == 0 { 6_u8 } else { 11_u8 }
            );
        }
    }
}

#[test]
fn both_sheep_printings_install_on_opponents_main_and_expire_at_that_turn_end() {
    for printing in ["BP20-099", "PR-550"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"sheep","card":printing}]);
        initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"return-draw","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"opponent-producer","card":"producer"}]);
        let mut game = with_producer(&initial, damage_prevent::all_leaders_damage());
        end_turn(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["turn"]["active"],
            "P2"
        );
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("Sheep Main trigger")
        };
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].card, printing);
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        resolved(step);
        finish_checks(&mut game);
        let mut expiry = game.clone();
        for life in [20_u8, 18] {
            resolved(
                game.activate_internal_card(
                    "opponent-producer",
                    AbilityId(1),
                    PlayerId { seat: 2 },
                )
                .unwrap(),
            );
            finish_checks(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                life
            );
        }
        // The unspent copy expires at this opponent turn's end, independently of consumption.
        end_turn(&mut expiry);
        assert_eq!(
            expiry.projection(View::Referee).unwrap()["turn"]["active"],
            "P1"
        );
        hit(&mut expiry);
        assert_eq!(
            expiry.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            18_u8
        );
    }
}

#[test]
fn gordon_quick_reduction_survives_zero_attack_and_applies_to_one_positive_attack() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"first","card":"BP01-037","state":{"power":0_u8}},
        {"id":"second","card":"BP01-037"}, {"id":"third","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"gordon","card":"CP03-040"}]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "gordon-quick",
    )
    .unwrap();
    game.internal_rule_action(&json!({"do":"attack","attacker":"first","target":"P2.leader"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(question.actor, PlayerId { seat: 2 });
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick spell")
    };
    let option = options
        .iter()
        .find(|option| option.card == "CP03-040")
        .unwrap();
    let step = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Quick(Some(option.handle)),
        )
        .unwrap();
    resolved(finish(&mut game, step, 1));
    finish_checks(&mut game);
    damage_prevent::pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_u8
    );
    game.internal_rule_action(&json!({"do":"attack","attacker":"second","target":"P2.leader"}))
        .unwrap();
    damage_prevent::pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_u8
    );
    game.internal_rule_action(&json!({"do":"attack","attacker":"third","target":"P2.leader"}))
        .unwrap();
    damage_prevent::pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        15_u8
    );
}

#[test]
fn gordon_without_enemy_followers_offers_only_the_untargeted_reduction_before_payment() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"gordon","card":"CP03-040"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = with_producer(&initial, own_leader_damage(7));
    let CardPlay::Awaiting(question) = game.play_internal_card("gordon", actor()).unwrap() else {
        panic!("printed Choice before payment");
    };
    let InputDomain::Modes { modes, .. } = &question.domain else {
        panic!("Choice modes")
    };
    assert_eq!(modes, &[ModeId(2)]);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        10_u8
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![modes[0]]),
        )
        .unwrap(),
    );
    finish_checks(&mut game);
    hit(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        18_u8
    );
}
