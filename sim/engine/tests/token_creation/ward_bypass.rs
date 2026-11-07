#![expect(
    clippy::indexing_slicing,
    reason = "Synthetic fixture coordinates must fail the test when their shape changes."
)]
use super::*;

fn abilities() -> Vec<Value> {
    vec![
        json!({"keywords":["storm"]}),
        json!({"static":{"effects":[{"ignore_ward":{"on":"this"}}]}}),
    ]
}

fn inputs(abilities: Vec<Value>) -> (String, String, Value) {
    let (source, source_author) = author(
        "creator",
        "11111111111111111111111111111111",
        "フォロワー",
        abilities,
    );
    let (guard, guard_author) = author(
        "guard",
        "22222222222222222222222222222222",
        "フォロワー",
        vec![json!({"keywords":["ward"]})],
    );
    let (ordinary, ordinary_author) = author(
        "ordinary",
        "33333333333333333333333333333333",
        "フォロワー",
        vec![],
    );
    (
        format!("{source}\n{guard}\n{ordinary}\n"),
        include_str!("../../../../authored/rules/keywords.yaml").into(),
        json!({"documents":[],"authors":[source_author,guard_author,ordinary_author]}),
    )
}

fn position() -> Value {
    let mut initial = initial(0);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"creator"}, {"id":"ally","card":"ordinary"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"guard","card":"guard","state":{"acted":true}},
        {"id":"other","card":"ordinary","state":{"acted":true}}
    ]);
    initial
}

fn game(abilities: Vec<Value>, initial: &Value) -> Game {
    let (snapshot, keywords, input) = inputs(abilities);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    Game::from_selected_effects(selected, initial, &Value::Null, &Value::Null, "ward-bypass")
        .unwrap()
}

#[test]
fn printed_ward_bypass_skips_only_ward_and_is_lost_with_the_ability() {
    for scenario in 0_u8..8 {
        let mut initial = position();
        let mut body = abilities();
        let mut attacker = "source";
        let mut target = "P2.leader";
        match scenario {
            1 => target = "other",
            2 => initial["players"]["P1"]["zones"]["field"][0]["state"]["acted"] = json!(true),
            3 => {
                initial["players"]["P1"]["zones"]["field"][0]["state"]["entered_this_turn"] =
                    json!(true);
                body.remove(0);
            }
            4 => initial["players"]["P1"]["zones"]["field"][0]["state"]["silenced"] = json!(true),
            5 => attacker = "ally",
            6 => {
                target = "other";
                initial["players"]["P2"]["zones"]["field"][1]["state"]["acted"] = json!(false);
            }
            7 => {
                body.push(json!({"static":{"effects":[{"cannot":{"what":"attack","on":"this"}}]}}));
            }
            _ => {}
        }
        let mut game = game(body, &initial);
        let result = game
            .internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
            .unwrap();
        assert_eq!(
            result.outcome,
            if scenario < 2 {
                "resolved"
            } else {
                "cannot-attack"
            },
            "scenario {scenario}"
        );
    }
}

#[test]
fn a_public_condition_controls_the_printed_bypass() {
    for allied in [false, true] {
        let mut initial = position();
        if !allied {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .pop();
        }
        let mut body = abilities();
        body[1]["static"]["while"] =
            json!({"cmp":{"left":{"count":{"who":"self","zone":"field"}},"op":"ge","right":2_u8}});
        let mut game = game(body, &initial);
        let result = game
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        assert_eq!(
            result.outcome,
            if allied { "resolved" } else { "cannot-attack" }
        );
    }
}

#[test]
fn ward_bypass_does_not_open_other_subjects_zones_or_installation() {
    let mut variants = Vec::new();
    for on in [json!({"who":"all","zone":"field"}), json!("ally")] {
        let mut body = abilities();
        body[1]["static"]["effects"][0]["ignore_ward"]["on"] = on;
        variants.push(body);
    }
    let mut hand = abilities();
    hand[1]["static"]["zone"] = json!("hand");
    variants.push(hand);
    let mut installed = abilities();
    installed[1] = json!({"fanfare":{"do":[{"apply":{"on":"this","effect":{"ignore_ward":{"on":"affected"}},"until":"turn"}}]}});
    variants.push(installed);
    for body in variants {
        let (snapshot, keywords, input) = inputs(body);
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
    }
}
