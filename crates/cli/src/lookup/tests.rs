use googletest::prelude::*;
use serde_json::json;

use super::*;

const TAG_INFRA: &str = "00000000-0000-0000-0000-00000000000a";
const VIEW_A: &str = "00000000-0000-0000-0000-0000000000a1";
const VIEW_B: &str = "00000000-0000-0000-0000-0000000000a2";

fn heap(slug: &str) -> serde_json::Value {
    json!({
        "id": Uuid::new_v4(), "team_id": Uuid::new_v4(), "slug": slug, "name": slug,
        "archived": false, "open_count": 0,
    })
}

fn team(slug: &str, heaps: &[&str]) -> serde_json::Value {
    json!({
        "id": Uuid::new_v4(), "slug": slug, "name": slug, "personal": slug == "ada", "role": "admin", "member_count": 1,
        "heaps": heaps.iter().map(|h| heap(h)).collect::<Vec<_>>(),
    })
}

fn me() -> googletest::Result<MeDto> {
    Ok(serde_json::from_value(json!({
        "user": { "id": Uuid::new_v4(), "username": "ada", "email": "ada@example.test", "display_name": "Ada", "version": 1 },
        "teams": [team("ada", &["personal", "backlog"]), team("infra", &["backlog", "ops"])],
        "allowed_actions": ["sign_out", "create_team"],
    }))?)
}

fn handle() -> HeapHandle {
    HeapHandle {
        team: "ada".to_owned(),
        heap: "personal".to_owned(),
    }
}

#[gtest]
fn a_heap_name_is_team_slash_heap_or_a_slug() -> googletest::Result<()> {
    verify_that!(
        HeapName::parse("infra/ops")?,
        eq(HeapName::Full {
            team: "infra",
            heap: "ops"
        })
    )?;
    verify_that!(HeapName::parse("ops")?, eq(HeapName::Bare("ops")))?;
    for bad in ["", "/ops", "infra/", "a/b/c"] {
        verify_that!(
            HeapName::parse(bad),
            err(matches_pattern!(LookupError::MalformedHeap { .. }))
        )?;
    }
    Ok(())
}

#[gtest]
fn the_personal_team_is_found_or_missing() -> googletest::Result<()> {
    let mut me = me()?;
    verify_that!(find_personal_team(&me)?, eq("ada"))?;
    me.teams.retain(|team| !team.personal);
    verify_that!(find_personal_team(&me), err(pat!(LookupError::NoPersonalTeam)))
}

#[gtest]
fn a_bare_slug_finds_the_one_heap_by_it() -> googletest::Result<()> {
    let found = resolve_heap(HeapName::Bare("ops"), &me()?)?;
    verify_that!(found.to_string(), eq("infra/ops"))
}

#[gtest]
fn a_bare_slug_two_teams_have_is_ambiguous() -> googletest::Result<()> {
    let found = resolve_heap(HeapName::Bare("backlog"), &me()?);
    verify_that!(
        found,
        err(matches_pattern!(LookupError::AmbiguousHeap {
            candidates: elements_are![eq("ada/backlog"), eq("infra/backlog")],
            ..
        }))
    )
}

#[gtest]
fn a_full_name_must_be_one_of_the_callers_heaps() -> googletest::Result<()> {
    verify_that!(
        resolve_heap(
            HeapName::Full {
                team: "ada",
                heap: "backlog"
            },
            &me()?
        )?
        .to_string(),
        eq("ada/backlog")
    )?;
    verify_that!(
        resolve_heap(
            HeapName::Full {
                team: "infra",
                heap: "personal"
            },
            &me()?
        ),
        err(matches_pattern!(LookupError::UnknownHeap {
            name: eq("infra/personal")
        }))
    )
}

#[gtest]
fn tags_resolve_by_name_ignoring_case_or_by_id() -> googletest::Result<()> {
    let tags: TagListDto = serde_json::from_value(json!({
        "heap": { "id": Uuid::new_v4(), "team_id": Uuid::new_v4(), "slug": "personal", "name": "Personal", "archived": false },
        "tags": [{ "id": TAG_INFRA, "heap_id": Uuid::new_v4(), "name": "Infra", "description": null, "version": 1, "allowed_actions": [] }],
        "allowed_actions": [],
    }))?;
    let id: Uuid = TAG_INFRA.parse()?;

    verify_that!(
        resolve_tags(&["infra".to_owned(), TAG_INFRA.to_owned()], &tags, &handle())?,
        elements_are![eq(&id), eq(&id)]
    )?;
    verify_that!(
        resolve_tags(&["ops".to_owned()], &tags, &handle()),
        err(displays_as(eq("ada/personal has no tag `ops`; it has Infra")))
    )
}

