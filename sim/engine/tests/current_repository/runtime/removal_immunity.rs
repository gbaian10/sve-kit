use super::damage_prevent::field_name;
use super::event_cemetery::move_named;
use super::*;
use sve_engine::game::internal::InstructionOutcome;

fn position(card: &str, acted: bool, silenced: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"protected","card":card,"state":{"acted":acted,"silenced":silenced}}
    ]);
    initial
}

fn hit(name: &str, amount: i64) -> Action {
    Action::Damage {
        recipients: RecipientSet::Objects {
            objects: field_name(name),
        },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(amount)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn bp22_061_always_forbids_destruction_but_only_acted_prevents_ability_damage() {
    let name = "黒白の乱舞・ノール＆ブラン";
    for acted in [false, true] {
        let initial = position("BP22-061", acted, false);
        let mut game = with_producer(
            &initial,
            Action::Destroy {
                objects: field_name(name),
            },
        );
        let destruction = activate(&mut game, "P1");
        assert!(destruction.progress.receipts.values().any(|outcome| matches!(outcome, InstructionOutcome::Destroy(result) if result.successful.is_empty())), "a forbidden destruction has no successful members");
        finish(&mut game);
        assert!(
            game.projection(View::Referee).unwrap()["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("protected")),
            "posture never limits the independent destruction prohibition"
        );
        let mut damage_game = with_producer(&initial, hit(name, 4));
        activate(&mut damage_game, "P1");
        finish(&mut damage_game);
        assert_eq!(
            damage_game.projection(View::Referee).unwrap()["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("protected")),
            acted
        );
    }
}

#[test]
fn bp17_092_destruction_immunity_does_not_stop_putting_into_the_cemetery() {
    let initial = position("BP17-092", false, false);
    let action = move_named(ZoneKind::Field, OwnerZone::Cemetery, "レリックゴッデス");
    let mut game = with_producer(&initial, action);
    activate(&mut game, "P1");
    finish(&mut game);
    assert!(
        !game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("protected")),
        "putting into the cemetery bypasses destruction immunity before Advance cleanup"
    );
}

#[test]
fn bp06_022_banish_immunity_disappears_when_abilities_are_lost() {
    for silenced in [false, true] {
        let initial = position("BP06-022", false, silenced);
        let mut game = with_producer(
            &initial,
            Action::Banish {
                objects: field_name("古の英雄"),
            },
        );
        activate(&mut game, "P1");
        finish(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("protected")),
            !silenced
        );
    }
}
