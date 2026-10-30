use super::*;

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, class, kind, cost) in (0_u8..9)
            .map(|cost| {
                (
                    format!("distinct-{cost}"),
                    format!("name-{cost}"),
                    "エルフ",
                    "スペル",
                    cost,
                )
            })
            .chain([
                (
                    "distinct-alias".into(),
                    "name-0".into(),
                    "エルフ",
                    "スペル",
                    0,
                ),
                (
                    "distinct-same-cost".into(),
                    "another-name".into(),
                    "エルフ",
                    "スペル",
                    1,
                ),
                (
                    "distinct-class".into(),
                    "other-class".into(),
                    "ウィッチ",
                    "スペル",
                    9,
                ),
                (
                    "distinct-kind".into(),
                    "other-kind".into(),
                    "エルフ",
                    "フォロワー",
                    10,
                ),
            ])
        {
            let stat = if kind == "フォロワー" { "1" } else { "-" };
            snapshot.push_str(
                &json!({"number":number,"faces":[{
                    "name":name,"card_class":class,"card_type":kind,"traits":[],
                    "cost":cost.to_string(),"power":stat,"hp":stat,"text":null,"sections":[]
                }]})
                .to_string(),
            );
            snapshot.push('\n');
            let file = format!("{number}.yaml");
            let document = json!({"format":1_u8,"kind":"effect_set","cards":{
                number:{"status":"complete","abilities":[]}
            }});
            input["documents"]
                .as_array_mut()
                .unwrap()
                .push(json!([file, document.to_string()]));
        }
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap()
    })
}

pub(super) fn fixture(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "distinct-count",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in (0_u8..9).map(|cost| format!("distinct-{cost}")).chain([
        "distinct-alias".into(),
        "distinct-same-cost".into(),
        "distinct-class".into(),
        "distinct-kind".into(),
    ]) {
        game.load_internal_card(&number, &empty).unwrap();
    }
    game
}

pub(super) fn position(card: &str, count: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"top","card":"BP01-037"},{"id":"next","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!((0..count).map(|n|json!({"id":format!("grave-{n}"),"card":format!("distinct-{n}"),"state":{"cost":0_u8}})).collect::<Vec<_>>());
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":"distinct-8"}]);
    initial
}

#[test]
fn bp09_007_distinct_names_filter_class_kind_zone_and_reprints_before_spell_cleanup() {
    for printing in ["BP09-007", "BP09-P03"] {
        for count in [4_usize, 5] {
            let mut initial = position(printing, count);
            initial["players"]["P1"]["zones"]["cemetery"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"alias","card":"distinct-alias"}),
                    json!({"id":"wrong-class","card":"distinct-class"}),
                    json!({"id":"wrong-kind","card":"distinct-kind"}),
                ]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":"distinct-8"}]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP04-117"}]);
            let mut game = fixture(&initial);
            resolved(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count == 5 { 5_u8 } else { 3_u8 }
            );
            assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count == 5 { 18_u8 } else { 20_u8 },
                "R-0007 keeps the following leader damage inside the condition"
            );
            assert_eq!(
                state["P1"]["cemetery"].as_array().unwrap().last(),
                Some(&json!("source"))
            );
        }
    }
}

#[test]
fn bp09_011_draws_then_heals_only_with_five_distinct_elf_spell_names() {
    for printing in ["BP09-011", "BP09-P04"] {
        for count in [4_usize, 5] {
            let mut game = fixture(&position(printing, count));
            resolved(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["top"]));
            assert_eq!(
                state["P1"]["leader"]["life"],
                if count == 5 { 12_u8 } else { 10_u8 }
            );
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        }
    }
}

#[test]
fn bp15_056_distinct_original_costs_discount_before_payment_and_preserve_storm_and_fanfare() {
    for count in [5_usize, 6] {
        let mut initial = position("BP15-056", count);
        initial["players"]["P1"]["zones"]["cemetery"] = json!((1..=count).map(|n|
            json!({"id":format!("grave-{n}"),"card":format!("distinct-{n}"),"state":{"cost":0_u8}})
        ).collect::<Vec<_>>());
        initial["players"]["P1"]["zones"]["cemetery"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"same-cost","card":"distinct-same-cost"}));
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"second","card":"BP04-117"}));
        let mut game = fixture(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("fanfare")
        };
        let CardPlay::Awaiting(targets) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("fanfare targets")
        };
        let InputDomain::Targets(domain) = &targets.domain else {
            panic!("targets")
        };
        assert_eq!(domain.members.len(), 2);
        resolved(
            game.answer_internal_card(
                targets.actor,
                targets.token,
                Answer::Targets(domain.members.iter().map(|m| m.handle).collect()),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if count == 6 { 8_u8 } else { 2_u8 }
        );
        for id in ["enemy", "second"] {
            assert_eq!(state["objects"][id]["damage"], 3_u8);
        }
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
    }
}
