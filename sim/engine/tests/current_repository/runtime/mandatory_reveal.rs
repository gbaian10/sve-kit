use super::*;

fn prepared() -> &'static SelectedEffects {
    static PREPARED: OnceLock<SelectedEffects> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| matches!(author["input"]["card_no"].as_str(), Some("DSD01a-014" | "BP17-054" | "BP06-052" | "DSD01a-T01" | "CP02-T01" | "BP03-T02")));
        for (number, traits) in [("reveal-academy",json!(["学院"])),("reveal-golem",json!(["ゴーレム"])),("reveal-neutral",json!([]))] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","traits":traits,"cost":"1","power":"1","hp":"20","text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot,&keywords,&input)
    })
}

fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "mandatory-reveal",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for no in ["reveal-academy", "reveal-golem", "reveal-neutral"] {
        game.load_internal_card(no, &blank).unwrap();
    }
    game
}

#[expect(
    clippy::panic,
    reason = "The real spell must ask for its mandatory hand materials."
)]
fn pay(game: &mut Game, handles: Vec<u64>) {
    let CardPlay::Awaiting(question) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("mandatory materials");
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("cost materials");
    };
    assert!(
        domain
            .members
            .iter()
            .all(|m| m.card.as_deref() == Some("reveal-academy")),
        "only the other Academy cards qualify"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent never sees unpaid candidates"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(handles),
        )
        .unwrap(),
    );
}

#[test]
fn dsd01a_014_both_printings_require_two_other_academy_cards_then_draw_and_create() {
    for printing in ["DSD01a-014", "DSD01a-P06"] {
        for materials in [1_u8, 2_u8] {
            let mut initial = draw_position();
            let mut hand = vec![
                json!({"id":"source","card":printing}),
                json!({"id":"wrong","card":"reveal-neutral"}),
            ];
            hand.extend(
                (0_u8..materials).map(|i| json!({"id":format!("h{i}"),"card":"reveal-academy"})),
            );
            initial["players"]["P1"]["zones"]["hand"] = json!(hand);
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"drawn","card":"reveal-neutral"}]);
            let mut board = game(&initial);
            if materials == 1 {
                let before = board.projection(View::Referee).unwrap();
                board
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap_err();
                assert_eq!(
                    board.projection(View::Referee).unwrap(),
                    before,
                    "Q1242: the source cannot be one of the two materials"
                );
                continue;
            }
            pay(&mut board, vec![0, 1]);
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["hand"],
                json!(["wrong", "h0", "h1", "drawn"]),
                "revelation leaves the selected cards in hand"
            );
            assert_eq!(
                view["P1"]["pp"]["current"], 9_u8,
                "the printed one PP is paid"
            );
            let ex = view["P1"]["ex"].as_array().unwrap();
            assert_eq!(ex.len(), 1, "the full spell creates its defined token");
            let token = ex[0].as_str().unwrap();
            assert_eq!(
                view["objects"][token]["card"], "DSD01a-T01",
                "the existing token definition is used"
            );
            for id in ["h0", "h1", "drawn"] {
                assert!(
                    board.projection(View::P2).unwrap()["objects"][id].is_null(),
                    "no hidden hand identity survives resolution"
                );
            }
        }
    }
}

#[test]
fn bp17_054_both_printings_reveal_the_same_academy_card_then_bottom_it_and_draw_two() {
    for printing in ["BP17-054", "BP17-P13"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing},{"id":"h0","card":"reveal-academy"},{"id":"h1","card":"reveal-academy"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"d0","card":"reveal-neutral"},{"id":"d1","card":"reveal-neutral"}]);
        let mut board = game(&initial);
        pay(&mut board, vec![1]);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["deck"],
            json!(["h1"]),
            "the selected card goes below both original top cards"
        );
        assert_eq!(
            view["P1"]["hand"],
            json!(["h0", "d0", "d1"]),
            "the other Academy card stays and both original cards are drawn"
        );
        assert_eq!(view["P1"]["pp"]["current"], 9_u8, "one PP is paid once");
        assert!(
            board.projection(View::P2).unwrap()["objects"]["h1"].is_null(),
            "public reveal does not retain a trackable hidden deck identity"
        );
    }
}

#[test]
fn bp06_052_can_decline_its_cost_and_only_paid_damage_hits_all_enemies() {
    for (present, accepted) in [(false, false), (true, false), (true, true)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP06-052"}]);
        initial["players"]["P1"]["zones"]["field"] = if present {
            json!([{"id":"fuel","card":"reveal-golem"},{"id":"own","card":"reveal-neutral"}])
        } else {
            json!([{"id":"own","card":"reveal-neutral"}])
        };
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy0","card":"reveal-golem"},{"id":"enemy1","card":"reveal-neutral"}]);
        let mut board = game(&initial);
        let mut play = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let mut asked_optional = false;
        while let CardPlay::Awaiting(question) = play {
            let answer = if matches!(question.domain, InputDomain::OptionalCost) {
                asked_optional = true;
                Answer::OptionalCost(accepted)
            } else {
                let InputDomain::CostMaterials(domain) = &question.domain else {
                    panic!("optional Golem cost");
                };
                assert!(
                    accepted && present,
                    "only accepted payable cost selects materials"
                );
                assert_eq!(
                    domain.members.len(),
                    1,
                    "CR10.4.3: only one's own Golem can pay"
                );
                Answer::CostMaterials(vec![0])
            };
            play = board
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        if present {
            assert!(
                asked_optional,
                "the actor actually chooses whether to pay the optional cost"
            );
        }
        let result = resolved(play);
        let view = board.projection(View::Referee).unwrap();
        let damage = if accepted { 3_u8 } else { 0_u8 };
        assert_eq!(
            view["P2"]["leader"]["life"],
            20_u8 - damage,
            "only paid cost deals damage"
        );
        for id in ["enemy0", "enemy1"] {
            assert_eq!(
                view["objects"][id]["damage"], damage,
                "all enemy followers receive the same batch"
            );
        }
        assert_eq!(
            view["objects"]["own"]["damage"], 0_u8,
            "own follower is excluded"
        );
        assert_eq!(
            view["P1"]["leader"]["life"], 20_u8,
            "own leader is excluded"
        );
        assert_eq!(
            view["P1"]["pp"]["current"], 8_u8,
            "the spell is playable and pays PP even without materials"
        );
        if present {
            let zone = if accepted { "cemetery" } else { "field" };
            assert!(
                view["P1"][zone]
                    .as_array()
                    .unwrap()
                    .contains(&json!("fuel")),
                "only payment moves the Golem"
            );
        }
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Discard(_))),
            "field costs are not discards"
        );
    }
}
