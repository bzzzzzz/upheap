//! Reading the audit log: a heap's or a team's history, or one entity's, filtered by what the caller may see.
//!
//! Entries come back ready to show. Each change names the field in words and gives the values before and after as
//! text, with IDs replaced by the names those things have now (tags, options, items, users), since a diff only
//! records IDs (see `audit.rs`, which writes them). The raw diff stays on the entry for API clients that want it.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};

use domain::audit::{AuditAction, AuditField, EntityType};
use domain::due::{describe_due_date, describe_time};
use domain::heap::Heap;
use domain::item::{Due, Kind, Need, Size, Stage, StateKind};
use domain::permission::{Permission, visible_audit_actions};
use domain::question::QuestionKind;
use domain::step::StepState;
use domain::tag::TagColor;
use domain::team::{Role, Team};
use domain::text_diff::{WordDiff, diff_words};
use domain::turn::{Goal, Reach, TakeBackWhy, TurnSource};
use domain::{HeapId, ItemId, RunId, SavedViewId, TeamId, Timestamp, UserId};
use jiff::tz::TimeZone;
use serde_json::{Map, Value};
use store::StoreError;
pub use store::audit::AuditCursor;
use store::audit::{AuditEntry, AuditQuery, AuditScope};
use uuid::Uuid;

use crate::access::{authorize_heap, authorize_team};
use crate::items::{ItemOrigin, find_viewable_item};
use crate::{Principal, ServiceError, Services};

/// The largest page of audit entries.
pub const MAX_AUDIT_PAGE: u64 = 200;

/// A page of audit entries, newest first.
#[derive(Debug)]
pub struct AuditPage {
    pub entries: Vec<AuditEntryView>,
    /// Where the next page starts; `None` on the last page.
    pub next: Option<AuditCursor>,
}

/// A page of what happened across heaps, newest first.
#[derive(Debug)]
pub struct AcrossHeapsAuditPage {
    pub rows: Vec<AcrossHeapsAuditRow>,
    /// Where the next page starts; `None` on the last page.
    pub next: Option<AuditCursor>,
}

/// An entry of the history across heaps, with the heap it was made in.
#[derive(Debug)]
pub struct AcrossHeapsAuditRow {
    pub heap: Heap,
    pub team: Team,
    pub entry: AuditEntryView,
}

/// An audit entry with what is needed to show it.
#[derive(Debug)]
pub struct AuditEntryView {
    pub id: Uuid,
    pub created_at: Timestamp,
    /// When it happened, in the caller's timezone.
    pub created_label: String,
    pub actor: UserId,
    /// The actor's display name now.
    pub actor_name: String,
    pub action: AuditAction,
    /// What was done, read after the actor's name: "edited item", "joined the team".
    pub phrase: Cow<'static, str>,
    /// Whether the entity's name goes after `phrase`. Not when the entry is about the actor themselves, as with
    /// "updated their profile".
    pub names_entity: bool,
    pub entity_type: EntityType,
    pub entity_id: Uuid,
    /// What the entity is called now: an item's title, a tag's name, a member's display name.
    pub entity_label: String,
    /// The changed fields, in the order the entity's form shows them.
    pub changes: Vec<AuditChange>,
    /// The change as recorded, with IDs rather than names (see `audit.rs`).
    pub diff: Value,
    /// The run an agent made the change in; `None` for a person's own change.
    pub run: Option<AuditRun>,
}

/// The run an audit entry was made in.
#[derive(Clone, Debug)]
pub struct AuditRun {
    pub id: RunId,
    /// The item the run worked on and where it is now; `None` when the caller can't view its heap.
    pub item: Option<ItemOrigin>,
}

/// One changed field of an audit entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditChange {
    /// The field as the diff names it: `title`, `tags`, or `parameters.<id>` for one parameter's value.
    pub field: String,
    /// The field in words: "Title", "Tags", or the parameter's name.
    pub label: String,
    /// `None` when it had no value, and for values with no short form, such as what a saved view shows.
    pub before: Option<String>,
    pub after: Option<String>,
    /// The edit as a word diff, for an edited text with a value before and after: an item's title and context, a
    /// step's title and context, a heap's context. `None` for any other change, and when too little stayed the same
    /// to read well as a diff.
    pub text_diff: Option<WordDiff>,
}

/// Which history to read and from where.
#[derive(Clone, Copy, Debug)]
pub struct AuditRequest {
    /// Only the history of this item, tag, parameter, saved view, heap, member, invite link or team, when
    /// given.
    pub entity_id: Option<Uuid>,
    pub after: Option<AuditCursor>,
    /// Clamped to 1..=[`MAX_AUDIT_PAGE`].
    pub limit: u64,
}

impl Services {
    /// What happened in `heap_id` that the caller may see: changes to items, tags and saved views for every member,
    /// and to the heap and its parameters for admins too. Archived heaps keep their history.
    pub async fn list_heap_audit(
        &self,
        principal: Principal,
        heap_id: HeapId,
        request: AuditRequest,
        tz: &TimeZone,
    ) -> Result<AuditPage, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        let actions = visible_audit_actions(access.membership.role);
        // An entity's history follows it between heaps and teams (an item that moved), so it is read from every team the
        // caller is a member of, each limited to what their role there may see; the heap's own history is the heap's.
        let teams: Vec<(TeamId, Vec<AuditAction>)>;
        let (scope, actions) = match request.entity_id {
            Some(entity_id) => {
                teams = store::teams::list_teams_for_user(&self.db, principal.user_id)
                    .await?
                    .into_iter()
                    .map(|(team, role)| (team.id, visible_audit_actions(role)))
                    .collect();
                let mut any: Vec<AuditAction> = teams.iter().flat_map(|(_, actions)| actions.iter().copied()).collect();
                any.sort_by_key(|action| *action as usize);
                any.dedup();
                (
                    AuditScope::Entity {
                        entity_id,
                        teams: &teams,
                    },
                    any,
                )
            }
            None => (AuditScope::Heap(heap_id), actions),
        };
        let (entries, next) = self.read_page(scope, &actions, request).await?;

