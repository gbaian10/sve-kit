#![expect(
    clippy::panic,
    reason = "Unexpected exchange questions invalidate the printed route protocol."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn prepared(with_tree: bool) -> &'static SelectedEffects {
    static BASE: OnceLock<SelectedEffects> = OnceLock::new();
    static TREE: OnceLock<SelectedEffects> = OnceLock::new();
    let cache = if with_tree { &TREE } else { &BASE };
    cache.get_or_init(|| {
        let (mut snapshot,keywords,mut input)=inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| matches!(author["input"]["card_no"].as_str(),Some("BP01-103"|"BP01-125"|"BP04-T01"|"CP02-T01"|"BP03-T02")) || (with_tree&&author["input"]["card_no"]=="BP07-T03"));
        snapshot.push_str(&json!({"number":"route-neutral","faces":[{"name":"route-neutral","card_class":"ニュートラル","card_type":"フォロワー","traits":[],"cost":"1","power":"1","hp":"20","text":null,"sections":[]}]}).to_string());snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["route-neutral.yaml",json!({"format":1_u8,"kind":"effect_set","cards":{"route-neutral":{"status":"complete","abilities":[]}}}).to_string()]));
        repository::selection_with_rules_from(&snapshot,&keywords,&input)
    })
}

fn board(initial: &Value, with_tree: bool) -> Game {
    let mut game = Game::from_selected_effects(
        prepared(with_tree).clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "skeleton-route",
    )
    .unwrap();
    game.load_internal_card(
        "route-neutral",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game
}

fn position(printing: &str, zone: &str, field: u8, ex: u8, pp: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..field)
            .map(|n| json!({"id":format!("viper{n}"),"card":"BP04-T01","state":{"acted":false}}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..ex)
            .map(|n| json!({"id":format!("ex{n}"),"card":"BP01-125"}))
            .collect::<Vec<_>>()
    );
    let zones = initial["players"]["P1"]["zones"].as_object_mut().unwrap();
    zones
        .entry(zone.to_owned())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"source","card":printing}));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"foreign","card":"BP04-T01"}]);
    initial
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("exchange question")
    };
    q
}

fn start(game: &mut Game) -> DecisionView {
    let q = awaiting(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(
        matches!(q.domain, InputDomain::OptionalCost),
        "acceptance remains a separate binary decision"
    );
    q
}

fn choose(game: &mut Game, q: &DecisionView, index: u64) -> DecisionView {
    let route = awaiting(
        game.answer_internal_card(q.actor, q.token, Answer::OptionalCost(true))
            .unwrap(),
    );
    let InputDomain::Integer(bounds) = &route.domain else {
        panic!("payment route")
    };
    assert!(
        index >= bounds.minimum.0 && bounds.maximum.is_some_and(|max| index <= max.0),
        "only an affordable entire route is offered"
    );
    let before = game.projection(View::Referee).unwrap();
    let mut unavailable = vec![2];
    if bounds.maximum == Some(bounds.minimum) {
        unavailable.push(u64::from(index == 0));
    } else {
        assert_eq!(
            (bounds.minimum, bounds.maximum),
            (Nat(0), Some(Nat(1))),
            "four legal materials in each zone offer both complete routes"
        );
    }
    for answer in unavailable {
        game.answer_internal_card(route.actor, route.token, Answer::Integer(Nat(answer)))
            .unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "an unavailable route preserves the pending play"
        );
    }
    awaiting(
        game.answer_internal_card(route.actor, route.token, Answer::Integer(Nat(index)))
            .unwrap(),
    )
}

fn pay(game: &mut Game, q: &DecisionView, number: &str) {
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("one route's materials")
    };
    assert_eq!(
        domain.members.len(),
        4,
        "the material question only exposes the selected route"
    );
    assert!(
        domain
            .members
            .iter()
            .all(|member| member.card.as_deref() == Some(number)),
        "the other zone cannot supply a mixed payment"
    );
    let before = game.projection(View::Referee).unwrap();
    for handles in [vec![0, 1, 2, 99], vec![0, 1, 1, 2]] {
        game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(handles))
            .unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "foreign or duplicate materials preserve the question"
        );
    }
    assert!(
        matches!(
            game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![0, 1, 2, 3]))
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the source enters only after full material and PP payment"
    );
}

