#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed payment scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn stat(kind: &str, value: &'static str) -> &'static str {
    if kind.starts_with("フォロワー") {
        value
    } else {
        "-"
    }
}
fn prepared() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        // Same-name stand-ins supply cards whose own programs are not admitted yet.
        for (id, name, kind) in [
            ("pay-unit", "test unit", "フォロワー"),
            ("pay-magna", "マグナセイバー", "フォロワー"),
            ("pay-magna-e", "マグナセイバー", "フォロワー・エボルヴ"),
            ("pay-ring", "惨禍の円環", "アミュレット"),
            ("pay-ring-spell", "惨禍の円環", "スペル"),
        ] {
            snapshot.push_str(&json!({"number":id,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"traits":[],"cost":"1","power":stat(kind, "1"),"hp":stat(kind, "10"),"text":"","sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{id}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{id:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}
fn printings(sample: &str) -> Vec<String> {
    let id = &prepared().current[sample].card_id;
    prepared()
        .current
        .iter()
        .filter(|(_, c)| &c.card_id == id)
        .map(|(number, _)| number.clone())
        .collect()
}
fn load_stand_ins(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for id in ["pay-unit", "pay-magna-e", "pay-ring", "pay-ring-spell"] {
        game.load_internal_card(id, &empty).unwrap();
    }
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "payment-light",
    )
    .unwrap();
    load_stand_ins(&mut game);
    game
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":card},{"id":"fee","card":"pay-unit"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top1","card":"pay-unit"},{"id":"top2","card":"pay-unit"},{"id":"tail","card":"pay-unit"},{"id":"last","card":"pay-unit"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"pay-unit"}]);
    initial
}
fn settle(game: &mut Game, mut play: CardPlay, mode: u64, accept: bool) {
    loop {
        while let CardPlay::Awaiting(q) = play {
            let answer = match &q.domain {
                InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
                InputDomain::OptionalCost => Answer::OptionalCost(accept),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                InputDomain::Targets(domain) => Answer::Targets(
                    domain
                        .members
                        .iter()
                        .take(usize::try_from(domain.cardinality.minimum.0.max(1)).unwrap())
                        .map(|m| m.handle)
                        .collect(),
                ),
                InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                    domain
                        .members
                        .iter()
                        .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                        .map(|m| m.handle)
                        .collect(),
                ),
                InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                    domain.cards.iter().map(|c| c.handle).collect(),
                )),
                InputDomain::Order(cards) => {
                    Answer::Order(cards.iter().map(|c| c.handle).collect())
                }
                _ => panic!("unexpected light payment question"),
            };
            play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending trigger")
        };
        play = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
    }
}
#[test]
fn bp14_101_healing_trigger_pays_act_to_draw() {
    for number in printings("BP14-101") {
        for accept in [true, false] {
            let mut initial = position(&number);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
            let mut game = with_selected_producer(
                &initial,
                Action::Life {
                    player: PlayerRef::Controller {},
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(1)),
                    },
                },
                prepared(),
            );
            load_stand_ins(&mut game);
            activate(&mut game, "P1");
            settle(&mut game, CardPlay::Cancelled, 1, accept);
            let s = game.projection(View::Referee).unwrap();
            assert_eq!(
                s["objects"]["source"]["acted"], accept,
                "the resolution payment acts only when accepted"
            );
            assert_eq!(
                s["P1"]["hand"],
                if accept { json!(["top1"]) } else { json!([]) },
                "only the paid trigger draws"
            );
        }
    }
}
#[test]
fn bp14_111_named_ally_evolution_pays_source_to_cemetery() {
    for number in printings("BP14-111") {
        let mut initial = position(&number);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":number},{"id":"magna","card":"pay-magna"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"pay-magna-e"}]);
        let mut game = game(&initial);
        let mut tree = expanded::tree().clone();
        tree["cards"] = json!({"pay-magna": tree["cards"]["synthetic-evolution"]});
        let actor = lowerer().lower(&tree).unwrap().remove("pay-magna").unwrap();
        game.load_internal_card("pay-magna", &actor).unwrap();
        game.internal_rule_action(
            &json!({"do":"evolve","source":"magna","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        settle(&mut game, CardPlay::Cancelled, 1, true);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["cemetery"],
            json!(["source"]),
            "the watching follower pays with its own source"
        );
        assert_eq!(
            s["P1"]["leader"]["life"], 21_i64,
            "the paid resolution heals one"
        );
    }
}
#[test]
fn bp10_094_fanfare_searches_the_amulet_into_ex_and_its_own_trigger_destroys() {
    for number in printings("BP10-094") {
        let mut initial = position(&number);
        initial["players"]["P1"]["pp"] = json!({"current":4_u8,"max":4_u8});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"decoy","card":"pay-ring-spell"},{"id":"ring","card":"pay-ring"},{"id":"rest","card":"pay-unit"}
        ]);
        let mut game = game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        settle(&mut game, play, 1, true);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["ex"],
            json!(["ring"]),
            "only the same-name amulet is searched into EX"
        );
        assert_eq!(
            s["P1"]["pp"]["current"], 0_i64,
            "the card cost and the fanfare fee are both paid"
        );
        assert_eq!(
            s["objects"]["source"]["acted"], true,
            "the EX placement trigger acts its own source"
        );
        assert_eq!(
            s["P2"]["field"],
            json!([]),
            "the paid trigger destroys the opposing follower"
        );
    }
}
