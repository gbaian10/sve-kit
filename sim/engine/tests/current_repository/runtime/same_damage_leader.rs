#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the simultaneous damage scenarios."
)]
use super::*;
use sve_engine::game::internal::{DamageShare, InstructionOutcome};

const BASE: &str = "damage-base";
const A: &str = "damage-target-a";
const B: &str = "damage-target-b";

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name) in [(BASE, "機鋒の咎人・カットスロート"), (A, "標的一"), (B, "標的二")] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","traits":[],"cost":"1","power":"2","hp":"5","text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn fixture(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "same-damage",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [A, B] {
        game.load_internal_card(number, &empty).unwrap();
    }
    // BP19-110 also needs a shuffle of observed cards, so the fixture supplies the evolution producer.
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({BASE:tree["cards"]["synthetic-evolution"]});
    let program = lowerer().lower(&tree).unwrap().remove(BASE).unwrap();
    game.load_internal_card(BASE, &program).unwrap();
    game
}

fn initial() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy-a","card":A,"owner":"P1"},{"id":"enemy-b","card":B}]);
    initial
}

fn pending_line(game: &mut Game, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    let option = options.iter().find(|option| option.line == line).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn target(game: &mut Game, play: CardPlay, numbers: &[&str]) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed formal targets")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("public followers")
    };
    let handles = numbers
        .iter()
        .map(|number| {
            domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some(*number))
                .unwrap()
                .handle
        })
        .collect();
    game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
        .unwrap()
}

fn combined_damage(activation: &Activation, amount: i64) {
    let receipts = activation
        .progress
        .receipts
        .values()
        .filter_map(|outcome| {
            if let InstructionOutcome::Damage(value) = outcome {
                Some(*value)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        receipts.len(),
        1,
        "both recipients share one damage instruction receipt"
    );
    assert_eq!(
        receipts[0].requested,
        Int(amount.checked_mul(2).unwrap()),
        "the receipt sums the requested damage over both recipients"
    );
    assert_eq!(
        receipts[0].actual,
        Nat(u64::try_from(amount.checked_mul(2).unwrap()).unwrap()),
        "the receipt records both applied damage amounts"
    );
    let hits = activation
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(event) = fact {
                Some(event)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        hits.len(),
        2,
        "one follower and its controller's leader take damage together"
    );
    assert!(
        hits.iter()
            .all(|event| event.amount.0.get() == u64::try_from(amount).unwrap()),
        "each recipient takes the printed amount"
    );
    assert!(
        hits.iter().any(|event| event.recipient
            == RuntimeDamageRecipient::Leader {
                player: PlayerId { seat: 2 }
            }),
        "the controller receives the leader hit"
    );
}

#[test]
fn bp19_035_damages_the_selected_follower_and_its_controller_in_one_instruction() {
    for available in [true, false] {
        let mut initial = initial();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP19-035"}]);
        initial["players"]["P2"]["zones"]["field"] = if available {
            json!([{"id":"enemy-a","card":A,"owner":"P1"}])
        } else {
            json!([])
        };
        let mut game = fixture(&initial);
        let mut play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        if let CardPlay::Awaiting(question) = play {
            assert!(matches!(question.domain, InputDomain::WardEntry { .. }));
            play = game
                .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
                .unwrap();
        }
        let CardPlay::Entered(entry) = play else {
            panic!("printed base entry")
        };
        if entry.resume == ResumeStage::CheckTiming {
            game.resume_internal_card().unwrap();
        }
        let trigger = pending_line(&mut game, 2);
        if available {
            let damage = target(&mut game, trigger, &[A]);
            let activation = resolved(damage);
            combined_damage(&activation, 3);
        } else {
            assert!(
                matches!(trigger, CardPlay::Cancelled),
                "no follower target cancels the whole ability"
            );
        }
        game.resume_internal_card().unwrap();
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P2"]["leader"]["life"],
            if available { 17_u8 } else { 20 }
        );
        assert_eq!(
            view["P1"]["leader"]["life"], 20_u8,
            "the follower's owner is not its leader for this effect"
        );
        if available {
            assert_eq!(view["objects"]["enemy-a"]["damage"], 3_u8);
        }
    }
}

#[test]
fn bp19_111_super_evolution_keeps_split_four_distinct_from_combined_eight() {
    let mut initial = initial();
    initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":BASE}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP19-SL27"}]);
    let mut game = fixture(&initial);
    assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}})).unwrap().outcome,"resolved");
    let play = pending_line(&mut game, 1);
    let CardPlay::Awaiting(allocation) = target(&mut game, play, &[A, B]) else {
        panic!("printed split damage")
    };
    let InputDomain::Distribution(domain) = &allocation.domain else {
        panic!("four damage budget")
    };
    assert_eq!(domain.total, Nat(4));
    let shares = domain
        .members
        .iter()
        .map(|member| DamageShare {
            handle: member.handle,
            amount: Nat(match member.card.as_deref() {
                Some(A) => 1,
                Some(B) => 3,
                _ => panic!("two selected followers"),
            }),
        })
        .collect();
    resolved(
        game.answer_internal_card(
            allocation.actor,
            allocation.token,
            Answer::Distribution(shares),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["P2"]["leader"]["life"], 20_u8,
        "ordinary evolution damage never includes a leader"
    );
    assert_eq!(before["objects"]["enemy-a"]["damage"], 1_u8);
    assert_eq!(before["objects"]["enemy-b"]["damage"], 3_u8);
    let super_evolution = pending_line(&mut game, 2);
    let damage = target(&mut game, super_evolution, &[A]);
    combined_damage(&resolved(damage), 8);
    game.resume_internal_card().unwrap();
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["leader"]["life"], 12_u8);
    assert_eq!(after["P1"]["leader"]["life"], 20_u8);
    assert_eq!(
        after["P1"]["cemetery"],
        json!(["enemy-a"]),
        "the controller receives leader damage even when the victim returns to its owner's cemetery"
    );
    assert_eq!(after["objects"]["enemy-b"]["damage"], 3_u8);
}
