#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;
fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
  let (mut snapshot, keywords, mut input)=producer_inputs();
  // Minimal companions isolate printed trait filters and payments from unrelated abilities.
  for (number, kind, traits) in [
   ("ready-cool", "フォロワー", vec!["クール"]),
   ("ready-wrong", "フォロワー", vec![]),
   ("ready-cool-amulet", "アミュレット", vec!["クール"]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"1","power":if kind=="フォロワー" {"3"} else {"-"},"hp":if kind=="フォロワー" {"4"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  for form in ["CSD02b-006","CSD02b-008"] {
   let name=snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).find(|row|row["number"]==form).unwrap()["faces"][0]["name"].clone();
   let number=format!("ready-base-{form}");
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"3","hp":"4","traits":["クール"],"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("idolmaster_cinderella_girls");
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
        "ready-csd02b-1",
    )
    .unwrap();
    load_companions(&mut game);
    game
}
fn finish(game: &mut Game, mut step: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Modes { .. } => Answer::Modes(if mode == 0 {
                vec![ModeId(1), ModeId(2)]
            } else {
                vec![ModeId(mode)]
            }),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(mode == 1),
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the printed eligible follower or opposing leader may be offered"
                );
                Answer::Targets(
                    domain
                        .members
                        .first()
                        .map(|member| vec![member.handle])
                        .unwrap_or_default(),
                )
            }
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .first()
                    .map(|m| vec![m.handle])
                    .unwrap_or_default(),
            )),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            }
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
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

fn has_keyword(state: &Value, object: &str, keyword: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}
fn evolution_position(card: &str) -> Value {
    let mut initial = position(card);
    let base = format!("ready-base-{}", fixture().current[card].card_no);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
    initial
}
fn evolve_form(initial: &Value, card: &str, payment: bool) -> Game {
    let mut game = game(initial);
    let base = format!("ready-base-{}", fixture().current[card].card_no);
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({base.clone():tree["cards"]["synthetic-evolution"]});
    let program = lowerer().lower(&tree).unwrap().remove(&base).unwrap();
    game.load_internal_card(&base, &program).unwrap();
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the ordinary fixture evolution must resolve"
    );
    settle(&mut game, if payment { 1 } else { 2 });
    game
}

fn load_companions(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["ready-cool", "ready-wrong", "ready-cool-amulet"] {
        game.load_internal_card(number, &empty).unwrap();
    }
}
#[test]
fn csd02b_006_real_evolution_grants_power_rush_and_assail_only_to_another_cool_follower() {
    for card in ["CSD02b-006", "CSD02b-P03"] {
        let mut initial = evolution_position(card);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"ally","card":"ready-cool"}),
                json!({"id":"wrong","card":"ready-wrong"}),
                json!({"id":"amulet","card":"ready-cool-amulet"}),
            ]);
        let game = evolve_form(&initial, card, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["ally"]["power"], 4_u8);
        assert_eq!(state["objects"]["wrong"]["power"], 3_u8);
        assert!(
            has_keyword(&state, "ally", "突進"),
            "the chosen Cool follower gains Rush"
        );
        assert!(
            has_keyword(&state, "ally", "指定攻撃"),
            "the same follower gains Assail"
        );
        assert!(
            !has_keyword(&state, "source", "突進"),
            "the evolving source is excluded"
        );
        assert!(
            !has_keyword(&state, "wrong", "指定攻撃"),
            "the wrong trait is excluded"
        );
    }
}

#[test]
fn csd02b_008_evolution_can_decline_three_pp_and_cannot_pay_a_shortfall() {
    for card in ["CSD02b-008", "CSD02b-P05"] {
        for payment in 0_u8..3 {
            let mut initial = evolution_position(card);
            if payment == 2 {
                initial["players"]["P1"]["pp"]["current"] = json!(3_u8);
            }
            let game = evolve_form(&initial, card, payment != 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["leader"]["life"],
                if payment == 0 { 17_u8 } else { 20_u8 }
            );
            assert_eq!(
                state["P1"]["pp"]["current"],
                match payment {
                    0 => 6_u8,
                    1 => 9_u8,
                    _ => 2_u8,
                }
            );
        }
    }
}

#[test]
fn csd02b_009_both_fanfare_and_real_damage_death_buff_a_cool_follower() {
    for card in ["CSD02b-009", "CSD02b-P06"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":"ready-cool"},{"id":"wrong","card":"ready-wrong"}]);
        let mut entering = game(&initial);
        play(&mut entering, 1);
        let state = entering.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["ally"]["power"], 4_u8);
        assert_eq!(state["objects"]["wrong"]["power"], 3_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"source","card":card}));
        let damage = Action::Damage {
            recipients: RecipientSet::Objects {
                objects: subjects(),
            },
            amount: DamageAmount::Uniform {
                amount: Expr::Literal(Int(3)),
            },
            flags: DamageFlags::Ability,
            source: ObjectView::SourceInformation {},
            batch: BatchPolicy::Instruction {},
        };
        let mut departing = with_selected_producer(&initial, damage, fixture());
        load_companions(&mut departing);
        activate(&mut departing, "P1");
        settle(&mut departing, 1);
        let departed = departing.projection(View::Referee).unwrap();
        assert!(
            departed["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "real damage must destroy the printed source"
        );
        assert_eq!(departed["objects"]["ally"]["power"], 4_u8);
        assert_eq!(departed["objects"]["wrong"]["power"], 3_u8);
    }
}
