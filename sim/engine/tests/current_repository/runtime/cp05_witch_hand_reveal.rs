
use super::*;

const OWNER: PlayerId = PlayerId { seat: 2 };

fn trigger(game: &mut Game, number: &str, actor: PlayerId) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    assert_eq!(question.actor, actor, "the card owner controls the trigger");
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the printed ability");
    };
    let ability = options.iter().find(|option| option.card == number).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(ability.handle),
    )
    .unwrap()
}

fn reveal(card: &str) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"detective","card":"BP06-118"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"revealed","card":card}]);
    let mut game = game(&initial);
    let revealer = PlayerId { seat: 1 };
    assert!(
        matches!(
            game.play_internal_card("detective", revealer).unwrap(),
            CardPlay::Entered(_)
        ),
        "the reveal producer is played normally"
    );
    resolved(trigger(&mut game, "BP06-118", revealer));
    game.resume_internal_card().unwrap();
    game
}

#[test]
fn esser_targets_an_enemy_when_an_opponent_reveals_it_from_hand() {
    let mut game = reveal("CP05-037");
    let targets = awaiting(trigger(&mut game, "CP05-037", OWNER));
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("expected the formal target");
    };
    let enemy = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP01-037"))
        .unwrap();
    resolved(
        game.answer_internal_card(
            targets.actor,
            targets.token,
            Answer::Targets(vec![enemy.handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["hand"],
        json!(["revealed"]),
        "Esser remains in its owner's hand after the reveal"
    );
    assert_eq!(
        state["objects"]["enemy"]["damage"],
        1_u8,
        "the chosen enemy takes one damage from the reveal trigger"
    );
}

#[test]
fn alchemic_flare_creates_fire_material_when_revealed_from_hand() {
    let mut game = reveal("CP05-048");
    resolved(trigger(&mut game, "CP05-048", OWNER));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["hand"],
        json!(["revealed"]),
        "Alchemic Flare remains in its owner's hand after the reveal"
    );
    let ex = state["P2"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 1, "the owner receives one material");
    assert_eq!(
        state["objects"][ex[0].as_str().unwrap()]["card"],
        "CP05-T09",
        "the reveal trigger creates a Fire material"
    );
}