        let mut names = Names::default();
        self.insert_heap_entry_names(&mut names, &entries, Some((heap_id, access.heap.team_id)))
            .await?;
        self.insert_saved_view_names(&mut names, &entries, heap_id).await?;
        self.insert_user_names(&mut names, &entries).await?;
        let runs = self.find_audit_runs(principal.user_id, &entries).await?;
        Ok(present_page(entries, next, &names, &runs, tz))
    }

    /// Adds what entries made in heaps name: the heaps, their tags, parameters and options, and the items. `home` is
    /// the heap the history is read for, and its team, whose names are added whatever the entries are; its items are
    /// looked for first.
    async fn insert_heap_entry_names(
        &self,
        names: &mut Names,
        entries: &[AuditEntry],
        home: Option<(HeapId, TeamId)>,
    ) -> Result<(), ServiceError> {
        // An item's history follows it between heaps and teams, and so can a blocker, so what the entries name may be
        // in any heap of any team they come from: the heaps' names, their tags, parameters and options, and the items.
        // Size assumption: a team holds a handful of heaps, each with tens of tags and parameters, so all of them are
        // loaded on every read. If teams grow to many heaps, load only the heaps the page's entries name, including
        // those in `item_moved` diffs.
        let entry_teams: Vec<TeamId> = entries
            .iter()
            .map(|entry| entry.team_id)
            .chain(home.map(|(_, team)| team))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut team_heaps: Vec<HeapId> = home.iter().map(|(heap, _)| *heap).collect();
        for heap in store::heaps::list_heaps_for_teams(&self.db, &entry_teams).await? {
            names.insert(*heap.id.as_uuid(), heap.name.as_str());
            if home.is_none_or(|(home, _)| heap.id != home) {
                team_heaps.push(heap.id);
            }
        }
        for heap in &team_heaps {
            self.insert_heap_vocabulary(names, *heap).await?;
        }
        // The items the entries name, looked for in this heap and then in the others until all are found.
        let mut missing: Vec<ItemId> = collect_referenced_items(entries)
            .into_iter()
            .map(ItemId::from_uuid)
            .collect();
        for heap in team_heaps {
            if missing.is_empty() {
                break;
            }
            for item in store::items::list_item_summaries(&self.db, heap, missing.iter().copied()).await? {
                names.insert(*item.id.as_uuid(), item.title.as_str());
            }
            missing.retain(|item| names.get(*item.as_uuid()).is_none());
        }
        // An item that is in no heap of a team left it, and that team's log names it by the title it had then. A page
        // spanning teams may name an item that left any of them.
        for team in &entry_teams {
            if missing.is_empty() {
                break;
            }
            for (item, title) in store::audit::list_left_item_titles(&self.db, *team, &missing).await? {
                names.insert(*item.as_uuid(), &title);
            }
            missing.retain(|item| names.get(*item.as_uuid()).is_none());
        }
        Ok(())
    }

    /// Adds the names of the saved views of `heap_id` that entries are about.
    async fn insert_saved_view_names(
        &self,
        names: &mut Names,
        entries: &[AuditEntry],
        heap_id: HeapId,
    ) -> Result<(), ServiceError> {
        let views: Vec<SavedViewId> = collect_entity_ids(entries, EntityType::SavedView)
            .map(SavedViewId::from_uuid)
            .collect();
        for view in store::saved_views::list_saved_views_by_ids(&self.db, heap_id, &views).await? {
            names.insert(*view.id.as_uuid(), view.name.as_str());
        }
        Ok(())
    }

    /// What happened in the active heaps the caller may view, of all their teams or of `team` alone, newest first:
    /// in each heap what [`Self::list_heap_audit`] shows them there, by their role in its team. Each entry comes
    /// with its heap and team. `request.entity_id` is not read: one entity's history is its heap's to give. The
    /// caller must be in `team`: resolve it with [`resolve_team`](Self::resolve_team) first.
    ///
    /// Size assumption: as for a heap's history, the names of every heap of the teams on the page are loaded on
    /// each read, here over all the caller's teams.
    pub async fn list_audit_across_heaps(
        &self,
        principal: Principal,
        team: Option<TeamId>,
        request: AuditRequest,
        tz: &TimeZone,
    ) -> Result<AcrossHeapsAuditPage, ServiceError> {
        let mut viewable = self.list_viewable_heaps(principal).await?;
        if let Some(team) = team {
            viewable.retain(|viewable| viewable.team.id == team);
        }
        let mut groups: Vec<(TeamId, Vec<HeapId>, Vec<AuditAction>)> = Vec::new();
        for heap in &viewable {
            match groups.iter_mut().find(|(team, ..)| *team == heap.team.id) {
                Some((_, heaps, _)) => heaps.push(heap.heap.id),
                None => groups.push((heap.team.id, vec![heap.heap.id], visible_audit_actions(heap.role))),
            }
        }
        let mut any: Vec<AuditAction> = groups
            .iter()
            .flat_map(|(.., actions)| actions.iter().copied())
            .collect();
        any.sort_by_key(|action| *action as usize);
        any.dedup();
        let groups: Vec<(Vec<HeapId>, Vec<AuditAction>)> =
            groups.into_iter().map(|(_, heaps, actions)| (heaps, actions)).collect();
        let request = AuditRequest {
            entity_id: None,
            ..request
        };
        let (entries, next) = self.read_page(AuditScope::Heaps(&groups), &any, request).await?;

        let mut names = Names::default();
        self.insert_heap_entry_names(&mut names, &entries, None).await?;
        let heaps: BTreeSet<HeapId> = entries
            .iter()
            .filter(|entry| entry.entity_type == EntityType::SavedView)
            .filter_map(|entry| entry.heap_id)
            .collect();
        for heap in heaps {
            self.insert_saved_view_names(&mut names, &entries, heap).await?;
        }
        self.insert_user_names(&mut names, &entries).await?;
        let runs = self.find_audit_runs(principal.user_id, &entries).await?;
        // The scope reads entries of these heaps only, so each has its heap among them.
        let places: Vec<Option<(Heap, Team)>> = entries
            .iter()
            .map(|entry| {
                viewable
                    .iter()
                    .find(|viewable| Some(viewable.heap.id) == entry.heap_id)
                    .map(|viewable| (viewable.heap.clone(), viewable.team.clone()))
            })
            .collect();
        let page = present_page(entries, next, &names, &runs, tz);
        Ok(AcrossHeapsAuditPage {
            rows: page
                .entries
                .into_iter()
                .zip(places)
                .filter_map(|(entry, place)| {
                    let (heap, team) = place?;
                    Some(AcrossHeapsAuditRow { heap, team, entry })
                })
                .collect(),
            next: page.next,
        })
    }

    /// The runs `entries` were made in, with the item each worked on where `viewer` may view its heap.
    /// Size assumption: a page of up to 200 entries names a handful of runs and items, so each is read on its own. If
    /// pages ever name many, batch the item and heap lookups.
    async fn find_audit_runs(
        &self,
        viewer: UserId,
        entries: &[AuditEntry],
    ) -> Result<HashMap<RunId, AuditRun>, ServiceError> {
        let mut runs = HashMap::new();
        let mut items: HashMap<ItemId, Option<ItemOrigin>> = HashMap::new();
        for run_id in entries.iter().filter_map(|entry| entry.run_id) {
            if runs.contains_key(&run_id) {
                continue;
            }
            // The foreign key from the entry to its run rules out a missing one.
            let Some(run) = store::runs::find_run(&self.db, run_id).await? else {
                return Err(StoreError::corrupt("runs", &run_id.to_string(), "an audit entry's run is missing").into());
            };
            let item = match items.get(&run.item_id) {
                Some(item) => item.clone(),
                None => {
                    let item = find_viewable_item(&self.db, viewer, run.item_id).await?;
                    items.insert(run.item_id, item.clone());
                    item
                }
            };
            runs.insert(run_id, AuditRun { id: run_id, item });
        }
        Ok(runs)
    }

    /// Adds the names of the tags, parameters and options of `heap_id`.
    async fn insert_heap_vocabulary(&self, names: &mut Names, heap_id: HeapId) -> Result<(), ServiceError> {
        for tag in store::tags::list_tags_for_history(&self.db, heap_id).await? {
            names.insert(*tag.id.as_uuid(), tag.name.as_str());
        }
        insert_parameter_names(names, store::parameters::list_parameters(&self.db, heap_id).await?);
        Ok(())
    }

    /// What happened to `team_id` itself that the caller may see: its name and members, display names, and its
    /// heaps being created, renamed, archived and given a new context. What happens inside a heap is in
    /// [`Self::list_heap_audit`].
    pub async fn list_team_audit(
        &self,
        principal: Principal,
        team_id: TeamId,
        request: AuditRequest,
        tz: &TimeZone,
    ) -> Result<AuditPage, ServiceError> {
        let membership = authorize_team(&self.db, principal, team_id, Permission::ViewTeam).await?;
        let actions = visible_audit_actions(membership.role);
        let (entries, next) = self.read_page(AuditScope::Team(team_id), &actions, request).await?;

        let mut names = Names::default();
        if let Some(team) = store::teams::find_team(&self.db, team_id).await? {
            names.insert(*team.id.as_uuid(), team.name.as_str());
        }
        // A priority or default heap change sits in the person's personal team but names a heap of any of their
        // teams. Only the heaps of teams they're in now are named; the rest read as unknown.
        let teams = if entries.iter().any(|entry| {
            matches!(
                entry.action,
                AuditAction::HeapPriorityChanged | AuditAction::DefaultHeapChanged
            )
        }) {
            store::teams::list_teams_for_user(&self.db, principal.user_id)
                .await?
                .into_iter()
                .map(|(team, _)| team.id)
                .collect()
        } else {
            vec![team_id]
        };
        for heap in store::heaps::list_heaps_for_teams(&self.db, &teams).await? {
            names.insert(*heap.id.as_uuid(), heap.name.as_str());
        }
        for tag in store::tags::list_team_tags(&self.db, team_id).await? {
            names.insert(*tag.id.as_uuid(), tag.name.as_str());
        }
        insert_parameter_names(
            &mut names,
            store::parameters::list_team_parameters(&self.db, team_id).await?,
        );
        self.insert_user_names(&mut names, &entries).await?;
        self.insert_invite_link_names(&mut names, team_id, &entries).await?;
        let runs = self.find_audit_runs(principal.user_id, &entries).await?;
        Ok(present_page(entries, next, &names, &runs, tz))
    }

    /// Names the invite links `entries` are about after their makers' display names now: "Ana's invite link".
    async fn insert_invite_link_names(
        &self,
        names: &mut Names,
        team_id: TeamId,
        entries: &[AuditEntry],
    ) -> Result<(), ServiceError> {
        let about: BTreeSet<Uuid> = collect_entity_ids(entries, EntityType::InviteLink).collect();
        if about.is_empty() {
            return Ok(());
        }
        let links: Vec<_> = store::invite_links::list_invite_links(&self.db, team_id)
            .await?
            .into_iter()
            .filter(|link| about.contains(link.id.as_uuid()))
            .collect();
        let makers: Vec<UserId> = links.iter().map(|link| link.created_by).collect();
        let makers = store::users::list_users(&self.db, &makers).await?;
        for link in &links {
            if let Some(maker) = makers.iter().find(|user| user.id == link.created_by) {
                names.insert(
                    *link.id.as_uuid(),
                    &format!("{}'s invite link", maker.display_name.as_str()),
                );
            }
        }
        Ok(())
    }

    pub(crate) async fn read_page(
        &self,
        scope: AuditScope<'_>,
        actions: &[AuditAction],
        request: AuditRequest,
    ) -> Result<(Vec<AuditEntry>, Option<AuditCursor>), ServiceError> {
        let limit = request.limit.clamp(1, MAX_AUDIT_PAGE);
        // One extra row tells whether another page follows.
        let query = AuditQuery {
            scope,
            entity_id: request.entity_id,
            actions,
            after: request.after,
            limit: limit + 1,
        };
        let mut entries = store::audit::list_audit_entries(&self.db, query).await?;
        let has_more = entries.len() as u64 > limit;
        entries.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        let next = entries.last().filter(|_| has_more).map(|last| AuditCursor {
            created_at: last.created_at,
            id: last.id,
        });
        Ok((entries, next))
    }

    /// Adds the display names of the actors, and of the members entries are about.
    async fn insert_user_names(&self, names: &mut Names, entries: &[AuditEntry]) -> Result<(), ServiceError> {
        let ids: BTreeSet<UserId> = entries
            .iter()
            .map(|entry| entry.actor)
            .chain(collect_entity_ids(entries, EntityType::User).map(UserId::from_uuid))
            .collect();
        let ids: Vec<UserId> = ids.into_iter().collect();
        for user in store::users::list_users(&self.db, &ids).await? {
            names.insert(*user.id.as_uuid(), user.display_name.as_str());
        }
        Ok(())
    }
}

