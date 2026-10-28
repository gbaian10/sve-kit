#![expect(
    clippy::panic,
    reason = "Unexpected decisions indicate a broken game fixture."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-first"),"card":"BP01-037"},
            {"id":format!("{seat}-second"),"card":"BP01-037"},
            {"id":format!("{seat}-third"),"card":"BP01-037"}
        ]);
    }
    initial
}

fn field_position(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(1)])
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("expected target decision");
            };
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            )
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game) {
    let result = game.play_internal_card("source", actor()).unwrap();
    finish(game, result);
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected pending trigger");
    };
    assert_eq!(options.len(), 1, "one card trigger is pending");
    let result = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, result);
}

fn pass_quick(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    let result = game
        .answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    finish(game, result);
}

fn evolve(card: &str, form: &str, cost: u8) {
    let mut initial = field_position(card);
    initial["players"]["P1"]["pp"]["current"] = json!(cost);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut game = game(&initial);
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the exact printed cost permits evolution"
    );
    let event = step
        .events
        .iter()
        .find(|event| event["kind"] == "進化")
        .unwrap();
    assert_eq!(event["object"], "source", "the real base evolves");
    assert_eq!(
        event["with"], "form",
        "the matching evolution resource is used"
    );
}

fn ward(card: &str) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"ward","card":card,"state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
        )
        .unwrap()
        .outcome,
        "cannot-attack",
        "rested ward protects the leader"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"ward"}))
            .unwrap()
            .outcome,
        "resolved",
        "the ward remains attackable"
    );
}

fn departed(card: &str) -> Game {
    let mut game = with_producer(
        &field_position(card),
        Action::Destroy {
            objects: subjects(),
        },
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    game
}

#[test]
fn bp07_082_fanfare_damages_only_its_controller() {
    let mut game = game(&position("BP07-082"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 17_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
}

#[test]
fn bp07_115_lastword_damages_only_opposing_leader() {
    let game = departed("BP07-115");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}

#[test]
fn bp07_065_fanfare_damage_requires_awakening() {
    for max in [6_u8, 7] {
        let mut initial = position("BP07-065");
        initial["players"]["P1"]["pp"] = json!({"current":max,"max":max});
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        play(&mut game);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if max == 7 { 2_u8 } else { 0_u8 }
        );
    }
}

#[test]
fn bp07_105_ward_and_lastword_preserve_the_controller() {
    ward("BP07-105");
    let game = departed("BP07-105");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        22_u8
    );
}

#[test]
fn bp07_114_fanfare_heals_three() {
    let mut game = game(&position("BP07-114"));
    play(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        23_u8
    );
}

#[test]
fn bp07_025_bane_destroys_a_larger_defender() {
    let mut initial = field_position("BP07-025");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "the follower attacks a rested defender"
    );
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}

#[test]
fn bp07_031_evolves_for_one_pp() {
    evolve("BP07-031", "BP07-032", 1);
}

#[test]
fn bp07_048_evolves_for_one_pp() {
    evolve("BP07-048", "BP07-049", 1);
}

#[test]
fn bp07_098_evolves_for_one_pp() {
    evolve("BP07-098", "BP07-099", 1);
}

#[test]
fn bp07_073_fanfare_mills_only_the_top_card() {
    let mut game = game(&position("BP07-073"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["P1-first"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
    assert_eq!(
        state["P2"]["deck"],
        json!(["P2-first", "P2-second", "P2-third"])
    );
}