fn tags_named(names: &[&str]) -> googletest::Result<TagListDto> {
    Ok(serde_json::from_value(json!({
        "heap": { "id": Uuid::new_v4(), "team_id": Uuid::new_v4(), "slug": "personal", "name": "Personal", "archived": false },
        "tags": names.iter().enumerate().map(|(i, name)| json!({
            "id": Uuid::from_u128(i as u128 + 1), "heap_id": Uuid::new_v4(), "name": name, "description": null,
            "version": 1, "allowed_actions": [],
        })).collect::<Vec<_>>(),
        "allowed_actions": [],
    }))?)
}

#[gtest]
fn an_unknown_tag_names_the_closest_ones_first() -> googletest::Result<()> {
    let tags = tags_named(&["Billing", "Infra", "Ops"])?;

    verify_that!(
        find_tag("Opz", &tags, &handle()),
        err(displays_as(eq(
            "ada/personal has no tag `Opz`; it has Ops, Infra, Billing"
        )))
    )
}

#[gtest]
fn more_than_five_tags_name_only_the_five_closest() -> googletest::Result<()> {
    let tags = tags_named(&["Aaaa", "Bbbb", "Cccc", "Dddd", "Eeee", "Ffff", "Ops"])?;

    verify_that!(
        find_tag("ops2", &tags, &handle()),
        err(displays_as(eq(
            "ada/personal has no tag `ops2`; the closest are Ops, Aaaa, Bbbb, Cccc, Dddd"
        )))
    )
}

#[gtest]
fn a_heap_without_tags_names_none() -> googletest::Result<()> {
    let tags = tags_named(&[])?;

    verify_that!(
        find_tag("ops", &tags, &handle()),
        err(displays_as(eq("ada/personal has no tag `ops`")))
    )
}

#[gtest]
fn find_tag_matches_by_name_ignoring_case_and_by_id() -> googletest::Result<()> {
    let tags = tags_named(&["Infra", "Ops"])?;

    verify_that!(
        find_tag("OPS", &tags, &handle()).map(|tag| tag.name.clone()),
        ok(eq("Ops"))
    )?;
    verify_that!(
        find_tag(&Uuid::from_u128(1).to_string(), &tags, &handle()).map(|tag| tag.name.clone()),
        ok(eq("Infra"))
    )
}

#[gtest]
fn edit_distance_counts_chars() {
    expect_that!(levenshtein("kitten", "sitting"), eq(3));
    expect_that!(levenshtein("", "abc"), eq(3));
    expect_that!(levenshtein("café", "cafe"), eq(1));
}

#[gtest]
fn views_resolve_by_name_and_say_when_two_share_it() -> googletest::Result<()> {
    let view = |id: &str, name: &str| {
        json!({
            "id": id, "heap_id": Uuid::new_v4(), "name": name, "created_by": Uuid::new_v4(), "archived_at": null,
            "version": 1, "allowed_actions": [],
            "view": { "open": "shown", "in_progress": "pinned", "blocked": "hidden", "blocked_layout": "grouped", "reminders": "hidden",
                      "sort": "priority", "sort_parameter": null, "tag": [], "effort": [], "impact": [], "need": [], "param": [] },
        })
    };
    let views: SavedViewListDto = serde_json::from_value(json!({
        "heap": { "id": Uuid::new_v4(), "team_id": Uuid::new_v4(), "slug": "personal", "name": "Personal", "archived": false },
        "views": [view(VIEW_A, "Quick wins"), view(VIEW_B, "Mine"), view("00000000-0000-0000-0000-0000000000a3", "mine")],
        "allowed_actions": [],
    }))?;

    verify_that!(find_view("quick WINS", &views, &handle())?.id.to_string(), eq(VIEW_A))?;
    verify_that!(find_view(VIEW_B, &views, &handle())?.name, eq("Mine"))?;
    verify_that!(
        find_view("mine", &views, &handle()),
        err(matches_pattern!(LookupError::AmbiguousView { .. }))
    )?;
    verify_that!(
        find_view("nope", &views, &handle()),
        err(matches_pattern!(LookupError::UnknownView { .. }))
    )
}