/// What things are called now, by ID. IDs are UUIDs, unique across every kind of entity, so one map holds them all.
#[derive(Default)]
struct Names(HashMap<Uuid, String>);

impl Names {
    fn insert(&mut self, id: Uuid, name: &str) {
        self.0.insert(id, name.to_owned());
    }

    fn get(&self, id: Uuid) -> Option<&str> {
        self.0.get(&id).map(String::as_str)
    }

    /// The name of the thing a diff value refers to, or `fallback` when the value isn't an ID of a known thing.
    fn of_value(&self, value: &Value, fallback: &str) -> String {
        value
            .as_str()
            .and_then(|raw| Uuid::parse_str(raw).ok())
            .and_then(|id| self.get(id))
            .unwrap_or(fallback)
            .to_owned()
    }
}

fn collect_entity_ids(entries: &[AuditEntry], entity_type: EntityType) -> impl Iterator<Item = Uuid> + '_ {
    entries
        .iter()
        .filter(move |entry| entry.entity_type == entity_type)
        .map(|entry| entry.entity_id)
}

/// The items entries are about, and the blockers their diffs name: the one picked, and the one whose closing as done
/// freed the item.
fn collect_referenced_items(entries: &[AuditEntry]) -> BTreeSet<Uuid> {
    let blockers = entries
        .iter()
        .filter(|entry| entry.entity_type == EntityType::Item)
        .flat_map(|entry| {
            [
                entry.diff.get(AuditField::BlockedBy.code()),
                entry.diff.get(AuditField::Origin.code()),
                entry.diff.get(AuditField::BlockerDone.code()),
            ]
        })
        .flatten()
        .flat_map(|value| [value.get("before"), value.get("after"), Some(value)])
        .flatten()
        .filter_map(|value| value.as_str().and_then(|raw| Uuid::parse_str(raw).ok()));
    collect_entity_ids(entries, EntityType::Item).chain(blockers).collect()
}

fn present_page(
    entries: Vec<AuditEntry>,
    next: Option<AuditCursor>,
    names: &Names,
    runs: &HashMap<RunId, AuditRun>,
    tz: &TimeZone,
) -> AuditPage {
    let entries = entries
        .into_iter()
        .map(|entry| {
            let about_actor = entry.entity_id == *entry.actor.as_uuid();
            let (phrase, names_entity) = describe_phrase(entry.action, entry.entity_type, about_actor, &entry.diff);
            // An operator's account change reads as about the account and doesn't name the operator.
            let actor = if is_operators_account_change(entry.action, &entry.diff) {
                UserId::from_uuid(entry.entity_id)
            } else {
                entry.actor
            };
            AuditEntryView {
                id: entry.id,
                created_at: entry.created_at,
                created_label: describe_time(entry.created_at, tz),
                actor,
                actor_name: names.get(*actor.as_uuid()).unwrap_or("Unknown user").to_owned(),
                action: entry.action,
                phrase,
                names_entity,
                entity_type: entry.entity_type,
                entity_id: entry.entity_id,
                entity_label: names.get(entry.entity_id).unwrap_or("Unknown").to_owned(),
                changes: describe_changes(&entry, names, tz),
                diff: entry.diff,
                run: entry.run_id.and_then(|id| runs.get(&id).cloned()),
            }
        })
        .collect();
    AuditPage { entries, next }
}

/// [`describe_action`], with the groups named for the two actions that come from the provider's groups.
fn describe_phrase(
    action: AuditAction,
    entity_type: EntityType,
    about_actor: bool,
    diff: &Value,
) -> (Cow<'static, str>, bool) {
    let (phrase, names_entity) = describe_action(action, about_actor);
    // A merge writes one entry on the team tag and one on each item it changed.
    if action == AuditAction::TagMerged && entity_type == EntityType::Item {
        return (Cow::Borrowed("merged a tag on"), names_entity);
    }
    // The same for a parameter made a team's: one entry on the parameter and one on each item whose value moved.
    if action == AuditAction::ParameterMerged && entity_type == EntityType::Item {
        return (Cow::Borrowed("merged a parameter on"), names_entity);
    }
    if action == AuditAction::ItemMoved {
        // A move across teams records one side of the heap change only (architecture.md, Audit log).
        let heap = diff.get(AuditField::Heap.code());
        let side = |name: &str| heap.and_then(|heap| heap.get(name)).is_some_and(Value::is_null);
        if side("after") {
            return (Cow::Borrowed("moved item to another team"), names_entity);
        }
        if side("before") {
            return (Cow::Borrowed("moved item here from another team"), names_entity);
        }
    }
    if action == AuditAction::ItemPlaced {
        let after = diff
            .get(AuditField::Placement.code())
            .and_then(|placement| placement.get("after"))
            .and_then(Value::as_str);
        return match after {
            Some("up") => (Cow::Borrowed("moved up item"), names_entity),
            Some("down") => (Cow::Borrowed("moved down item"), names_entity),
            _ => (Cow::Borrowed("reset the list place of item"), names_entity),
        };
    }
    if action == AuditAction::MemberRoleChanged
        && diff.get(AuditField::MadeBy.code()).and_then(Value::as_str) == Some("operator")
    {
        return (Cow::Borrowed("was made an admin by whoever runs the server"), false);
    }
    if is_operators_account_change(action, diff) {
        let phrase = match action {
            AuditAction::AccountDeactivated => "was turned off by whoever runs the server",
            AuditAction::AccountReactivated => "was turned back on by whoever runs the server",
            _ => "had a reset link made by whoever runs the server",
        };
        return (Cow::Borrowed(phrase), false);
    }
    let (lead, key) = match action {
        AuditAction::GroupMemberAdded => ("joined through", AuditField::Groups),
        AuditAction::GroupMemberRemoved => ("left: no longer in", AuditField::Groups),
        _ => return (Cow::Borrowed(phrase), names_entity),
    };
    let groups: Vec<&str> = diff
        .get(key.code())
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let noun = match groups.split_last() {
        Some((last, [])) => (*last).to_owned(),
        Some((last, first)) => format!("{} or {last}", first.join(", ")),
        None => return (Cow::Borrowed(phrase), names_entity),
    };
    (Cow::Owned(format!("{lead} the {noun} group")), names_entity)
}

/// Whether the entry is an operator turning an account off or on, or making it a reset link. Its actor is the
/// operator, or the person for the server's shell, and the entity is the account, which the line is about.
fn is_operators_account_change(action: AuditAction, diff: &Value) -> bool {
    matches!(
        action,
        AuditAction::AccountDeactivated | AuditAction::AccountReactivated | AuditAction::PasswordResetLinkMade
    ) && diff.get(AuditField::MadeBy.code()).and_then(Value::as_str) == Some("operator")
}

