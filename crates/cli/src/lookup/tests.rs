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
        "id": Uuid::new_v4(), "slug": slug, "name": slug, "personal": false, "role": "admin", "member_count": 1,
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

#[gtest]
fn views_resolve_by_name_and_say_when_two_share_it() -> googletest::Result<()> {
    let view = |id: &str, name: &str| {
        json!({
            "id": id, "heap_id": Uuid::new_v4(), "name": name, "created_by": Uuid::new_v4(), "archived_at": null,
            "version": 1, "allowed_actions": [],
            "view": { "open": "shown", "in_progress": "pinned", "blocked": "hidden", "blocked_layout": "grouped",
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
        "description": "Before they expire in March", "state": "open", "blocked_by": null, "need": "expected",
        "effort": null, "impact": null, "due": null, "tags": [], "parameters": [], "created_at": 0,
        "created_label": "today", "created_by": null, "closed_at": null, "closed_label": null, "closed_by": null,
        "closed_group": null, "version": 1, "allowed_actions": [],
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
