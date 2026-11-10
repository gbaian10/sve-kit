#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Only the printed card's declared choices may occur."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"stack","card":"BP03-054"},
        {"id":"rite","card":"BP03-041"},
        {"id":"spell","card":"BP01-063"},
        {"id":"plain","card":"BP01-037"},
        {"id":"below","card":"BP01-037"}
    ]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, expected: &[&str], take: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                let mut cards = domain
                    .members
                    .iter()
                    .map(|m| m.card.as_deref().unwrap())
                    .collect::<Vec<_>>();
                cards.sort_unstable();
                let mut expected = expected.to_vec();
                expected.sort_unstable();
                assert_eq!(cards, expected, "only matching ability holders are offered");
                Answer::Targets(if take {
                    vec![domain.members[0].handle]
                } else {
                    vec![]
                })
            }
            InputDomain::Search(domain) => {
                let cards = domain
                    .cards
                    .iter()
                    .map(|c| c.card.as_str())
                    .collect::<Vec<_>>();
                assert_eq!(cards, expected, "Only matching ability holders qualify for the search");
                Answer::Search(SearchAnswer::Ungrouped(
                    domain
                        .cards
                        .iter()
                        .take(usize::from(take))
                        .map(|c| c.handle)
                        .collect(),
                ))
            }
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|c| c.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(false),
            _ => panic!("unexpected card choice"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn trigger(game: &mut Game, card: &str, expected: &[&str], take: bool) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending ability");
    };
    let chosen = options.iter().find(|o| o.card == card).unwrap();
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(chosen.handle),
        )
        .unwrap();
    finish(game, play, expected, take);
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "The source must evolve before its printed evolution ability can trigger"
    );
}

#[test]
fn bp03_054_takes_earth_rite_but_never_stack_alone_and_reveals_only_the_chosen_card() {
    for alias in ["BP03-054", "BP03-P15", "PR-341"] {
        assert!(selection().current.contains_key(alias));
    }
    let mut game = game(&position("BP03-054"));
    let play = game.play_internal_card("source", actor()).unwrap();
    finish(&mut game, play, &[], true);
    trigger(&mut game, "BP03-054", &["BP03-041", "BP01-063"], true);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["rite"]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["below", "plain", "spell", "stack"])
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(known.iter().any(|c| c["id"] == "rite"));
    assert!(!known.iter().any(|c| c["id"] == "stack"));
}

#[test]
fn bp08_049_may_decline_earth_rite_then_orders_all_four_cards_below_the_unseen_remainder() {
    let mut initial = position("BP08-048");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP08-048"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP08-049"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    trigger(&mut game, "BP08-049", &["BP03-041", "BP01-063"], false);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["below", "plain", "spell", "rite", "stack"])
    );
}

fn riley_game(initial: &Value) -> Game {
    static PREPARED: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        snapshot.push_str(&json!({"number":"filter-base","faces":[{
            "name":"アストラルシャーマン・ライリー","card_class":"ウィッチ","card_type":"フォロワー",
            "traits":[],"cost":"3","power":"1","hp":"3","text":null,"sections":[]
        }]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["filter-base.yaml",json!({"format":1_u8,"kind":"effect_set","cards":{"filter-base":{"status":"complete","abilities":[]}}}).to_string()]));
        repository::select(&snapshot, &keywords, &input).unwrap()
    });
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "riley-filter",
    )
    .unwrap();
    // The real base still needs Earth Rite history; a synthetic base produces only evolution.
    let mut tree = expanded::tree().clone();
    let mut card = tree["cards"]["BP08-038"].clone();
    let mut ability = card["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("fanfare");
    ability["line"] = json!(1_u8);
    ability["evolve"] = json!({"cost":{"pp":1_u8}});
    card["abilities"] = json!([ability]);
    tree["cards"] = json!({"filter-base":card});
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("filter-base")
        .unwrap();
    game.load_internal_card("filter-base", &program).unwrap();
    game
}

#[test]
fn bp14_038_searches_only_earth_rite_followers_with_original_cost_within_max_pp() {
    assert!(selection().current.contains_key("BP14-SL10"));
    let mut initial = position("filter-base");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"filter-base"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP14-038"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"self-copy","card":"filter-base"},
        {"id":"rite","card":"BP03-041"},
        {"id":"expensive","card":"BP01-051"},
        {"id":"spell","card":"BP01-063"}
    ]);
    initial["players"]["P1"]["pp"] = json!({"current":4_u8,"max":4_u8});
    let mut game = riley_game(&initial);
    evolve(&mut game);
    trigger(&mut game, "BP14-038", &["BP03-041"], true);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "rite"]));
    assert_eq!(state["P1"]["pp"]["current"], 3_u8);
}

#[test]
fn bp15_082_revives_only_a_small_nightmare_lastword_and_uses_necrocharge_discount() {
    assert!(selection().current.contains_key("BP15-U05"));
    let mut initial = position("BP15-082");
    let mut grave = vec![
        json!({"id":"lastword","card":"BP03-077"}),
        json!({"id":"grave-plain","card":"BP01-117"}),
        json!({"id":"other-class","card":"BP01-045"}),
        json!({"id":"too-large","card":"BP04-093"}),
    ];
    grave.extend((0_u8..6).map(|i| json!({"id":format!("filler-{i}"),"card":"BP01-171"})));
    initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
    let mut game = game(&initial);
    let play = game.play_internal_card("source", actor()).unwrap();
    finish(&mut game, play, &["BP03-077"], true);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["lastword"]));
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("grave-plain"))
    );
}
