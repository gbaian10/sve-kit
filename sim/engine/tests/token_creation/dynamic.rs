#![expect(
    clippy::indexing_slicing,
    reason = "Synthetic fixture coordinates must fail the test when their shape changes."
)]

use super::*;
use sve_engine::ir::lowering::ExpandedLowerer;
use sve_engine::ir::{
    Action, Control, Generation, Nat, Node, PlayerOrder, PlayerRef, PlayerRelation, Program,
    Selector, WellFormedProgram,
};

fn steps() -> Value {
    json!([
        {"destroy":{"ref":{"who":"all","zone":"field","kind":"follower"},"result":"broken"}},
        {"each_player":{"as":"p","stages":[[
            {"create":{"token":"token","to":"field","by":"p","n":{"count":{
                "in":{"objects_of":"broken","view":"before"},"as":"victim",
                "where":{"cmp":{"left":{"player":{"master_of":"victim"}},"op":"eq","right":{"player":"p"}}}
            }}}}
        ]]}}
    ])
}

fn selected(steps: Value, ward: bool) -> (String, String, Value) {
    let (snapshot, _, mut input) = selection(1);
    input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
        ["abilities"][0]["spell"]["do"] = steps;
    let (token, author) = author(
        "token",
        "22222222222222222222222222222222",
        "フォロワー・トークン",
        if ward {
            vec![json!({"keywords":["ward"]})]
        } else {
            vec![]
        },
    );
    let snapshot = format!("{}\n{token}\n", snapshot.lines().next().unwrap());
    input["authors"][1] = author;
    (
        snapshot,
        include_str!("../../../../authored/rules/keywords.yaml").into(),
        input,
    )
}

fn position() -> Value {
    let mut position = initial(0);
    position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"mine","card":"token","owner":"P2"}]);
    position["players"]["P2"]["zones"]["field"] = json!([
        {"id":"theirs-1","card":"token","owner":"P1"},
        {"id":"theirs-2","card":"token","owner":"P1"}
    ]);
    position
}

fn game(steps: Value, ward: bool, position: &Value) -> Game {
    let (snapshot, keywords, input) = selected(steps, ward);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    Game::from_selected_effects(
        selected,
        position,
        &Value::Null,
        &Value::Null,
        "dynamic-creation",
    )
    .unwrap()
}

#[test]
fn each_recipient_answers_its_own_ward_entry_and_resume_does_not_recreate() {
    let mut game = game(steps(), true, &position());
    let mut play = game
        .play_internal_card("creator", PlayerId { seat: 1 })
        .unwrap();
    for seat in [1, 2, 2] {
        let CardPlay::Awaiting(question) = play else {
            panic!("Ward entry for each arriving token");
        };
        assert_eq!(question.actor, PlayerId { seat });
        assert!(matches!(question.domain, InputDomain::WardEntry { .. }));
        let pending = game.projection(View::Referee).unwrap();
        assert_eq!(
            pending["P1"]["field"],
            json!([]),
            "all arrivals wait for both players' entry choices"
        );
        assert_eq!(pending["P2"]["field"], json!([]));
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            PlayerId { seat: 3 - seat },
            question.token,
            Answer::WardEntry(true),
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        play = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(seat == 2))
            .unwrap();
    }
    assert!(matches!(play, CardPlay::Resolved(_)));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 1);
    let opponents = view["P2"]["field"].as_array().unwrap();
    assert_eq!(opponents.len(), 2);
    assert!(
        opponents
            .iter()
            .all(|id| view["objects"][id.as_str().unwrap()]["acted"] == true)
    );
}

pub(super) fn raw_body() -> Program {
    let (_, _, input) = selected(steps(), false);
    let lowered = ExpandedLowerer::new()
        .unwrap()
        .lower_current(&input["authors"][0]["input"])
        .unwrap();
    lowered.program.card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap()
        .clone()
}

fn raw_game() -> Game {
    let (snapshot, keywords, mut input) = selected(steps(), false);
    input["authors"].as_array_mut().unwrap().remove(0);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    Game::from_selected_effects(
        selected,
        &position(),
        &Value::Null,
        &Value::Null,
        "raw-token-projection",
    )
    .unwrap()
}

fn banish_steps() -> Value {
    json!([
        {"banish":{"ref":{"any_of":[{"who":"all","zone":"field"},{"who":"all","zone":"ex"}]}}},
        {"each_player":{"as":"p","stages":[[{"banish":{"top":{"who":"p","n":2_u8}}}]]}}
    ])
}

#[test]
fn raw_player_banish_rejects_unproved_private_authority_without_mutation() {
    use sve_engine::game::internal::Context;
    let (_, _, input) = selected(banish_steps(), false);
    let lowered = ExpandedLowerer::new()
        .unwrap()
        .lower_current(&input["authors"][0]["input"])
        .unwrap();
    let base = lowered.program.card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap();
    for variant in 0_u8..5 {
        let mut body = base.clone();
        let Node::Control {
            control:
                Control::EachPlayer {
                    players,
                    stages,
                    choice_order,
                    ..
                },
        } = &mut body.steps[1].node
        else {
            panic!("player stage");
        };
        let Node::Instruction { instruction } = &mut stages[0].body.steps[0].node else {
            panic!("banish instruction");
        };
        match variant {
            0 => *choice_order = PlayerOrder::FirstPlayerFirst,
            1 => players.relation = PlayerRelation::Opponents,
            2 => players.cardinality.maximum = Some(Nat(1)),
            3 => instruction.actor = PlayerRef::Controller {},
            _ => {
                let Action::Banish {
                    objects: Selector::Ordered { zone, .. },
                } = &mut instruction.action
                else {
                    panic!("deck slice");
                };
                zone.player = PlayerRef::Controller {};
            }
        }
        let span = body.steps[0].source.clone();
        let checked = WellFormedProgram::check(body).unwrap();
        let mut game = raw_game();
        let before = format!("{game:?}");
        game.execute_internal(
            &checked,
            &span,
            Context {
                source: "creator".into(),
                generation: Generation(0),
                controller: PlayerId { seat: 1 },
            },
        )
        .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "unsupported raw stage must reject before mutation"
        );
    }
}
