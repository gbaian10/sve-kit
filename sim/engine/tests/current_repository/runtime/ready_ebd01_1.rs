#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(||{
  let (mut snapshot,keywords,mut input)=producer_inputs();
  // Minimal companions isolate the printed grave filters and authoritative evolution event.
  for (number,name,kind,class,cost,power,hp) in [
   ("ready-elf","ready-elf","フォロワー","エルフ","2","2","3"),
   ("ready-wrong","ready-wrong","フォロワー","ウィッチ","2","2","3"),
   ("ready-expensive","ready-expensive","フォロワー","エルフ","3","2","3"),
   ("ready-amulet","ready-amulet","アミュレット","エルフ","2","-","-"),
   ("ready-base","ready-base","フォロワー","エルフ","1","2","3"),
   ("ready-form","ready-base","フォロワー・エボルヴ","エルフ","-","3","4"),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":class,"card_type":kind,"cost":cost,"power":power,"hp":hp,"traits":[],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["sep"] = json!(0_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..5)
            .map(|i| json!({"id":format!("draw{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"ready-wrong"}]);
    initial
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        fixture().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-ebd01-1",
    )
    .unwrap();
    load_companions(&mut game);
    game
}
fn finish(game: &mut Game, mut step: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(mode == 1),
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the printed eligible follower may be offered"
                );
                Answer::Targets(
                    domain
                        .members
                        .first()
                        .map(|member| vec![member.handle])
                        .unwrap_or_default(),
                )
            }
            InputDomain::Modes { .. }
            | InputDomain::CostMaterials(_)
            | InputDomain::Search(_)
            | InputDomain::Order(_)
            | InputDomain::MealPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. } => {
                panic!("unexpected printed question: {:?}", question.domain)
            }
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}
fn settle(game: &mut Game, mode: u64) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability");
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step, mode);
    }
}
fn play(game: &mut Game, mode: u64) {
    let step = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(game, step, mode);
    settle(game, mode);
}

fn load_companions(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "ready-elf",
        "ready-wrong",
        "ready-expensive",
        "ready-amulet",
        "ready-form",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
}

#[test]
fn ebd01_spinne_recovers_a_low_cost_elf_to_ex_and_heals_only_for_an_allied_evolution() {
    for card in [
        "EBD01-007",
        "SP01-001",
        "SP01-SL01",
        "SP01-SP01",
        "SP01-SP02",
        "SP01-U01",
    ] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"recover","card":"ready-elf"},{"id":"wrong","card":"ready-wrong"},{"id":"expensive","card":"ready-expensive"},{"id":"amulet","card":"ready-amulet"}]);
        initial["players"]["P2"]["zones"]["cemetery"] =
            json!([{"id":"foreign","card":"ready-elf"}]);
        let mut entering = game(&initial);
        play(&mut entering, 1);
        let recovered = entering.projection(View::Referee).unwrap();
        assert!(
            recovered["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("recover")),
            "the one eligible own Elf follower enters EX"
        );
        let step = entering
            .play_internal_card("recover", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut entering, step, 1);
        settle(&mut entering, 1);
        assert_eq!(
            entering.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            7_u8
        );
        for seat in ["P1", "P2"] {
            let mut evolution_setup = position(card);
            evolution_setup["turn"]["active"] = json!(seat);
            evolution_setup["players"]["P1"]["zones"]["hand"] = json!([]);
            evolution_setup["players"]["P1"]["zones"]["field"] =
                json!([{"id":"source","card":card}]);
            evolution_setup["players"][seat]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"base","card":"ready-base"}));
            evolution_setup["players"][seat]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"ready-form"}]);
            let mut evolved = game(&evolution_setup);
            let mut tree = expanded::tree().clone();
            tree["cards"] = json!({"ready-base":tree["cards"]["synthetic-evolution"]});
            let program = lowerer()
                .lower(&tree)
                .unwrap()
                .remove("ready-base")
                .unwrap();
            evolved.load_internal_card("ready-base", &program).unwrap();
            assert_eq!(evolved.internal_rule_action(&json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":1_u8}})).unwrap().outcome,"resolved");
            settle(&mut evolved, 1);
            assert_eq!(
                evolved.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                if seat == "P1" { 12_u8 } else { 10_u8 }
            );
        }
    }
}
