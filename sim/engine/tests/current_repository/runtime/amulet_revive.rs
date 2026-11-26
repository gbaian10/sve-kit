use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn position(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-037"},{"id":"remaining","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"hp":12_u8,"max_hp":12_u8}}
    ]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                assert_eq!(domain.members.len(), 1, "only the intended target matches");
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::CostMaterials(domain) => {
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::Search(domain) => {
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
            }
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[0]]),
            _ => panic!("unexpected printed decision: {:?}", question.domain),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    assert_eq!(options.len(), 1, "only the printed trigger is pending");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    resolved(finish(game, play));
    game.resume_internal_card().unwrap();
}

fn play(game: &mut Game) {
    let play = game.play_internal_card("source", PLAYER).unwrap();
    let result = finish(game, play);
    assert!(
        matches!(result, CardPlay::Entered(_) | CardPlay::Resolved(_)),
        "the printed card play completes"
    );
    if matches!(game.internal_card_stage(), Some(ResumeStage::CheckTiming)) {
        game.resume_internal_card().unwrap();
    }
}

// Only supporting cards are synthetic; each tested card keeps its accepted program.
fn extra_fixture() -> &'static (SelectedEffects, WellFormedCardProgram) {
    static READY: OnceLock<(SelectedEffects, WellFormedCardProgram)> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost) in [
            ("ready-glass", "ツインプリズナー・グラス", "フォロワー", "3"),
            ("ready-combined", "フラム＝グラス", "フォロワー", "3"),
            ("ready-cerberus", "ケルベロス", "フォロワー", "3"),
            ("ready-expensive-amulet", "expensive amulet", "アミュレット", "6"),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if kind=="アミュレット" {"-"} else {"3"},"hp":if kind=="アミュレット" {"-"} else {"3"},"traits":[],"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"), json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        let selected = repository::selection_with_rules_from(&snapshot, &keywords, &input);
        let mut tree = expanded::tree().clone();
        tree["cards"] = json!({"ready-cerberus":tree["cards"]["synthetic-evolution"]});
        let base = lowerer().lower(&tree).unwrap().remove("ready-cerberus").unwrap();
        (selected, base)
    })
}
fn extra_game(initial: &Value) -> Game {
    let (selected, base) = extra_fixture();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-support",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "ready-glass",
        "ready-combined",
        "ready-cerberus",
        "ready-expensive-amulet",
    ] {
        game.load_internal_card(
            number,
            if number == "ready-cerberus" {
                base
            } else {
                &empty
            },
        )
        .unwrap();
    }
    game
}

fn amulet_return(number: &str, fanfare: bool) {
    let mut initial = position(number, "hand");
    let count = if fanfare { 1_u8 } else { 3 };
    let mut cemetery: Vec<_> = (0..count)
        .map(|i| json!({"id":format!("amulet{i}"),"card":"BP01-141"}))
        .collect();
    cemetery.push(json!({"id":"expensive","card":"ready-expensive-amulet"}));
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
    let mut game = extra_game(&initial);
    if fanfare {
        play(&mut game);
        trigger(&mut game);
    } else {
        let CardPlay::Awaiting(question) = game.play_internal_card("source", PLAYER).unwrap()
        else {
            panic!("formal cemetery targets")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("formal amulet targets")
        };
        assert_eq!(domain.members.len(), 3, "the six-cost amulet is excluded");
        assert_eq!(
            domain.cardinality.maximum,
            Some(Nat(3)),
            "the printed selection allows at most three amulets"
        );
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(domain.members.iter().map(|m| m.handle).collect()),
            )
            .unwrap();
        resolved(finish(&mut game, step));
        game.resume_internal_card().unwrap();
    }
    let state = game.projection(View::Referee).unwrap();
    for i in 0..count {
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!(format!("amulet{i}"))),
            "every chosen amulet enters the controller field"
        );
    }
    assert_eq!(
        state["P1"]["cemetery"],
        if fanfare {
            json!(["expensive"])
        } else {
            json!(["expensive", "source"])
        },
        "the over-cost card stays in the cemetery and the resolved spell joins it"
    );
}
#[test]
fn prayer_reanimates_three_amulets_and_excludes_six_cost() {
    amulet_return("BP01-132", false);
}
