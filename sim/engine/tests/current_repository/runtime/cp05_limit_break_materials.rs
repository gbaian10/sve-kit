use super::*;

const ELEMENTS: [(&str, &str, &str, &str); 6] = [
    ("CP05-T09", "CP05-P93", "fire", "火属性"),
    ("CP05-T10", "CP05-P94", "water", "水属性"),
    ("CP05-T11", "CP05-P95", "earth", "土属性"),
    ("CP05-T12", "CP05-P96", "wind", "風属性"),
    ("CP05-T13", "CP05-P97", "light", "光属性"),
    ("CP05-T14", "CP05-P98", "dark", "闇属性"),
];
const GOLD: [&str; 2] = ["CP05-T15", "CP05-P99"];

fn materials() -> &'static SelectedEffects {
    static MATERIALS: OnceLock<SelectedEffects> = OnceLock::new();
    MATERIALS.get_or_init(|| {
        let (source_snapshot, keywords, input) = inputs();
        let prints: BTreeSet<_> = ELEMENTS
            .iter()
            .flat_map(|&(number, alias, _, _)| [number, alias])
            .chain(GOLD)
            .collect();
        let mut snapshot = source_snapshot
            .lines()
            .filter(|line| {
                let record: Value = serde_json::from_str(line).unwrap();
                record["number"]
                    .as_str()
                    .is_some_and(|number| prints.contains(number))
            })
            .map(String::from)
            .collect::<Vec<_>>();
        let mut authors = input["authors"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|author| {
                author["input"]["card_no"]
                    .as_str()
                    .is_some_and(|number| prints.contains(number))
            })
            .cloned()
            .collect::<Vec<_>>();

        // Isolate the material's own printed ability from the bearer's triggered abilities.
        for (index, (number, trait_name)) in ELEMENTS
            .iter()
            .map(|&(_, _, number, trait_name)| (number, trait_name))
            .chain([("granblue", "グラブル"), ("unrelated", "人形")])
            .enumerate()
        {
            let (mut record, author) = current_fixture::author(
                number,
                &format!("{index:032x}"),
                "フォロワー",
                vec![json!({"keywords":["final_limit_break_possible"]})],
            );
            record["faces"][0]["traits"] = json!([trait_name]);
            snapshot.push(record.to_string());
            authors.push(author);
        }
        Catalog::from_current_selection(
            &snapshot.join("\n"),
            keywords,
            &json!({"documents":[],"authors":authors}),
        )
        .unwrap()
    })
}

fn board(printings: [&str; 2], bearer: &str) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":0_u8,"max":0_u8});
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"bearer","card":bearer}, {"id":"other","card":"unrelated"}]);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"first","card":printings[0]}, {"id":"second","card":printings[1]}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":bearer}]);
    Game::from_selected_effects(
        materials().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "cp05-limit-break-materials",
    )
    .unwrap()
}

fn use_material(game: &mut Game, material: &str, bearer: &str) {
    let CardPlay::Awaiting(question) = game
        .activate_internal_card(material, AbilityId(1), actor())
        .unwrap()
    else {
        panic!("expected a formal target before the material resolves");
    };
    let InputDomain::Targets(targets) = question.domain else {
        panic!("expected a printed follower target");
    };
    assert_eq!(targets.members.len(), 1, "only one follower qualifies");
    let candidate = targets.members.first().unwrap();
    assert_eq!(candidate.card.as_deref(), Some(bearer), "the trait matches");
    assert_eq!(candidate.player, actor(), "the follower is on our field");
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![candidate.handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

fn element(index: usize) {
    let (number, alias, bearer, _) = ELEMENTS[index];
    let mut game = board([number, alias], bearer);
    use_material(&mut game, "first", bearer);
    let normal = game.projection(View::Referee).unwrap();
    assert_eq!(
        normal["P1"]["ex"],
        json!(["second"]),
        "the unused reprint stays in EX"
    );
    assert_eq!(
        normal["P1"]["limit_break"],
        json!(["first"]),
        "the spent element attaches"
    );
    assert_eq!(
        normal["objects"]["bearer"]["power"], 2_u8,
        "normal break adds one attack"
    );
    assert_eq!(
        normal["objects"]["bearer"]["max_hp"], 2_u8,
        "normal break leaves maximum life"
    );

    use_material(&mut game, "second", bearer);
    let final_stage = game.projection(View::Referee).unwrap();
    assert_eq!(
        final_stage["P1"]["ex"],
        json!([]),
        "both materials were spent"
    );
    assert_eq!(
        final_stage["P1"]["limit_break"],
        json!(["first", "second"]),
        "both elements attach"
    );
    assert_eq!(
        final_stage["objects"]["bearer"]["power"], 3_u8,
        "final break adds one more attack"
    );
    assert_eq!(
        final_stage["objects"]["bearer"]["max_hp"], 3_u8,
        "final break adds one life"
    );
}

#[test]
fn fire_element_targets_own_fire_follower_and_replaces_the_second_break() {
    element(0);
}

#[test]
fn water_element_targets_own_water_follower_and_replaces_the_second_break() {
    element(1);
}

#[test]
fn earth_element_targets_own_earth_follower_and_replaces_the_second_break() {
    element(2);
}

#[test]
fn wind_element_targets_own_wind_follower_and_replaces_the_second_break() {
    element(3);
}

#[test]
fn light_element_targets_own_light_follower_and_replaces_the_second_break() {
    element(4);
}

#[test]
fn dark_element_targets_own_dark_follower_and_replaces_the_second_break() {
    element(5);
}

#[test]
fn gold_targets_own_granblue_follower_and_supplies_both_stages_at_once() {
    let [canonical, reprint] = GOLD;
    for printings in [GOLD, [reprint, canonical]] {
        let mut game = board(printings, "granblue");
        use_material(&mut game, "first", "granblue");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"], json!(["second"]));
        assert_eq!(state["P1"]["limit_break"], json!(["first"]));
        assert_eq!(state["objects"]["bearer"]["power"], 3_u8);
        assert_eq!(state["objects"]["bearer"]["max_hp"], 3_u8);
    }
}