/// What `action` says, read after the actor's name, and whether the entity's name follows it. `about_actor` is
/// whether the entry is about the actor themselves, such as joining a team on creating it.
const fn describe_action(action: AuditAction, about_actor: bool) -> (&'static str, bool) {
    match (action, about_actor) {
        (AuditAction::UserRegistered, _) => ("signed up", false),
        (AuditAction::UserUpdated, true) => ("updated their profile", false),
        (AuditAction::UserUpdated, false) => ("updated the profile of", true),
        (AuditAction::TeamCreated, _) => ("created team", true),
        (AuditAction::TeamUpdated, _) => ("renamed team", true),
        (AuditAction::MemberAdded, true) => ("joined the team", false),
        (AuditAction::MemberAdded, false) => ("added member", true),
        (AuditAction::MemberRoleChanged, true) => ("changed their own role", false),
        (AuditAction::MemberRoleChanged, false) => ("changed the role of", true),
        (AuditAction::MemberRemoved, _) => ("removed member", true),
        // Only the member themselves leaves.
        (AuditAction::MemberLeft, _) => ("left the team", false),
        // The entry is about the link, so the line reads "Sam joined through Ana's invite link".
        (AuditAction::MemberJoined, _) => ("joined through", true),
        // The entry is about the person signing in; `describe_phrase` names the groups.
        (AuditAction::GroupMemberAdded, _) => ("joined through their groups", false),
        (AuditAction::GroupMemberRemoved, _) => ("left: no longer in their groups", false),
        (AuditAction::HeapCreated, _) => ("created heap", true),
        (AuditAction::HeapUpdated, _) => ("changed heap", true),
        (AuditAction::HeapContextUpdated, _) => ("changed the context of heap", true),
        (AuditAction::HeapArchived, _) => ("archived heap", true),
        (AuditAction::HeapUnarchived, _) => ("unarchived heap", true),
        (AuditAction::ItemCreated, _) => ("added item", true),
        (AuditAction::ItemUpdated, _) => ("edited item", true),
        (AuditAction::ItemMoved, _) => ("moved item", true),
        // `describe_phrase` says which way it was moved.
        (AuditAction::ItemPlaced, _) => ("reset the list place of item", true),
        (AuditAction::StepAdded, _) => ("added a step to", true),
        (AuditAction::StepUpdated, _) => ("changed a step of", true),
        (AuditAction::QuestionAsked, _) => ("asked a question on", true),
        (AuditAction::QuestionAnswered, _) => ("answered a question on", true),
        (AuditAction::ReplyChanged, _) => ("changed the reply to a question on", true),
        (AuditAction::QuestionWithdrawn, _) => ("withdrew a question on", true),
        (AuditAction::CommentAdded, _) => ("commented on", true),
        (AuditAction::TurnOpened, _) => ("opened a turn on", true),
        (AuditAction::TurnTakenBack, _) => ("took back a turn on", true),
        (AuditAction::TurnReached, _) => ("reached the goal of a turn on", true),
        (AuditAction::TurnHandedBack, _) => ("handed back a turn on", true),
        (AuditAction::TurnWatching, _) => ("started watching a pull request on", true),
        (AuditAction::TurnRetried, _) => ("let agents try again on", true),
        (AuditAction::TurnRedone, _) => ("asked for changes on", true),
        (AuditAction::TagCreated, _) => ("created tag", true),
        (AuditAction::TagUpdated, _) => ("edited tag", true),
        (AuditAction::TagMerged, _) => ("merged tags into", true),
        (AuditAction::SavedViewCreated, _) => ("saved view", true),
        (AuditAction::SavedViewUpdated, _) => ("changed view", true),
        (AuditAction::SavedViewArchived, _) => ("archived view", true),
        (AuditAction::SavedViewUnarchived, _) => ("unarchived view", true),
        (AuditAction::ParameterCreated, _) => ("added parameter", true),
        (AuditAction::ParameterUpdated, _) => ("changed parameter", true),
        (AuditAction::ParameterArchived, _) => ("archived parameter", true),
        (AuditAction::ParameterMerged, _) => ("made a team parameter of", true),
        (AuditAction::OptionArchived, _) => ("archived an option of", true),
        (AuditAction::InviteLinkCreated, _) => ("made an invite link", false),
        (AuditAction::InviteLinkRevoked, _) => ("revoked", true),
        (AuditAction::HeapPriorityChanged, _) => ("changed their priority for a heap", false),
        (AuditAction::DefaultHeapChanged, _) => ("changed their default heap", false),
        (AuditAction::PasswordChanged, _) => ("changed their password", false),
        (AuditAction::PasswordResetLinkMade, _) => ("got a reset link", false),
        (AuditAction::PasswordReset, _) => ("set a new password with a reset link", false),
        (AuditAction::AccountDeactivated, _) => ("was turned off by the sign-in provider", false),
        (AuditAction::AccountReactivated, _) => ("was turned back on by the sign-in provider", false),
        (AuditAction::EmailChangeAsked, _) => ("asked to change their email address", false),
        (AuditAction::EmailChanged, _) => ("changed their email address", false),
        (AuditAction::EmailConfirmed, _) => ("confirmed their email address", false),
        (AuditAction::OperatorGranted, _) => ("was made an operator", false),
        (AuditAction::OperatorRemoved, _) => ("was no longer an operator", false),
    }
}

/// How an action's diff is shaped (see `audit.rs`).
enum DiffShape {
    /// `{"field": value}`: the values something was created with.
    Created,
    /// `{"field": value}`: the values something had when it ended, such as a membership's role.
    Ended,
    /// `{"field": {"before": …, "after": …}}`, plus fields that only describe the change, such as
    /// `items_switched`.
    Changed,
}

const fn classify_diff(action: AuditAction) -> DiffShape {
    match action {
        AuditAction::UserRegistered
        | AuditAction::TeamCreated
        | AuditAction::MemberAdded
        | AuditAction::MemberJoined
        | AuditAction::GroupMemberAdded
        | AuditAction::HeapCreated
        | AuditAction::ItemCreated
        | AuditAction::StepAdded
        | AuditAction::QuestionAsked
        | AuditAction::QuestionAnswered
        | AuditAction::QuestionWithdrawn
        | AuditAction::CommentAdded
        | AuditAction::TurnOpened
        | AuditAction::TurnTakenBack
        | AuditAction::TurnHandedBack
        | AuditAction::TurnWatching
        | AuditAction::TurnRetried
        | AuditAction::TagCreated
        | AuditAction::SavedViewCreated
        | AuditAction::ParameterCreated
        | AuditAction::InviteLinkCreated
        | AuditAction::PasswordChanged
        | AuditAction::PasswordResetLinkMade
        | AuditAction::PasswordReset
        | AuditAction::AccountDeactivated
        | AuditAction::AccountReactivated
        | AuditAction::EmailChangeAsked
        | AuditAction::EmailConfirmed
        | AuditAction::OperatorGranted
        | AuditAction::OperatorRemoved => DiffShape::Created,
        AuditAction::MemberRemoved | AuditAction::MemberLeft | AuditAction::GroupMemberRemoved => DiffShape::Ended,
        AuditAction::UserUpdated
        | AuditAction::TeamUpdated
        | AuditAction::MemberRoleChanged
        | AuditAction::HeapUpdated
        | AuditAction::HeapContextUpdated
        | AuditAction::HeapArchived
        | AuditAction::HeapUnarchived
        | AuditAction::HeapPriorityChanged
        | AuditAction::DefaultHeapChanged
        | AuditAction::ItemUpdated
        | AuditAction::ItemMoved
        | AuditAction::ItemPlaced
        | AuditAction::TurnReached
        | AuditAction::TurnRedone
        | AuditAction::ReplyChanged
        | AuditAction::StepUpdated
        | AuditAction::TagUpdated
        | AuditAction::TagMerged
        | AuditAction::SavedViewUpdated
        | AuditAction::SavedViewArchived
        | AuditAction::SavedViewUnarchived
        | AuditAction::ParameterUpdated
        | AuditAction::ParameterArchived
        | AuditAction::ParameterMerged
        | AuditAction::OptionArchived
        | AuditAction::InviteLinkRevoked
        | AuditAction::EmailChanged => DiffShape::Changed,
    }
}

/// A diff value as the values before and after it, by the shape of the action's diff.
fn changed_values<'a>(shape: &DiffShape, value: &'a Value) -> (&'a Value, &'a Value) {
    match shape {
        DiffShape::Created => (&Value::Null, value),
        DiffShape::Ended => (value, &Value::Null),
        DiffShape::Changed => match (value.get("before"), value.get("after")) {
            (Some(before), Some(after)) => (before, after),
            _ => (&Value::Null, value),
        },
    }
}

fn describe_changes(entry: &AuditEntry, names: &Names, tz: &TimeZone) -> Vec<AuditChange> {
    // The phrase says it all ("made an invite link"), and the diff holds a raw end time and link states, which
    // have no short form worth showing.
    if entry.entity_type == EntityType::InviteLink {
        return Vec::new();
    }
    let Value::Object(fields) = &entry.diff else {
        return Vec::new();
    };
    // Declared fields in the order forms show them, then the ones no version declared, by name.
    let mut fields: Vec<(Option<AuditField>, &String, &Value)> = fields
        .iter()
        .map(|(key, value)| (AuditField::from_code(key), key, value))
        .collect();
    fields.sort_by_key(|(field, key, _)| (field.is_none(), field.map(|field| field as usize), key.as_str()));

    let shape = classify_diff(entry.action);
    let mut changes = Vec::new();
    for (field, key, value) in fields {
        let reading = field.map_or(
            Reading::Shown {
                form: Form::Plain,
                diffed: false,
            },
            |field| reading(field, entry.entity_type),
        );
        let (form, diffed) = match reading {
            Reading::Unshown => continue,
            Reading::PerParameter => {
                let (before, after) = changed_values(&shape, value);
                changes.extend(describe_parameter_values(before, after, names));
                continue;
            }
            Reading::Shown { form, diffed } => (form, diffed),
        };
        let (before, after) = changed_values(&shape, value);
        if has_no_value(before) && has_no_value(after) {
            continue;
        }
        let before = describe_value(form, before, names, tz);
        let after = describe_value(form, after, names, tz);
        let text_diff = match (&shape, &before, &after) {
            (DiffShape::Changed, Some(before), Some(after)) if diffed => diff_words(before, after),
            _ => None,
        };
        let change = AuditChange {
            field: key.clone(),
            label: field
                .map_or(key.as_str(), |field| describe_field(field, entry.entity_type))
                .to_owned(),
            before,
            after,
            text_diff,
        };
        // A created value with no short form, such as what a new saved view shows, would only say "changed".
        if matches!(shape, DiffShape::Created) && change.after.is_none() {
            continue;
        }
        changes.push(change);
    }
    changes
}