#[gtest]
fn text_matches_title_or_description_ignoring_case() -> googletest::Result<()> {
    let item: ItemDto = serde_json::from_value(json!({
        "id": Uuid::new_v4(), "key": "k3x9a", "heap_id": Uuid::new_v4(), "title": "Rotate the staging certs",
        "description": "Before they expire in March", "kind": "note", "stage": "as_written", "stage_choices": [], "state": "open", "blocked_by": null, "need": "expected",
        "effort": null, "impact": null, "due": null, "tags": [], "parameters": [], "created_at": 0,
        "created_label": "today", "created_by": null, "closed_at": null, "closed_label": null, "closed_by": null,
        "closed_group": null, "steps": [], "step_progress": null, "version": 1, "allowed_actions": [],
    }))?;

    verify_that!(matches_text(&item, "STAGING"), eq(true))?;
    verify_that!(matches_text(&item, "march"), eq(true))?;
    verify_that!(matches_text(&item, "prod"), eq(false))
}

fn item_in(team: &str, heap: &str, key: &str) -> ItemName {
    ItemName::Full {
        heap: HeapHandle {
            team: team.to_owned(),
            heap: heap.to_owned(),
        },
        key: key.to_owned(),
    }
}

#[gtest]
fn an_item_is_named_by_its_key_a_path_or_a_link() -> googletest::Result<()> {
    verify_that!(ItemName::parse("k3x9a")?, eq(&ItemName::Key("k3x9a".to_owned())))?;
    verify_that!(
        ItemName::parse("team/heap/k3x9a")?,
        eq(&item_in("team", "heap", "k3x9a"))
    )?;
    for link in [
        "https://x.test/h/team/heap/k3x9a",
        "http://localhost:8080/h/team/heap/k3x9a/",
        "https://x.test/h/team/heap/k3x9a?x=1",
        "https://x.test/h/team/heap/k3x9a#top",
        "https://x.test/prefix/h/team/heap/k3x9a",
        "https://x.test/teams/team/heaps/heap/items/k3x9a",
        "http://localhost:8080/teams/team/heaps/heap/items/k3x9a/",
        "https://x.test/teams/team/heaps/heap/items/k3x9a?x=1",
        "https://x.test/teams/team/heaps/heap/items/k3x9a#top",
        "https://x.test/prefix/teams/team/heaps/heap/items/k3x9a",
    ] {
        expect_that!(ItemName::parse(link)?, eq(&item_in("team", "heap", "k3x9a")), "{link}");
    }
    Ok(())
}

#[gtest]
fn anything_else_names_no_item() -> googletest::Result<()> {
    for bad in [
        "",
        "a/b",
        "a/b/c/d",
        "a//k",
        "/a/b/k",
        "https://x.test/team/heap/k3x9a",
        "https://x.test/h/team/heap",
        "https://x.test/h/team//k3x9a",
        "https://x.test/teams/team/heaps/heap/items",
        "https://x.test/teams/team/heaps//items/k3x9a",
        "https://",
    ] {
        expect_that!(
            ItemName::parse(bad),
            err(matches_pattern!(LookupError::MalformedItem { .. })),
            "{bad:?}"
        );
    }
    Ok(())
}

#[gtest]
fn a_step_is_an_item_and_a_number_split_at_the_last_dot() -> googletest::Result<()> {
    let step = |name: &str| StepName::parse(name);

    verify_that!(
        step("k3x9a.3")?,
        eq(&StepName {
            item_typed: "k3x9a".to_owned(),
            item: ItemName::Key("k3x9a".to_owned()),
            number: 3,
        })
    )?;
    verify_that!(
        step("team/heap/k3x9a.12")?,
        eq(&StepName {
            item_typed: "team/heap/k3x9a".to_owned(),
            item: item_in("team", "heap", "k3x9a"),
            number: 12,
        })
    )?;
    // The host of a link has dots, and so does an address with a port; only the part after the last `/` is split.
    verify_that!(
        step("https://up.heap.example.test/h/team/heap/k3x9a.3")?,
        eq(&StepName {
            item_typed: "https://up.heap.example.test/h/team/heap/k3x9a".to_owned(),
            item: item_in("team", "heap", "k3x9a"),
            number: 3,
        })
    )?;
    Ok(())
}

