#![expect(
    clippy::panic,
    reason = "A different decision invalidates the printed-card scenario."
)]

use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn answer(game: &mut Game, question: &DecisionView, response: Answer) -> CardPlay {
    game.answer_internal_card(question.actor, question.token, response)
        .unwrap()
}

fn first_trigger(game: &mut Game, number: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected a pending ability");
    };
    let option = options.iter().find(|option| option.card == number).unwrap();
    answer(game, &question, Answer::Pending(option.handle))
}

fn answer_single_target(game: &mut Game, play: CardPlay) -> CardPlay {
    let question = awaiting(play);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected a formal target");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the test position has one eligible target"
    );
    answer(
        game,
        &question,
        Answer::Targets(vec![domain.members[0].handle]),
    )
}

fn created_in(view: &Value, zone: &str, card: &str) -> bool {
    view["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .any(|id| view["objects"][id.as_str().unwrap()]["card"] == card)
}

// Supporting cards keep the real effects independent of the earlier authoring batch.
fn prepared_game(initial: &Value) -> Game {
    let (snapshot, keywords, mut input) = inputs().clone();
    let form = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|record| record["number"] == "CP05-050")
        .unwrap();
    let base = json!({
        "number":"lunalu-base",
        "faces":[{
            "name":form["faces"][0]["name"],
            "card_class":form["faces"][0]["card_class"],
            "card_type":"フォロワー","cost":"1","power":"1","hp":"1",
            "traits":[],"text":null,"sections":[]
        }]
    });
    let document = json!({
        "format":1_u8,"kind":"effect_set",
        "cards":{"lunalu-base":{"status":"complete","abilities":[]}}
    });
    input["documents"]
        .as_array_mut()
        .unwrap()
        .push(json!(["lunalu-base.yaml", document.to_string()]));

    let (mut earth, earth_author) = current_fixture::author(
        "earth-form",
        "55777777777777777777777777777777",
        "フォロワー・エボルヴ",
        vec![],
    );
    earth["faces"][0]["traits"] = json!(["土属性"]);
    let (mut ally, ally_author) = current_fixture::author(
        "matching-ally",
        "55888888888888888888888888888888",
        "フォロワー",
        vec![],
    );
    let (mut copy, copy_author) = current_fixture::author(
        "deck-copy",
        "55999999999999999999999999999999",
        "フォロワー",
        vec![],
    );
    for record in [&mut ally, &mut copy] {
        record["faces"][0]["name"] = json!("Matching follower");
        record["faces"][0]["traits"] = json!(["グラブル"]);
    }
    input["authors"]
        .as_array_mut()
        .unwrap()
        .extend([earth_author, ally_author, copy_author]);
    let mut snapshot = snapshot.trim_end().to_owned();
    for record in [base, earth, ally, copy] {
        snapshot.push('\n');
        snapshot.push_str(&record.to_string());
    }
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({"lunalu-base":tree["cards"]["synthetic-evolution"]});
    let programs = lowerer().lower(&tree).unwrap();
    let mut game = game_from_selection(initial, &snapshot, &keywords, &input);
    game.load_internal_card("lunalu-base", &programs["lunalu-base"])
        .unwrap();
    game
}

#[test]
fn arulumaya_creates_earth_material_heals_and_flips_one_earth_form() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(18_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-045"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"earth","card":"earth-form"},
        {"id":"ordinary","card":"BP04-043"},
        {"id":"open","card":"earth-form","face_up":true}
    ]);
    let mut game = prepared_game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PLAYER).unwrap(),
        CardPlay::Entered(_)
    ));
    let question = awaiting(first_trigger(&mut game, "CP05-045"));
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected a private flip choice");
    };
    assert_eq!(domain.cardinality.minimum, Nat(0), "the flip is optional");
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "only one evolution can flip"
    );
    assert_eq!(
        domain.members.len(),
        1,
        "only the face-down earth form is a flip candidate"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent cannot see the face-down choice"
    );
    resolved(answer(
        &mut game,
        &question,
        Answer::Targets(vec![domain.members[0].handle]),
    ));

    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert!(created_in(&state, "ex", "CP05-T11"));
    assert_eq!(
        state["P1"]["evolve_deck"],
        json!(["earth", "ordinary", "open"])
    );
    assert_eq!(state["objects"]["earth"]["face_up"], true);
    assert_eq!(state["objects"]["ordinary"]["face_up"], false);
}

#[test]
fn evolved_lunalu_uses_the_formal_targets_name_and_banishes_only_the_entered_copy() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"lunalu-base"},{"id":"ally","card":"matching-ally"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-050"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"other_granblue","card":"BP01-037"},{"id":"copy","card":"deck-copy"}
    ]);
    let mut game = prepared_game(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    let play = first_trigger(&mut game, "CP05-050");
    let search = awaiting(answer_single_target(&mut game, play));
    let InputDomain::Search(domain) = &search.domain else {
        panic!("expected same-name search");
    };
    assert_eq!(
        domain.cards.len(),
        1,
        "only the matching name appears in the search"
    );
    assert_eq!(domain.cards[0].card, "deck-copy");
    resolved(answer(
        &mut game,
        &search,
        Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
    ));
    game.resume_internal_card().unwrap();
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["P1"]["field"], json!(["source", "ally", "copy"]));
    assert_eq!(entered["P1"]["deck"], json!(["other_granblue"]));

    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("expected the end-phase granted trigger");
    };
    assert_eq!(options.len(), 1, "only the grant is pending at end phase");
    resolved(answer(
        &mut game,
        &pending,
        Answer::Pending(options[0].handle),
    ));
    let ended = game.projection(View::Referee).unwrap();
    assert_eq!(ended["P1"]["field"], json!(["source", "ally"]));
    assert_eq!(ended["P1"]["banish"], json!(["copy"]));
}