/// One change per parameter whose value differs, from maps of parameter ID to option ID.
fn describe_parameter_values(before: &Value, after: &Value, names: &Names) -> Vec<AuditChange> {
    let empty = Map::new();
    let before = before.as_object().unwrap_or(&empty);
    let after = after.as_object().unwrap_or(&empty);
    let parameters: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    parameters
        .into_iter()
        .filter(|parameter| before.get(*parameter) != after.get(*parameter))
        .map(|parameter| {
            let option = |values: &Map<String, Value>| {
                values
                    .get(parameter)
                    .filter(|value| !value.is_null())
                    .map(|value| names.of_value(value, "Unknown option"))
            };
            AuditChange {
                field: format!("parameters.{parameter}"),
                label: names.of_value(&Value::String(parameter.clone()), "Unknown parameter"),
                before: option(before),
                after: option(after),
                text_diff: None,
            }
        })
        .collect()
}

fn has_no_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        Value::Array(values) => values.is_empty(),
        Value::Object(fields) => fields.is_empty(),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

/// A field's label. An item's description is its context to people; a tag's stays a description.
const fn describe_field(field: AuditField, entity_type: EntityType) -> &'static str {
    match field {
        AuditField::Step | AuditField::QuestionStep => "Step",
        AuditField::StepTitle => "Step title",
        AuditField::StepContext => "Step context",
        AuditField::StepState => "Step state",
        AuditField::StepPlace => "Step place",
        AuditField::Placement => "Place in the list",
        AuditField::Question => "Question",
        AuditField::QuestionKind | AuditField::Kind => "Kind",
        AuditField::QuestionText => "Question text",
        AuditField::QuestionDetails => "Details",
        AuditField::QuestionChoices => "Choices",
        AuditField::QuestionMany => "Pick any",
        AuditField::QuestionLink => "Link",
        AuditField::QuestionPiece => "Piece",
        AuditField::QuestionRound => "Round",
        AuditField::Comment | AuditField::ReplyComment => "Comment",
        AuditField::CommentText => "Comment text",
        AuditField::Reply => "Reply",
        AuditField::Reach => "Reach",
        AuditField::Goal => "Goal",
        AuditField::Line => "Line",
        AuditField::Source => "Source",
        AuditField::Why => "Why",
        AuditField::How => "How",
        AuditField::Reason => "Reason",
        AuditField::BlockerDone => "Blocker done",
        AuditField::AgentsPrepare => "Agents prepare new notes",
        AuditField::Title => "Title",
        AuditField::Heap => "Heap",
        // Never shown (see `reading`).
        AuditField::ItemTitle => field.code(),
        AuditField::Key => "Key",
        AuditField::Stage => "Stage",
        AuditField::Name => "Name",
        AuditField::DisplayName => "Display name",
        AuditField::Description if matches!(entity_type, EntityType::Item) => "Context",
        AuditField::Description => "Description",
        AuditField::Color => "Colour",
        AuditField::Context => "Context",
        AuditField::State => "State",
        AuditField::BlockedBy => "Blocked by",
        AuditField::Origin => "Made from",
        AuditField::BlockedReason => "Blocked because",
        AuditField::Need => "Need",
        AuditField::Effort => "Effort",
        AuditField::Impact => "Impact",
        AuditField::Due => "Due",
        AuditField::Tags => "Tags",
        AuditField::Role => "Role",
        AuditField::Weight => "Weight",
        AuditField::NeedWeight => "Need weight",
        AuditField::AgeWeight => "Age weight",
        AuditField::DeadlineWeight => "Deadline weight",
        AuditField::ValueWeight => "Value per effort weight",
        AuditField::TagsWeight => "Tags weight",
        AuditField::TagOrder => "Tag order",
        AuditField::Options => "Options",
        AuditField::ArchivedOptions => "Archived options",
        AuditField::ItemsSwitched => "Items switched",
        AuditField::EndedSignIns => "Other sign-ins ended",
        AuditField::ListView => "What it shows",
        AuditField::Slug => "Link name",
        AuditField::Username => "Username",
        AuditField::Priority => "Priority",
        AuditField::HeapId => "Heap",
        AuditField::MadeBy => "Made by",
        AuditField::Email => "Email",
        AuditField::Url => "Pull request",
        AuditField::Used => "Used in the heap",
        // Never shown as changes of their own (see `reading`): the phrase names the groups.
        AuditField::Groups => "Groups",
        AuditField::By => "By",
        AuditField::EndsAt => "Ends",
        AuditField::Merged => "Merged",
        // Never shown as one change of their own (see `reading`).
        AuditField::ArchivedAt | AuditField::Parameters => field.code(),
    }
}

/// How a field's value reads in the log.
enum Reading {
    /// Left out: the action already says it, as with `archived_at` for an archiving.
    Unshown,
    /// One change per parameter whose value differs, from maps of parameter ID to option ID.
    PerParameter,
    /// One change, its values read as `form`; with a word diff when `diffed` and it was edited.
    Shown { form: Form, diffed: bool },
}