#[gtest]
fn anything_else_names_no_step() {
    for bad in [
        "",
        "k3x9a",
        "k3x9a.",
        "k3x9a.x",
        "k3x9a.0",
        "k3x9a.-1",
        "k3x9a.03",
        ".3",
        "a/b.3",
        "k3x9a.3.4",
    ] {
        expect_that!(
            StepName::parse(bad),
            err(matches_pattern!(LookupError::MalformedStep { .. })),
            "{bad:?}"
        );
    }
}

fn due_choices() -> googletest::Result<DueChoicesDto> {
    Ok(serde_json::from_value(json!({ "choices": [
        { "label": "today", "date": "2027-01-01" },
        { "label": "next week", "date": "2027-01-08" },
        { "label": "in a few weeks", "date": "2027-01-22" },
    ] }))?)
}

#[gtest]
fn a_date_written_out_is_a_firm_deadline_and_anything_else_is_rough() -> googletest::Result<()> {
    verify_that!(
        DueWhen::parse("2027-03-12"),
        eq(&DueWhen::Date("2027-03-12".to_owned()))
    )?;
    for rough in ["next-week", "2027-13-40", "2027-3-12", "2027-03-12T10:00:00"] {
        expect_that!(DueWhen::parse(rough), eq(&DueWhen::Rough(rough.to_owned())), "{rough}");
    }
    let due = due_on("2027-03-12");
    verify_that!((due.date.as_str(), due.firm), eq(("2027-03-12", true)))
}

#[gtest]
fn a_rough_choice_matches_ignoring_case_dashes_and_spacing() -> googletest::Result<()> {
    let choices = due_choices()?;
    for typed in ["next-week", "Next Week", " NEXT--week "] {
        let due = match_due_choice(typed, &choices)?;
        expect_that!((due.date.as_str(), due.firm), eq(("2027-01-08", false)), "{typed}");
    }
    let due = match_due_choice("in a few  weeks", &choices)?;
    verify_that!(due.date, eq("2027-01-22"))
}

#[gtest]
fn an_unknown_due_time_lists_the_choices_in_the_servers_order() -> googletest::Result<()> {
    verify_that!(
        match_due_choice("someday", &due_choices()?),
        err(displays_as(eq(
            "`someday` isn't a due time: give a date, YYYY-MM-DD, or one of: today, next week, in a few weeks"
        )))
    )?;
    verify_that!(
        match_due_choice("2027-13-40", &due_choices()?),
        err(matches_pattern!(LookupError::UnknownDueChoice { .. }))
    )
}

fn parameters() -> googletest::Result<ParameterListDto> {
    let option = |id: u128, label: &str| json!({ "id": Uuid::from_u128(id), "label": label });
    Ok(serde_json::from_value(json!({
        "heap": { "id": Uuid::new_v4(), "team_id": Uuid::new_v4(), "slug": "personal", "name": "Personal", "archived": false },
        "parameters": [{
            "id": Uuid::from_u128(1), "heap_id": Uuid::new_v4(), "name": "Area", "weight": 10,
            "options": [option(11, "Backend"), option(12, "Front end")],
            "archived_options": [{ "id": Uuid::from_u128(13), "label": "Legacy", "archived_at": 0, "replaced_by_option_id": Uuid::from_u128(11) }],
            "tag_ids": [], "archived_at": null, "version": 1, "allowed_actions": [],
        }, {
            "id": Uuid::from_u128(2), "heap_id": Uuid::new_v4(), "name": "Empty", "weight": 10,
            "options": [], "archived_options": [], "tag_ids": [], "archived_at": null, "version": 1, "allowed_actions": [],
        }],
        "archived": [], "allowed_actions": [],
    }))?)
}

#[gtest]
fn an_option_is_found_by_label_ignoring_case_or_by_id() -> googletest::Result<()> {
    let parameters = parameters()?;
    let area = find_parameter("area", &parameters, &handle())?;

    verify_that!(
        find_option("FRONT END", area, &handle()).map(|option| option.id).ok(),
        some(eq(Uuid::from_u128(12)))
    )?;
    verify_that!(
        find_option(&Uuid::from_u128(11).to_string(), area, &handle())
            .map(|option| option.label.as_str())
            .ok(),
        some(eq("Backend"))
    )
}

