#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "delayed-entry",
    )
    .unwrap()
}

/// P1 holds three vanilla followers; P2 has one follower to target.
fn position() -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"first","card":"BP01-037"},
        {"id":"second","card":"BP01-037"},
        {"id":"third","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top","card":"BP01-037"},
        {"id":"next","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":12_u8,"max_hp":12_u8}}]);
    initial
}

/// Answers modes with `modes`, the opposing follower for targets, the last quota candidate,
/// and resolves pending abilities; returns each quota question's candidate count.
fn settle(game: &mut Game, mut step: Option<CardPlay>, modes: &[ModeId]) -> Vec<usize> {
    let mut quotas = vec![];
    for _ in 0_u8..32_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(d) => Answer::Targets(
                    d.members
                        .iter()
                        .filter(|m| m.player.seat == 2)
                        .map(|m| m.handle)
                        .take(1)
                        .collect(),
                ),
                InputDomain::Modes { .. } => Answer::Modes(modes.to_vec()),
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::Quick { .. } => Answer::Quick(None),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                InputDomain::TriggerQuota(d) => {
                    quotas.push(d.candidates.len());
                    Answer::TriggerQuota(d.candidates.last().unwrap().handle)
                }
                _ => panic!("unexpected decision: {:?}", q.domain),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return quotas;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("expected a pending ability");
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
    panic!("scenario did not settle");
}

fn play(game: &mut Game, id: &str) -> Vec<usize> {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    settle(game, Some(step), &[])
}

fn activate(game: &mut Game, card: &str, line: u32, modes: &[ModeId]) {
    let program = &selection_with_rules().current[card].program;
    let ability = program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", ability, PLAYER).unwrap();
    assert!(settle(game, Some(step), modes).is_empty());
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

fn grown(view: &Value, id: &str, printed: &Value) -> bool {
    view["objects"][id]["power"].as_u64().unwrap() == printed["power"].as_u64().unwrap() + 1
        && view["objects"][id]["hp"].as_u64().unwrap() == printed["hp"].as_u64().unwrap() + 1
}

#[test]
fn bp03_089_buffs_only_the_next_played_follower() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"command","card":"BP03-089"}));
    let mut game = loaded(&initial);
    assert!(play(&mut game, "command").is_empty());
    let cast = state(&game);
    assert_eq!(cast["P1"]["leader"]["life"], 9_u8);
    assert!(play(&mut game, "first").is_empty());
    let entered = state(&game);
    let printed = entered["objects"]["second"].clone();
    assert!(play(&mut game, "second").is_empty());
    let view = state(&game);
    assert!(grown(&view, "first", &printed), "the next played follower");
    assert!(!grown(&view, "second", &printed), "only once");
}

#[test]
fn sp01_026_grows_the_next_entering_follower_and_draws() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"SP01-026"}]);
    let mut game = loaded(&initial);
    activate(&mut game, "SP01-026", 2, &[ModeId(2), ModeId(4)]);
    let activated = state(&game);
    assert_eq!(activated["P1"]["hand"].as_array().unwrap().len(), 4, "mode 4 draws");
    let printed = activated["objects"]["second"].clone();
    assert!(play(&mut game, "first").is_empty());
    assert!(play(&mut game, "second").is_empty());
    let view = state(&game);
    assert!(grown(&view, "first", &printed));
    assert!(!grown(&view, "second", &printed));
}

#[test]
fn sp01_026_grows_one_follower_of_a_simultaneous_entry() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"SP01-026"}]);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fairies","card":"BP10-018"}));
    let mut game = loaded(&initial);
    activate(&mut game, "SP01-026", 2, &[ModeId(2), ModeId(4)]);
    let before = state(&game);
    let printed = before["objects"]["second"].clone();
    assert_eq!(play(&mut game, "fairies"), [2], "both fairies compete for one occurrence");
    let view = state(&game);
    let fairies: Vec<&Value> = view["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| *id != "source")
        .collect();
    assert_eq!(fairies.len(), 2);
    let grown_fairies = fairies
        .iter()
        .filter(|id| {
            let object = &view["objects"][id.as_str().unwrap()];
            object["power"] == 2_u8 && object["hp"] == 2_u8
        })
        .count();
    assert_eq!(grown_fairies, 1, "only one of them");
    assert!(play(&mut game, "first").is_empty());
    assert!(!grown(&state(&game), "first", &printed), "the quota is spent");
}