/// How a value reads as text.
#[derive(Clone, Copy)]
enum Form {
    /// A string as it is, a number, yes or no, or other JSON as JSON.
    Plain,
    /// The label of a coded value, or none for a code no version wrote.
    Coded(fn(&str) -> Option<&'static str>),
    /// The words for a coded value, or the code as it is when it has none.
    Worded(fn(&str) -> Option<&'static str>),
    /// A step's place: the number of the step it follows, or 0 for first.
    Place,
    /// A step, named by its title (its number is in the diff for API clients).
    Step,
    /// A question or comment, by its number after a prefix: `q2`, `c3`.
    Numbered(char),
    /// A list of strings, joined.
    Joined,
    /// A recorded due date.
    Due,
    /// The name of the thing an ID refers to now, or the fallback.
    Named(&'static str),
    /// Tag IDs, as tag names.
    Tags,
    /// Options, by their labels.
    OptionLabels,
    /// A count of items: "1 item", "2 items".
    ItemCount,
    /// Heap tags merged into a team tag: each heap's tag as it wrote it, and its items.
    MergedTags,
    /// Heap parameters made a team parameter: each heap's parameter as it wrote it, its items and the options it added.
    MergedParameters,
    /// No short form, as a whole list view: the label says it changed.
    NoShort,
}

/// How `field`'s value reads in the log. The texts people edit, an item's title and context, a step's, and a heap's
/// context, show an edit as a word diff; other text, such as a tag's name, shows in full.
fn reading(field: AuditField, entity_type: EntityType) -> Reading {
    let shown = |form| Reading::Shown { form, diffed: false };
    let coded = |label| shown(Form::Coded(label));
    match field {
        // `ends_at` is only on an invite link's creation, whose entries show no changes.
        AuditField::ArchivedAt | AuditField::EndsAt => Reading::Unshown,
        // Group membership entries say the groups in their phrase, and `by` is always the provider's groups.
        AuditField::Groups | AuditField::By => Reading::Unshown,
        // A heap priority change, the only entry about a user with a name, records the heap's for account history;
        // its `heap_id` already names the heap here.
        AuditField::Name if entity_type == EntityType::User => Reading::Unshown,
        // The name the log of the team an item left keeps for it; the phrase and the heap say the rest.
        AuditField::ItemTitle => Reading::Unshown,
        // The phrase says which way the item was placed.
        AuditField::Placement => Reading::Unshown,
        AuditField::Parameters => Reading::PerParameter,
        AuditField::Kind => coded(|code| Kind::from_code(code).map(Kind::label)),
        AuditField::Stage => coded(|code| Stage::from_code(code).map(Stage::label)),
        AuditField::State => coded(|code| StateKind::from_code(code).map(StateKind::label)),
        AuditField::StepState => coded(|code| StepState::from_code(code).map(StepState::label)),
        AuditField::Reach => coded(|code| Reach::from_code(code).map(Reach::label)),
        AuditField::Goal => coded(|code| Goal::from_code(code).map(Goal::label)),
        AuditField::Source => coded(|code| TurnSource::from_code(code).map(TurnSource::label)),
        AuditField::Why => coded(|code| TakeBackWhy::from_code(code).map(TakeBackWhy::label)),
        AuditField::How => coded(describe_how),
        AuditField::MadeBy => shown(Form::Worded(describe_made_by)),
        AuditField::QuestionKind => coded(|code| QuestionKind::from_code(code).map(QuestionKind::label)),
        AuditField::Need => coded(|code| Need::from_code(code).map(Need::label)),
        AuditField::Effort | AuditField::Impact => coded(|code| Size::from_code(code).map(Size::label)),
        AuditField::Color => coded(|code| TagColor::from_code(code).map(TagColor::label)),
        AuditField::Role => coded(|code| Role::from_code(code).map(Role::label)),
        AuditField::StepPlace => shown(Form::Place),
        AuditField::Step => shown(Form::Step),
        AuditField::Question => shown(Form::Numbered('q')),
        AuditField::Comment => shown(Form::Numbered('c')),
        AuditField::QuestionChoices => shown(Form::Joined),
        AuditField::Due => shown(Form::Due),
        AuditField::BlockedBy | AuditField::Origin | AuditField::BlockerDone => shown(Form::Named("Unknown item")),
        AuditField::Heap | AuditField::HeapId => shown(Form::Named("Unknown heap")),
        AuditField::Tags | AuditField::TagOrder => shown(Form::Tags),
        AuditField::Options | AuditField::ArchivedOptions => shown(Form::OptionLabels),
        AuditField::ItemsSwitched => shown(Form::ItemCount),
        // Only on a `TagMerged` entry, or a parameter's `ParameterMerged`: what the merge took in.
        AuditField::Merged if entity_type == EntityType::Parameter => shown(Form::MergedParameters),
        AuditField::Merged => shown(Form::MergedTags),
        AuditField::ListView => shown(Form::NoShort),
        AuditField::Title | AuditField::Description | AuditField::StepTitle | AuditField::StepContext => {
            Reading::Shown {
                form: Form::Plain,
                diffed: entity_type == EntityType::Item,
            }
        }
        AuditField::Context => Reading::Shown {
            form: Form::Plain,
            diffed: entity_type == EntityType::Heap,
        },
        AuditField::QuestionText
        | AuditField::QuestionDetails
        | AuditField::QuestionMany
        | AuditField::QuestionStep
        | AuditField::QuestionLink
        | AuditField::QuestionPiece
        | AuditField::QuestionRound
        | AuditField::CommentText
        | AuditField::Reply
        | AuditField::ReplyComment
        | AuditField::Line
        | AuditField::Reason
        | AuditField::Key
        | AuditField::Name
        | AuditField::DisplayName
        | AuditField::BlockedReason
        | AuditField::Weight
        | AuditField::Used
        | AuditField::NeedWeight
        | AuditField::AgeWeight
        | AuditField::DeadlineWeight
        | AuditField::ValueWeight
        | AuditField::TagsWeight
        | AuditField::EndedSignIns
        | AuditField::AgentsPrepare
        | AuditField::Priority
        | AuditField::Slug
        | AuditField::Username
        | AuditField::Email
        | AuditField::Url => shown(Form::Plain),
    }
}

/// A diff value as text, or `None` when there was no value or it has no short form.
fn describe_value(form: Form, value: &Value, names: &Names, tz: &TimeZone) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let text = match form {
        Form::Plain => match value {
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            Value::Bool(true) => "yes".to_owned(),
            Value::Bool(false) => "no".to_owned(),
            other => other.to_string(),
        },
        Form::Coded(label) => label(value.as_str()?)?.to_owned(),
        Form::Worded(words) => {
            let code = value.as_str()?;
            words(code).unwrap_or(code).to_owned()
        }
        Form::Place => match value.as_u64()? {
            0 => "first".to_owned(),
            number => format!("after step {number}"),
        },
        Form::Step => value.get("title")?.as_str()?.to_owned(),
        Form::Numbered(prefix) => format!("{prefix}{}", value.as_u64()?),
        Form::Joined => join(value.as_array()?.iter().filter_map(Value::as_str).map(str::to_owned))?,
        Form::Due => describe_recorded_due(value, tz)?,
        Form::Named(fallback) => names.of_value(value, fallback),
        Form::Tags => join(value.as_array()?.iter().map(|tag| names.of_value(tag, "Unknown tag")))?,
        Form::OptionLabels => join(
            value
                .as_array()?
                .iter()
                .filter_map(|option| option.get("label").and_then(Value::as_str))
                .map(str::to_owned),
        )?,
        Form::ItemCount => count_items(value.as_array()?.len()),
        Form::MergedTags => join(value.as_array()?.iter().map(|merged| {
            let heap = names.of_value(&merged["heap"], "Unknown heap");
            let name = merged["name"].as_str().unwrap_or_default();
            let items = merged["items"].as_u64().unwrap_or_default();
            format!("{heap}'s \u{201c}{name}\u{201d} ({})", count_items(items as usize))
        }))?,
        Form::MergedParameters => join(value.as_array()?.iter().map(|merged| {
            let heap = names.of_value(&merged["heap"], "Unknown heap");
            let name = merged["name"].as_str().unwrap_or_default();
            let items = count_items(merged["items"].as_u64().unwrap_or_default() as usize);
            let added: Vec<&str> = merged["added"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            if added.is_empty() {
                format!("{heap}'s \u{201c}{name}\u{201d} ({items})")
            } else {
                format!("{heap}'s \u{201c}{name}\u{201d} ({items}, adds {})", added.join(", "))
            }
        }))?,
        Form::NoShort => return None,
    };
    Some(text)
}

fn count_items(count: usize) -> String {
    match count {
        1 => "1 item".to_owned(),
        n => format!("{n} items"),
    }
}

fn join(values: impl Iterator<Item = String>) -> Option<String> {
    let values: Vec<String> = values.collect();
    (!values.is_empty()).then(|| values.join(", "))
}

fn describe_recorded_due(value: &Value, tz: &TimeZone) -> Option<String> {
    let at = Timestamp::from_millis(value.get("at")?.as_i64()?)?;
    let firm = value.get("firm")?.as_bool()?;
    Some(describe_due_date(Due { at, firm }, tz))
}

/// Who made a reset link, as the entry's `made_by` records it (see `password_reset.rs`), in account history's words.
fn describe_made_by(code: &str) -> Option<&'static str> {
    match code {
        "operator" => Some("Whoever runs the server"),
        "mail" => Some("Mail, asked on the sign-in page"),
        _ => None,
    }
}

/// How a turn came to be reached without a run saying so, as the entry's `how` records it.
fn describe_how(code: &str) -> Option<&'static str> {
    match code {
        "stage_set" => Some("Stage set by hand"),
        "already_met" => Some("Already at the goal"),
        "became" => Some("Defined when its note became"),
        _ => None,
    }
}

/// Adds the names of `parameters` and their options, in use or archived.
fn insert_parameter_names(names: &mut Names, parameters: Vec<domain::parameter::Parameter>) {
    for parameter in parameters {
        names.insert(*parameter.id.as_uuid(), parameter.name.as_str());
        for option in &parameter.options {
            names.insert(*option.id.as_uuid(), option.label.as_str());
        }
        for option in &parameter.archived_options {
            names.insert(*option.id.as_uuid(), option.label.as_str());
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::text_diff::{DiffPiece, DiffPieceKind};
    use googletest::prelude::*;
    use serde_json::json;

    use super::*;

    fn piece(kind: DiffPieceKind, text: &str) -> DiffPiece {
        DiffPiece {
            kind,
            text: text.to_owned(),
        }
    }

    fn entry(action: AuditAction, entity_type: EntityType, diff: Value) -> AuditEntry {
        AuditEntry {
            id: Uuid::from_u128(1),
            team_id: TeamId::from_uuid(Uuid::from_u128(2)),
            heap_id: None,
            actor: UserId::from_uuid(Uuid::from_u128(3)),
            action,
            entity_type,
            entity_id: Uuid::from_u128(4),
            diff,
            run_id: None,
            created_at: Timestamp::EPOCH,
        }
    }

    fn change(field: &str, label: &str, before: Option<&str>, after: Option<&str>) -> AuditChange {
        AuditChange {
            field: field.to_owned(),
            label: label.to_owned(),
            before: before.map(str::to_owned),
            after: after.map(str::to_owned),
            text_diff: None,
        }
    }

    #[gtest]
    fn an_item_edit_reads_in_form_order_with_names_for_ids() {
        let (tag, parameter, low, high) = (
            Uuid::from_u128(10),
            Uuid::from_u128(11),
            Uuid::from_u128(12),
            Uuid::from_u128(13),
        );
        let mut names = Names::default();
        names.insert(tag, "backend");
        names.insert(parameter, "Customer");
        names.insert(low, "Small");
        names.insert(high, "Big");
        let diff = json!({
            "tags": { "before": [], "after": [tag.to_string()] },
            "state": { "before": "open", "after": "in_progress" },
            "parameters": { "before": { parameter.to_string(): low.to_string() }, "after": { parameter.to_string(): high.to_string() } },
            "title": { "before": "Old", "after": "New" },
        });

        let changes = describe_changes(
            &entry(AuditAction::ItemUpdated, EntityType::Item, diff),
            &names,
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![
                eq(&change("title", "Title", Some("Old"), Some("New"))),
                eq(&change("state", "State", Some("Open"), Some("In progress"))),
                eq(&change("tags", "Tags", None, Some("backend"))),
                eq(&change(
                    &format!("parameters.{parameter}"),
                    "Customer",
                    Some("Small"),
                    Some("Big")
                )),
            ]
        );
    }

    #[gtest]
    fn a_creation_lists_the_values_it_set_and_skips_empty_ones() {
        let diff = json!({
            "title": "Write docs", "description": null, "tags": [], "parameters": {},
            "effort": "m", "due": { "at": 1_798_891_200_000_i64, "firm": false },
        });

        let changes = describe_changes(
            &entry(AuditAction::ItemCreated, EntityType::Item, diff),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![
                eq(&change("title", "Title", None, Some("Write docs"))),
                eq(&change("effort", "Effort", None, Some("M"))),
                eq(&change("due", "Due", None, Some("around 2 Jan 2027"))),
            ]
        );
    }

    #[gtest]
    fn an_ended_membership_shows_the_role_it_had_and_archiving_shows_no_timestamp() {
        let left = describe_changes(
            &entry(AuditAction::MemberLeft, EntityType::User, json!({ "role": "admin" })),
            &Names::default(),
            &TimeZone::UTC,
        );
        let archived = describe_changes(
            &entry(
                AuditAction::HeapArchived,
                EntityType::Heap,
                json!({ "archived_at": { "before": null, "after": 5 } }),
            ),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(left, elements_are![eq(&change("role", "Role", Some("Admin"), None))]);
        expect_that!(archived, is_empty());
    }

    #[gtest]
    fn an_option_archiving_counts_the_switched_items() {
        let diff = json!({
            "options": { "before": [{ "id": "a", "label": "Low" }, { "id": "b", "label": "High" }], "after": [{ "id": "b", "label": "High" }] },
            "items_switched": ["x", "y"],
        });

        let changes = describe_changes(
            &entry(AuditAction::OptionArchived, EntityType::Parameter, diff),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![
                eq(&change("options", "Options", Some("Low, High"), Some("High"))),
                eq(&change("items_switched", "Items switched", None, Some("2 items"))),
            ]
        );
    }

    #[gtest]
    fn a_weights_save_labels_the_heap_weights_that_changed() {
        let diff = json!({
            "value_weight": { "before": 10, "after": 0 },
            "need_weight": { "before": 10, "after": 30 },
        });

        let changes = describe_changes(
            &entry(AuditAction::HeapUpdated, EntityType::Heap, diff),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![
                eq(&change("need_weight", "Need weight", Some("10"), Some("30"))),
                eq(&change(
                    "value_weight",
                    "Value per effort weight",
                    Some("10"),
                    Some("0")
                )),
            ]
        );
    }

    #[gtest]
    fn a_new_saved_view_shows_its_name_but_not_a_bare_changed_line() {
        let diff = json!({ "name": "Blocked", "list_view": { "blocked": "shown" } });

        let changes = describe_changes(
            &entry(AuditAction::SavedViewCreated, EntityType::SavedView, diff),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![eq(&change("name", "Name", None, Some("Blocked")))]
        );
    }

    #[gtest]
    fn an_entry_about_the_actor_does_not_name_them_again() {
        expect_that!(
            describe_action(AuditAction::UserUpdated, true),
            eq(("updated their profile", false))
        );
        expect_that!(
            describe_action(AuditAction::MemberAdded, true),
            eq(("joined the team", false))
        );
        expect_that!(
            describe_action(AuditAction::MemberAdded, false),
            eq(("added member", true))
        );
        expect_that!(
            describe_action(AuditAction::MemberJoined, false),
            eq(("joined through", true))
        );
    }

    #[gtest]
    fn a_step_added_shows_its_title_and_context() {
        let diff = json!({ "step": { "number": 3, "title": "Run in staging" }, "step_context": "Use the blue stack" });

        let changes = describe_changes(
            &entry(AuditAction::StepAdded, EntityType::Item, diff),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![
                eq(&change("step", "Step", None, Some("Run in staging"))),
                eq(&change(
                    "step_context",
                    "Step context",
                    None,
                    Some("Use the blue stack")
                )),
            ]
        );
        expect_that!(
            describe_action(AuditAction::StepAdded, false),
            eq(("added a step to", true))
        );
    }

    #[gtest]
    fn a_step_change_names_the_step_first_then_what_changed() {
        let diff = json!({
            "step_state": { "before": "open", "after": "ticked" },
            "step_title": { "before": "Run in staging", "after": "Run in prod" },
            "step": { "number": 3, "title": "Run in prod" },
            "step_context": { "before": "old", "after": null },
        });

        let changes = describe_changes(
            &entry(AuditAction::StepUpdated, EntityType::Item, diff),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![
                eq(&change("step", "Step", None, Some("Run in prod"))),
                eq(&AuditChange {
                    text_diff: Some(WordDiff {
                        pieces: vec![
                            piece(DiffPieceKind::Same, "Run in "),
                            piece(DiffPieceKind::Removed, "staging"),
                            piece(DiffPieceKind::Added, "prod"),
                        ],
                        spacing_only: false,
                    }),
                    ..change("step_title", "Step title", Some("Run in staging"), Some("Run in prod"))
                }),
                eq(&change("step_context", "Step context", Some("old"), None)),
                eq(&change("step_state", "Step state", Some("Open"), Some("Ticked"))),
            ]
        );
        expect_that!(
            describe_action(AuditAction::StepUpdated, false),
            eq(("changed a step of", true))
        );
    }

    #[gtest]
    fn a_moved_step_shows_the_step_it_now_follows() {
        let diff = json!({
            "step": { "number": 3, "title": "Run in prod" },
            "step_place": { "before": 2, "after": 0 },
        });

        let changes = describe_changes(
            &entry(AuditAction::StepUpdated, EntityType::Item, diff),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![
                eq(&change("step", "Step", None, Some("Run in prod"))),
                eq(&change("step_place", "Step place", Some("after step 2"), Some("first"))),
            ]
        );
    }

    #[gtest]
    fn an_edited_context_comes_with_a_word_diff() {
        let diff = json!({
            "description": { "before": "Ship it on Friday.\nTell the team.", "after": "Ship it on Monday.\nTell the team." },
            "title": { "before": "Launch", "after": "Launch day" },
        });

        let changes = describe_changes(
            &entry(AuditAction::ItemUpdated, EntityType::Item, diff),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![
                field!(
                    AuditChange.text_diff,
                    some(field!(
                        WordDiff.pieces,
                        elements_are![
                            eq(&piece(DiffPieceKind::Same, "Launch")),
                            eq(&piece(DiffPieceKind::Added, " day")),
                        ]
                    ))
                ),
                field!(
                    AuditChange.text_diff,
                    some(field!(
                        WordDiff.pieces,
                        elements_are![
                            eq(&piece(DiffPieceKind::Same, "Ship it on ")),
                            eq(&piece(DiffPieceKind::Removed, "Friday.")),
                            eq(&piece(DiffPieceKind::Added, "Monday.")),
                            eq(&piece(DiffPieceKind::Same, "\nTell the team.")),
                        ]
                    ))
                ),
            ]
        );
    }

    #[gtest]
    fn a_heaps_edited_context_comes_with_a_word_diff() {
        let diff =
            json!({ "context": { "before": "Planning for the web app", "after": "Planning for the whole web app" } });

        let changes = describe_changes(
            &entry(AuditAction::HeapContextUpdated, EntityType::Heap, diff),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![field!(
                AuditChange.text_diff,
                some(field!(
                    WordDiff.pieces,
                    contains(eq(&piece(DiffPieceKind::Added, "whole ")))
                ))
            )]
        );
    }

    #[gtest]
    fn a_change_that_cant_be_diffed_shows_in_full() {
        let item_edit = json!({
            // Set from nothing, cleared to nothing, and rewritten: nothing to diff, or too little kept.
            "description": { "before": null, "after": "A new context" },
            "step_context": { "before": "Old notes here", "after": null },
            "title": { "before": "Fix the login page", "after": "Write the release notes" },
            "state": { "before": "open", "after": "in_progress" },
        });
        let tag_edit = json!({ "name": { "before": "back end work", "after": "back end" } });
        let item_created = json!({ "title": "Fix the login page" });

        let changes: Vec<AuditChange> = [
            entry(AuditAction::ItemUpdated, EntityType::Item, item_edit),
            entry(AuditAction::TagUpdated, EntityType::Tag, tag_edit),
            entry(AuditAction::ItemCreated, EntityType::Item, item_created),
        ]
        .iter()
        .flat_map(|entry| describe_changes(entry, &Names::default(), &TimeZone::UTC))
        .collect();

        expect_that!(changes, len(eq(6)));
        expect_that!(changes, each(field!(AuditChange.text_diff, none())));
    }

    #[gtest]
    fn a_tag_merge_shows_the_heap_tags_it_took_in() {
        let launch = Uuid::from_u128(20);
        let mut names = Names::default();
        names.insert(launch, "Launch");
        let merged = json!({ "merged": [
            { "heap": launch.to_string(), "tag": Uuid::from_u128(21).to_string(), "name": "billing", "description": null, "items": 1 },
            { "heap": Uuid::from_u128(22).to_string(), "tag": Uuid::from_u128(23).to_string(), "name": "Billing", "description": null, "items": 3 },
        ] });

        let changes = describe_changes(
            &entry(AuditAction::TagMerged, EntityType::Tag, merged),
            &names,
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![eq(&change(
                "merged",
                "Merged",
                None,
                Some("Launch's \u{201c}billing\u{201d} (1 item), Unknown heap's \u{201c}Billing\u{201d} (3 items)")
            ))]
        );
    }

    #[gtest]
    fn a_parameter_merge_shows_the_parameters_it_took_in_and_what_they_added() {
        let launch = Uuid::from_u128(20);
        let mut names = Names::default();
        names.insert(launch, "Launch");
        let merged = json!({ "merged": [
            { "heap": launch.to_string(), "parameter": Uuid::from_u128(21).to_string(), "name": "Customer tier", "weight": 30, "items": 12, "added": [] },
            { "heap": Uuid::from_u128(22).to_string(), "parameter": Uuid::from_u128(23).to_string(), "name": "customer tier", "weight": 5, "items": 1, "added": ["Platinum", "Bronze"] },
        ] });

        let changes = describe_changes(
            &entry(AuditAction::ParameterMerged, EntityType::Parameter, merged),
            &names,
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![eq(&change(
                "merged",
                "Merged",
                None,
                Some(
                    "Launch's \u{201c}Customer tier\u{201d} (12 items), Unknown heap's \u{201c}customer tier\u{201d} (1 item, adds Platinum, Bronze)"
                )
            ))]
        );
    }

    #[gtest]
    fn a_new_team_or_heap_and_a_rename_show_the_link_name() {
        let created = describe_changes(
            &entry(
                AuditAction::TeamCreated,
                EntityType::Team,
                json!({ "name": "Ops", "slug": "ops" }),
            ),
            &Names::default(),
            &TimeZone::UTC,
        );
        let renamed = describe_changes(
            &entry(
                AuditAction::HeapUpdated,
                EntityType::Heap,
                json!({ "slug": { "before": "ops", "after": "infra" } }),
            ),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(created, contains(eq(&change("slug", "Link name", None, Some("ops")))));
        expect_that!(
            renamed,
            elements_are![eq(&change("slug", "Link name", Some("ops"), Some("infra")))]
        );
    }

    #[gtest]
    fn a_sign_up_shows_the_username() {
        let changes = describe_changes(
            &entry(
                AuditAction::UserRegistered,
                EntityType::User,
                json!({ "username": "ada" }),
            ),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![eq(&change("username", "Username", None, Some("ada")))]
        );
    }

    #[gtest]
    fn a_priority_change_names_the_heap_and_shows_the_priority_before_and_after() {
        let heap = Uuid::from_u128(20);
        let mut names = Names::default();
        names.insert(heap, "Launch");
        // The heap's name as recorded stays out: `heap_id` names the heap once.
        let diff = json!({ "heap_id": heap.to_string(), "name": "Launch", "priority": { "before": 1, "after": 7 } });

        let named = describe_changes(
            &entry(AuditAction::HeapPriorityChanged, EntityType::User, diff.clone()),
            &names,
            &TimeZone::UTC,
        );
        let unknown = describe_changes(
            &entry(AuditAction::HeapPriorityChanged, EntityType::User, diff),
            &Names::default(),
            &TimeZone::UTC,
        );

        expect_that!(
            named,
            unordered_elements_are![
                eq(&change("heap_id", "Heap", None, Some("Launch"))),
                eq(&change("priority", "Priority", Some("1"), Some("7"))),
            ]
        );
        expect_that!(
            unknown,
            contains(eq(&change("heap_id", "Heap", None, Some("Unknown heap"))))
        );
    }

    #[gtest]
    fn a_default_heap_change_names_the_heap_before_and_after() {
        let (old, new) = (Uuid::from_u128(20), Uuid::from_u128(21));
        let mut names = Names::default();
        names.insert(old, "Launch");
        names.insert(new, "Garden");
        let diff = json!({ "heap_id": { "before": old.to_string(), "after": new.to_string() } });

        let changes = describe_changes(
            &entry(AuditAction::DefaultHeapChanged, EntityType::User, diff),
            &names,
            &TimeZone::UTC,
        );

        expect_that!(
            changes,
            elements_are![eq(&change("heap_id", "Heap", Some("Launch"), Some("Garden")))]
        );
    }

    #[gtest]
    fn a_group_membership_names_the_groups_in_its_phrase() {
        let phrase = |action, groups: Value| {
            describe_phrase(
                action,
                EntityType::User,
                true,
                &json!({ "groups": groups, "by": "provider_groups" }),
            )
        };

        expect_that!(
            phrase(AuditAction::GroupMemberAdded, json!(["engineering"])),
            eq(&(Cow::Borrowed("joined through the engineering group"), false))
        );
        expect_that!(
            phrase(AuditAction::GroupMemberRemoved, json!(["engineering", "sre"])),
            eq(&(Cow::Borrowed("left: no longer in the engineering or sre group"), false))
        );
        expect_that!(
            phrase(AuditAction::GroupMemberAdded, json!(["a", "b", "c"])).0,
            eq("joined through the a, b or c group")
        );
    }

    #[gtest]
    fn an_operator_made_admin_reads_as_such_not_as_a_change_of_their_own_role() {
        let diff = json!({ "role": { "before": "member", "after": "admin" }, "made_by": "operator" });

        expect_that!(
            describe_phrase(AuditAction::MemberRoleChanged, EntityType::User, true, &diff),
            eq(&(Cow::Borrowed("was made an admin by whoever runs the server"), false))
        );
        expect_that!(
            describe_phrase(
                AuditAction::MemberRoleChanged,
                EntityType::User,
                true,
                &json!({ "role": {} })
            ),
            eq(&(Cow::Borrowed("changed their own role"), false))
        );
    }

    #[gtest]
    fn a_group_membership_shows_no_change_rows_but_the_role_it_ended() {
        let added = entry(
            AuditAction::GroupMemberAdded,
            EntityType::User,
            json!({ "groups": ["engineering"], "by": "provider_groups" }),
        );
        let removed = entry(
            AuditAction::GroupMemberRemoved,
            EntityType::User,
            json!({ "role": "admin", "groups": ["engineering"], "by": "provider_groups" }),
        );

        expect_that!(describe_changes(&added, &Names::default(), &TimeZone::UTC), is_empty());
        expect_that!(
            describe_changes(&removed, &Names::default(), &TimeZone::UTC),
            elements_are![eq(&change("role", "Role", Some("Admin"), None))]
        );
    }

    #[gtest]
    fn a_reset_link_says_who_made_it_in_words() {
        let made_by = |code: &str| {
            describe_changes(
                &entry(
                    AuditAction::PasswordResetLinkMade,
                    EntityType::User,
                    json!({ "made_by": code }),
                ),
                &Names::default(),
                &TimeZone::UTC,
            )
        };

        expect_that!(
            made_by("operator"),
            elements_are![eq(&change("made_by", "Made by", None, Some("Whoever runs the server")))]
        );
        expect_that!(
            made_by("mail"),
            elements_are![eq(&change(
                "made_by",
                "Made by",
                None,
                Some("Mail, asked on the sign-in page")
            ))]
        );
        expect_that!(
            made_by("robot"),
            elements_are![eq(&change("made_by", "Made by", None, Some("robot")))]
        );
    }

    #[gtest]
    fn an_operators_account_change_reads_as_about_the_account() {
        let operator = json!({ "made_by": "operator" });
        let phrase = |action, diff: &Value| describe_phrase(action, EntityType::User, false, diff);

        expect_that!(
            phrase(AuditAction::AccountDeactivated, &operator),
            eq(&(Cow::Borrowed("was turned off by whoever runs the server"), false))
        );
        expect_that!(
            phrase(AuditAction::AccountReactivated, &operator),
            eq(&(Cow::Borrowed("was turned back on by whoever runs the server"), false))
        );
        expect_that!(
            phrase(AuditAction::PasswordResetLinkMade, &operator),
            eq(&(Cow::Borrowed("had a reset link made by whoever runs the server"), false))
        );
        // The provider's own pushes keep their words.
        expect_that!(
            phrase(AuditAction::AccountDeactivated, &json!({})).0,
            eq("was turned off by the sign-in provider")
        );

        // The line names the account, not the operator who is the entry's actor.
        let mut made = entry(AuditAction::AccountDeactivated, EntityType::User, operator);
        made.entity_id = Uuid::from_u128(4);
        let mut names = Names::default();
        names.insert(Uuid::from_u128(3), "Ana Lee");
        names.insert(Uuid::from_u128(4), "Bo Park");
        let page = present_page(vec![made], None, &names, &HashMap::new(), &TimeZone::UTC);
        expect_that!(page.entries[0].actor_name, eq("Bo Park"));
    }

    #[gtest]
    fn an_end_time_is_labelled_but_never_shown() {
        expect_that!(describe_field(AuditField::EndsAt, EntityType::InviteLink), eq("Ends"));
        expect_that!(
            matches!(reading(AuditField::EndsAt, EntityType::InviteLink), Reading::Unshown),
            eq(true)
        );
    }
}