#[gtest]
fn an_archived_or_unknown_option_lists_the_options_in_use() -> googletest::Result<()> {
    let parameters = parameters()?;
    let area = find_parameter("Area", &parameters, &handle())?;
    let empty = find_parameter("Empty", &parameters, &handle())?;

    verify_that!(
        find_option("Legacy", area, &handle()),
        err(displays_as(eq(
            "parameter `Area` in ada/personal has no option `Legacy`; it has Backend, Front end"
        )))
    )?;
    verify_that!(
        find_option("X", empty, &handle()),
        err(displays_as(eq("parameter `Empty` in ada/personal has no option `X`")))
    )
}

#[gtest]
fn a_question_is_named_by_its_item_and_number() -> googletest::Result<()> {
    verify_that!(
        QuestionName::parse("k3x9a.q2")?,
        eq(&QuestionName {
            item_typed: "k3x9a".to_owned(),
            item: ItemName::Key("k3x9a".to_owned()),
            number: 2,
        })
    )?;
    verify_that!(
        QuestionName::parse("team/heap/k3x9a.q12")?,
        eq(&QuestionName {
            item_typed: "team/heap/k3x9a".to_owned(),
            item: item_in("team", "heap", "k3x9a"),
            number: 12,
        })
    )?;
    verify_that!(
        QuestionName::parse("https://up.heap.example.test/h/team/heap/k3x9a.q3")?,
        eq(&QuestionName {
            item_typed: "https://up.heap.example.test/h/team/heap/k3x9a".to_owned(),
            item: item_in("team", "heap", "k3x9a"),
            number: 3,
        })
    )?;
    Ok(())
}

#[gtest]
fn anything_else_names_no_question() {
    for bad in [
        "",
        "k3x9a",
        "k3x9a.2",
        "k3x9a.q",
        "k3x9a.q0",
        "k3x9a.Q2",
        "k3x9a.q-1",
        "k3x9a.q02",
        ".q2",
        "a/b.q2",
        "k3x9a.q2.q3",
    ] {
        expect_that!(
            QuestionName::parse(bad),
            err(matches_pattern!(LookupError::MalformedQuestion { .. })),
            "{bad:?}"
        );
    }
}

fn picks(typed: &[&str], choices: &[&str]) -> Result<Vec<u32>, LookupError> {
    let strings = |texts: &[&str]| texts.iter().map(|text| (*text).to_owned()).collect::<Vec<_>>();
    match_choices("k3x9a.q2", &strings(typed), &strings(choices))
}

#[gtest]
fn a_pick_is_a_label_ignoring_case_or_a_number() {
    expect_that!(picks(&["internal"], &["Internal", "Let's Encrypt"]), ok(eq(&vec![0])));
    expect_that!(
        picks(&["  LET'S encrypt "], &["Internal", "Let's Encrypt"]),
        ok(eq(&vec![1]))
    );
    expect_that!(picks(&["2", "1"], &["a", "b", "c"]), ok(eq(&vec![1, 0])));
}

#[gtest]
fn a_label_beats_a_number_and_a_repeated_pick_counts_once() {
    expect_that!(picks(&["2"], &["x", "2"]), ok(eq(&vec![1])));
    expect_that!(picks(&["1"], &["a", "1"]), ok(eq(&vec![1])));
    expect_that!(picks(&["b", "2", "a", "B"], &["a", "b"]), ok(eq(&vec![1, 0])));
}

#[gtest]
fn an_unknown_pick_lists_the_choices() {
    for typed in ["maybe", "0", "3", "-1", ""] {
        expect_that!(
            picks(&[typed], &["Done", "Can't be done"]),
            err(matches_pattern!(LookupError::UnknownChoice {
                typed: eq(typed),
                known: eq(&vec!["Done".to_owned(), "Can't be done".to_owned()]),
                ..
            })),
            "{typed:?}"
        );
    }
}

#[gtest]
fn a_pick_on_a_free_text_question_is_refused() {
    expect_that!(
        picks(&["1"], &[]),
        err(matches_pattern!(LookupError::NoChoicesToPick { .. }))
    );
    expect_that!(picks(&[], &[]), ok(eq(&Vec::<u32>::new())));
}