#[test]
fn skeleton_king_both_printings_pay_four_standing_tokens_before_entering_a_full_field() {
    for printing in ["BP01-103", "BP01-SL15"] {
        let mut initial = position(printing, "hand", 4, 4, 0);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"neutral","card":"route-neutral"}));
        let mut game = board(&initial, false);
        let q = start(&mut game);
        let materials = choose(&mut game, &q, 0);
        pay(&mut game, &materials, "BP04-T01");
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["field"],
            json!(["neutral", "source"]),
            "Q215 pays the four materials before the full-field entry"
        );
        assert_eq!(
            after["P1"]["pp"]["current"], 0_u8,
            "the printed nine is reduced by nine"
        );
        assert_eq!(
            after["P1"]["ex"],
            json!(["ex0", "ex1", "ex2", "ex3"]),
            "the unchosen EX route is untouched"
        );
        assert_eq!(
            after["P1"]["cemetery"],
            json!([]),
            "Q271 tokens leave after reaching the cemetery by the existing token rule"
        );
        assert_eq!(
            after["P2"]["field"],
            json!(["foreign"]),
            "the opponent's Nightmare does not pay"
        );
    }
}

#[test]
fn skeleton_king_both_printings_banish_only_the_other_four_ex_materials() {
    for printing in ["BP01-103", "BP01-SL15"] {
        let initial = position(printing, "ex", 0, 4, 0);
        let mut game = board(&initial, false);
        let q = start(&mut game);
        let materials = choose(&mut game, &q, 1);
        pay(&mut game, &materials, "BP01-125");
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["field"],
            json!(["source"]),
            "the played source leaves EX before material selection"
        );
        assert_eq!(
            after["P1"]["banish"],
            json!(["ex0", "ex1", "ex2", "ex3"]),
            "only the selected EX route is banished"
        );
        assert_eq!(
            after["P1"]["pp"]["current"], 0_u8,
            "the EX route shares the same reduction"
        );
    }
}

#[test]
fn skeleton_king_rejects_mixed_partial_routes_acted_materials_and_counting_the_ex_source() {
    for printing in ["BP01-103", "BP01-SL15"] {
        for variant in ["mixed", "acted", "ex-source"] {
            let mut initial = match variant {
                "mixed" => position(printing, "hand", 2, 2, 0),
                "acted" => position(printing, "hand", 4, 0, 0),
                _ => position(printing, "ex", 0, 3, 0),
            };
            if variant == "acted" {
                initial["players"]["P1"]["zones"]["field"][3]["state"]["acted"] = json!(true);
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"neutral","card":"route-neutral"}));
            }
            let mut game = board(&initial, false);
            let before = game.projection(View::Referee).unwrap();
            let q = start(&mut game);
            game.answer_internal_card(q.actor, q.token, Answer::OptionalCost(true))
                .unwrap_err();
            assert_eq!(
                game.projection(View::Referee).unwrap(),
                before,
                "no whole route can be paid in {variant}; Q216 forbids combining two partial routes"
            );
        }
    }
}

#[test]
fn skeleton_king_can_decline_at_nine_pp_and_keeps_the_existing_trigger_guard() {
    let field_only = position("BP01-103", "hand", 4, 0, 0);
    let mut field_game = board(&field_only, false);
    let field_question = start(&mut field_game);
    let materials = choose(&mut field_game, &field_question, 0);
    pay(&mut field_game, &materials, "BP04-T01");
    let initial = position("BP01-103", "hand", 4, 0, 9);
    let mut game = board(&initial, false);
    let q = start(&mut game);
    assert!(
        matches!(
            game.answer_internal_card(q.actor, q.token, Answer::OptionalCost(false))
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "declining pays the printed cost without a route question"
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["pp"]["current"], 0_u8,
        "declining pays all nine PP"
    );
    assert_eq!(
        after["P1"]["field"].as_array().unwrap().len(),
        5,
        "all four unspent Vipers remain"
    );
    let guarded = position("BP01-103", "hand", 4, 0, 0);
    let mut guarded_game = board(&guarded, true);
    let before = guarded_game.projection(View::Referee).unwrap();
    let error = guarded_game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("exchange payment or material triggers"),
        "unsupported payment observers keep the existing contract: {error}"
    );
    assert_eq!(
        guarded_game.projection(View::Referee).unwrap(),
        before,
        "the existing unsupported trigger scope fails before mutation"
    );
}
