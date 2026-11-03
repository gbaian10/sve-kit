#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed payment scenario."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

pub(super) fn prepared() -> &'static SelectedEffects {
    static PREPARED: OnceLock<SelectedEffects> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| matches!(author["input"]["card_no"].as_str(), Some("BP07-030" | "BP07-T03" | "CP02-T01" | "BP03-T02" | "BP08-035" | "BP08-036" | "BP04-T01" | "BP11-068")));
        for (number,hp) in [("followup-neutral",20_u8),("followup-weak",2),("followup-large",20)] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","traits":[],"cost":if number=="followup-large" {"6"} else {"1"},"power":"1","hp":hp.to_string(),"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

pub(super) fn board(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "material-followup",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["followup-neutral", "followup-weak", "followup-large"] {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}

pub(super) fn declare(game: &mut Game, line: u64) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed Fanfare")
    };
    let handle = options
        .iter()
        .find(|option| option.line == line)
        .unwrap()
        .handle;
    game.answer_internal_card(q.actor, q.token, Answer::Pending(handle))
        .unwrap()
}

fn answer(game: &mut Game, q: &DecisionView, answer: Answer) -> CardPlay {
    game.answer_internal_card(q.actor, q.token, answer).unwrap()
}

fn finish_fanfare(game: &mut Game, mut play: CardPlay, destination: Option<usize>) {
    while let CardPlay::Awaiting(q) = play {
        let selected = match &q.domain {
            InputDomain::Integer(bounds) => {
                assert_eq!(
                    (bounds.minimum, bounds.maximum),
                    (Nat(0), Some(Nat(1))),
                    "the resolution optional decision is binary"
                );
                Answer::Integer(Nat(u64::from(destination.is_some())))
            }
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[destination.unwrap()]]),
            _ => panic!("optional destination"),
        };
        play = answer(game, &q, selected);
    }
    resolved(play);
    game.resume_internal_card().unwrap();
}

fn pay(game: &mut Game, mut play: CardPlay, count: u64, available: usize) {
    while let CardPlay::Awaiting(q) = play {
        let selected = match &q.domain {
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the opposing follower is a formal target"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    available,
                    "only standing own field Trees can pay"
                );
                let before = game.projection(View::Referee).unwrap();
                game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![99]))
                    .unwrap_err();
                assert_eq!(
                    game.projection(View::Referee).unwrap(),
                    before,
                    "an invalid material handle preserves the pending play"
                );
                Answer::CostMaterials((0..count).collect())
            }
            _ => panic!("target then standing Tree materials"),
        };
        play = answer(game, &q, selected);
    }
    resolved(play);
    game.resume_internal_card().unwrap();
}

#[test]
fn halberd_wolf_both_printings_choose_a_real_tree_destination_and_damage_by_only_paid_acts() {
    for printing in ["BP07-030", "PR-226"] {
        for (destination, count) in [(None, 0_u64), (Some(0_usize), 2), (Some(1), 1)] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"tree","card":"BP07-T03","state":{"acted":false}},{"id":"acted-tree","card":"BP07-T03","state":{"acted":true}},{"id":"wrong-name","card":"followup-neutral","state":{"acted":false}}]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex-tree","card":"BP07-T03"}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"followup-neutral"},{"id":"foreign-tree","card":"BP07-T03"}]);
            let mut game = board(&initial);
            assert!(matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            let fanfare = declare(&mut game, 1);
            finish_fanfare(&mut game, fanfare, destination);
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(
                before["P1"]["pp"]["current"], 8_u8,
                "the real follower costs two PP"
            );
            assert_eq!(
                before["P1"]["field"].as_array().unwrap().len(),
                4 + usize::from(destination == Some(0))
            );
            assert_eq!(
                before["P1"]["ex"].as_array().unwrap().len(),
                1 + usize::from(destination == Some(1))
            );
            let ability = prepared().current[printing]
                .program
                .card()
                .abilities
                .iter()
                .find(|a| a.source.line == 2)
                .unwrap()
                .node
                .id;
            let play = game
                .activate_internal_card("source", ability, PlayerId { seat: 1 })
                .unwrap();
            pay(
                &mut game,
                play,
                count,
                1 + usize::from(destination == Some(0)),
            );
            let paid = game.projection(View::Referee).unwrap();
            assert_eq!(
                paid["objects"]["enemy"]["hp"],
                20 - count,
                "damage reads completed material Acts"
            );
            assert_eq!(paid["objects"]["source"]["acted"], true);
            assert_eq!(paid["objects"]["tree"]["acted"], count > 0);
            assert_eq!(paid["objects"]["wrong-name"]["acted"], false);
            assert_eq!(paid["objects"]["foreign-tree"]["acted"], false);
            assert_eq!(
                paid["P1"]["ex"], before["P1"]["ex"],
                "EX Trees cannot pay field Acts"
            );
            assert_eq!(
                paid["P1"]["pp"], before["P1"]["pp"],
                "the activated fee has no PP"
            );
        }
    }
}
