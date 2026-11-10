//! Reading the audit log: a heap's or a team's history, or one entity's, filtered by what the caller may see.
//!
//! Entries come back ready to show. Each change names the field in words and gives the values before and after as
//! text, with IDs replaced by the names those things have now (tags, options, items, users), since a diff only
//! records IDs (see `audit.rs`, which writes them). The raw diff stays on the entry for API clients that want it.

use std::collections::{BTreeSet, HashMap};

use domain::audit::{AuditAction, EntityType};
use domain::due::{describe_due_date, describe_time};
use domain::item::{Due, Kind, Need, Size, Stage, StateKind};
use domain::permission::{Permission, visible_audit_actions};
use domain::question::QuestionKind;
use domain::step::StepState;
use domain::team::Role;
use domain::{HeapId, ItemId, SavedViewId, TeamId, Timestamp, UserId};
use jiff::tz::TimeZone;
use serde_json::{Map, Value};
pub use store::audit::AuditCursor;
use store::audit::{AuditEntry, AuditQuery, AuditScope};
use uuid::Uuid;

use crate::access::{authorize_heap, authorize_team};
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
    pub phrase: &'static str,
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
        let (entries, next) = self.read_page(AuditScope::Heap(heap_id), &actions, request).await?;

        let mut names = Names::default();
        names.insert(*access.heap.id.as_uuid(), access.heap.name.as_str());
        for tag in store::tags::list_tags(&self.db, heap_id).await? {
            names.insert(*tag.id.as_uuid(), tag.name.as_str());
        }
        for parameter in store::parameters::list_parameters(&self.db, heap_id).await? {
            names.insert(*parameter.id.as_uuid(), parameter.name.as_str());
            for option in &parameter.options {
                names.insert(*option.id.as_uuid(), option.label.as_str());
            }
            for option in &parameter.archived_options {
                names.insert(*option.id.as_uuid(), option.label.as_str());
            }
        }
        let items = collect_referenced_items(&entries).into_iter().map(ItemId::from_uuid);
        for item in store::items::list_item_summaries(&self.db, heap_id, items).await? {
            names.insert(*item.id.as_uuid(), item.title.as_str());
        }
        let views: Vec<SavedViewId> = collect_entity_ids(&entries, EntityType::SavedView)
            .map(SavedViewId::from_uuid)
            .collect();
        for view in store::saved_views::list_saved_views_by_ids(&self.db, heap_id, &views).await? {
            names.insert(*view.id.as_uuid(), view.name.as_str());
        }
        self.insert_user_names(&mut names, &entries).await?;
        Ok(present_page(entries, next, &names, tz))
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
        for heap in store::heaps::list_heaps_for_teams(&self.db, &[team_id]).await? {
            names.insert(*heap.id.as_uuid(), heap.name.as_str());
        }
        self.insert_user_names(&mut names, &entries).await?;
        self.insert_invite_link_names(&mut names, team_id, &entries).await?;
        Ok(present_page(entries, next, &names, tz))
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

    async fn read_page(
        &self,
        scope: AuditScope,
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

/// The items entries are about, and the blockers their diffs name.
fn collect_referenced_items(entries: &[AuditEntry]) -> BTreeSet<Uuid> {
    let blockers = entries
        .iter()
        .filter(|entry| entry.entity_type == EntityType::Item)
        .filter_map(|entry| entry.diff.get("blocked_by"))
        .flat_map(|value| [value.get("before"), value.get("after"), Some(value)])
        .flatten()
        .filter_map(|value| value.as_str().and_then(|raw| Uuid::parse_str(raw).ok()));
    collect_entity_ids(entries, EntityType::Item).chain(blockers).collect()
}

fn present_page(entries: Vec<AuditEntry>, next: Option<AuditCursor>, names: &Names, tz: &TimeZone) -> AuditPage {
    let entries = entries
        .into_iter()
        .map(|entry| {
            let about_actor = entry.entity_id == *entry.actor.as_uuid();
            let (phrase, names_entity) = describe_action(entry.action, about_actor);
            AuditEntryView {
                id: entry.id,
                created_at: entry.created_at,
                created_label: describe_time(entry.created_at, tz),
                actor: entry.actor,
                actor_name: names.get(*entry.actor.as_uuid()).unwrap_or("Unknown user").to_owned(),
                action: entry.action,
                phrase,
                names_entity,
                entity_type: entry.entity_type,
                entity_id: entry.entity_id,
                entity_label: names.get(entry.entity_id).unwrap_or("Unknown").to_owned(),
                changes: describe_changes(&entry, names, tz),
                diff: entry.diff,
            }
        })
        .collect();
    AuditPage { entries, next }
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
        (AuditAction::HeapCreated, _) => ("created heap", true),
        (AuditAction::HeapUpdated, _) => ("changed heap", true),
        (AuditAction::HeapContextUpdated, _) => ("changed the context of heap", true),
        (AuditAction::HeapArchived, _) => ("archived heap", true),
        (AuditAction::HeapUnarchived, _) => ("unarchived heap", true),
        (AuditAction::ItemCreated, _) => ("added item", true),
        (AuditAction::ItemUpdated, _) => ("edited item", true),
        (AuditAction::StepAdded, _) => ("added a step to", true),
        (AuditAction::StepUpdated, _) => ("changed a step of", true),
        (AuditAction::QuestionAsked, _) => ("asked a question on", true),
        (AuditAction::QuestionAnswered, _) => ("answered a question on", true),
        (AuditAction::TagCreated, _) => ("created tag", true),
        (AuditAction::TagUpdated, _) => ("edited tag", true),
        (AuditAction::SavedViewCreated, _) => ("saved view", true),
        (AuditAction::SavedViewUpdated, _) => ("changed view", true),
        (AuditAction::SavedViewArchived, _) => ("archived view", true),
        (AuditAction::ParameterCreated, _) => ("added parameter", true),
        (AuditAction::ParameterUpdated, _) => ("changed parameter", true),
        (AuditAction::ParameterArchived, _) => ("archived parameter", true),
        (AuditAction::OptionArchived, _) => ("archived an option of", true),
        (AuditAction::InviteLinkCreated, _) => ("made an invite link", false),
        (AuditAction::InviteLinkRevoked, _) => ("revoked", true),
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
        | AuditAction::HeapCreated
        | AuditAction::ItemCreated
        | AuditAction::StepAdded
        | AuditAction::QuestionAsked
        | AuditAction::QuestionAnswered
        | AuditAction::TagCreated
        | AuditAction::SavedViewCreated
        | AuditAction::ParameterCreated
        | AuditAction::InviteLinkCreated => DiffShape::Created,
        AuditAction::MemberRemoved | AuditAction::MemberLeft => DiffShape::Ended,
        AuditAction::UserUpdated
        | AuditAction::TeamUpdated
        | AuditAction::MemberRoleChanged
        | AuditAction::HeapUpdated
        | AuditAction::HeapContextUpdated
        | AuditAction::HeapArchived
        | AuditAction::HeapUnarchived
        | AuditAction::ItemUpdated
        | AuditAction::StepUpdated
        | AuditAction::TagUpdated
        | AuditAction::SavedViewUpdated
        | AuditAction::SavedViewArchived
        | AuditAction::ParameterUpdated
        | AuditAction::ParameterArchived
        | AuditAction::OptionArchived
        | AuditAction::InviteLinkRevoked => DiffShape::Changed,
    }
}

/// Fields in the order forms show them. Fields not listed come after these, by name.
const FIELD_ORDER: [&str; 38] = [
    "step",
    "step_title",
    "step_context",
    "step_state",
    "question",
    "question_kind",
    "question_text",
    "question_details",
    "question_choices",
    "question_many",
    "question_step",
    "reply",
    "reply_comment",
    "title",
    "kind",
    "stage",
    "name",
    "display_name",
    "description",
    "context",
    "state",
    "blocked_by",
    "need",
    "effort",
    "impact",
    "due",
    "tags",
    "parameters",
    "role",
    "weight",
    "need_weight",
    "age_weight",
    "deadline_weight",
    "value_weight",
    "options",
    "archived_options",
    "items_switched",
    "list_view",
];

/// Fields left out: the action already says it, as with `archived_at` for an archiving.
const UNSHOWN_FIELDS: [&str; 1] = ["archived_at"];

fn describe_changes(entry: &AuditEntry, names: &Names, tz: &TimeZone) -> Vec<AuditChange> {
    // The phrase says it all ("made an invite link"), and the diff holds a raw end time and link states, which
    // have no short form worth showing.
    if entry.entity_type == EntityType::InviteLink {
        return Vec::new();
    }
    let Value::Object(fields) = &entry.diff else {
        return Vec::new();
    };
    let mut fields: Vec<(&String, &Value)> = fields
        .iter()
        .filter(|(field, _)| !UNSHOWN_FIELDS.contains(&field.as_str()))
        .collect();
    fields.sort_by_key(|(field, _)| {
        let position = FIELD_ORDER.iter().position(|known| known == field);
        (position.unwrap_or(FIELD_ORDER.len()), field.as_str())
    });

    let shape = classify_diff(entry.action);
    let mut changes = Vec::new();
    for (field, value) in fields {
        let (before, after) = match shape {
            DiffShape::Created => (&Value::Null, value),
            DiffShape::Ended => (value, &Value::Null),
            DiffShape::Changed => match (value.get("before"), value.get("after")) {
                (Some(before), Some(after)) => (before, after),
                _ => (&Value::Null, value),
            },
        };
        if field == "parameters" {
            changes.extend(describe_parameter_values(before, after, names));
            continue;
        }
        if has_no_value(before) && has_no_value(after) {
            continue;
        }
        let change = AuditChange {
            field: field.clone(),
            label: describe_field(field, entry.entity_type).to_owned(),
            before: describe_value(field, before, names, tz),
            after: describe_value(field, after, names, tz),
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
fn describe_field(field: &str, entity_type: EntityType) -> &str {
    match field {
        "step" => "Step",
        "step_title" => "Step title",
        "step_context" => "Step context",
        "step_state" => "Step state",
        "question" => "Question",
        "question_kind" => "Kind",
        "question_text" => "Question text",
        "question_details" => "Details",
        "question_choices" => "Choices",
        "question_many" => "Pick any",
        "question_step" => "Step",
        "reply" => "Reply",
        "reply_comment" => "Comment",
        "title" => "Title",
        "kind" => "Kind",
        "stage" => "Stage",
        "name" => "Name",
        "display_name" => "Display name",
        "description" if entity_type == EntityType::Item => "Context",
        "description" => "Description",
        "context" => "Context",
        "state" => "State",
        "blocked_by" => "Blocked by",
        "need" => "Need",
        "effort" => "Effort",
        "impact" => "Impact",
        "due" => "Due",
        "tags" => "Tags",
        "role" => "Role",
        "weight" => "Weight",
        "need_weight" => "Need weight",
        "age_weight" => "Age weight",
        "deadline_weight" => "Deadline weight",
        "value_weight" => "Value per effort weight",
        "options" => "Options",
        "archived_options" => "Archived options",
        "items_switched" => "Items switched",
        "list_view" => "What it shows",
        other => other,
    }
}

/// A diff value as text, or `None` when there was no value or it has no short form.
fn describe_value(field: &str, value: &Value, names: &Names, tz: &TimeZone) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let text = match field {
        "kind" => describe_kind(Kind::from_code(value.as_str()?)?).to_owned(),
        "stage" => describe_stage(Stage::from_code(value.as_str()?)?).to_owned(),
        "state" => describe_state(StateKind::from_code(value.as_str()?)?).to_owned(),
        // A step is named by its title; its number is in the diff for API clients.
        "step" => value.get("title")?.as_str()?.to_owned(),
        "step_state" => describe_step_state(StepState::from_code(value.as_str()?)?).to_owned(),
        // A question is named by its number, as `q2`, after its item's key.
        "question" => format!("q{}", value.as_u64()?),
        "question_kind" => describe_question_kind(QuestionKind::from_code(value.as_str()?)?).to_owned(),
        "question_choices" => join(value.as_array()?.iter().filter_map(Value::as_str).map(str::to_owned))?,
        "need" => Need::from_code(value.as_str()?)?.label().to_owned(),
        "effort" | "impact" => describe_size(Size::from_code(value.as_str()?)?).to_owned(),
        "role" => describe_role(Role::from_code(value.as_str()?)?).to_owned(),
        "due" => describe_recorded_due(value, tz)?,
        "blocked_by" => names.of_value(value, "Unknown item"),
        "tags" => join(value.as_array()?.iter().map(|tag| names.of_value(tag, "Unknown tag")))?,
        "options" | "archived_options" => join(
            value
                .as_array()?
                .iter()
                .filter_map(|option| option.get("label").and_then(Value::as_str))
                .map(str::to_owned),
        )?,
        "items_switched" => match value.as_array()?.len() {
            1 => "1 item".to_owned(),
            n => format!("{n} items"),
        },
        // A whole list view has no short form; the label says it changed.
        "list_view" => return None,
        _ => match value {
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            Value::Bool(true) => "yes".to_owned(),
            Value::Bool(false) => "no".to_owned(),
            other => other.to_string(),
        },
    };
    Some(text)
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

const fn describe_state(state: StateKind) -> &'static str {
    match state {
        StateKind::Open => "Open",
        StateKind::InProgress => "In progress",
        StateKind::Blocked => "Blocked",
        StateKind::Done => "Done",
        StateKind::WontDo => "Won't do",
    }
}

const fn describe_question_kind(kind: QuestionKind) -> &'static str {
    match kind {
        QuestionKind::Question => "Question",
        QuestionKind::Request => "Request",
    }
}

const fn describe_step_state(state: StepState) -> &'static str {
    match state {
        StepState::Open => "Open",
        StepState::Ticked => "Ticked",
        StepState::Dropped => "Dropped",
    }
}

const fn describe_kind(kind: Kind) -> &'static str {
    match kind {
        Kind::Note => "Note",
        Kind::Task => "Task",
    }
}

const fn describe_stage(stage: Stage) -> &'static str {
    match stage {
        Stage::AsWritten => "As written",
        Stage::WithContext => "With context",
        Stage::Defined => "Defined",
    }
}

const fn describe_size(size: Size) -> &'static str {
    match size {
        Size::S => "S",
        Size::M => "M",
        Size::L => "L",
        Size::Xl => "XL",
    }
}

const fn describe_role(role: Role) -> &'static str {
    match role {
        Role::Admin => "Admin",
        Role::Member => "Member",
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use serde_json::json;

    use super::*;

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
            created_at: Timestamp::EPOCH,
        }
    }

    fn change(field: &str, label: &str, before: Option<&str>, after: Option<&str>) -> AuditChange {
        AuditChange {
            field: field.to_owned(),
            label: label.to_owned(),
            before: before.map(str::to_owned),
            after: after.map(str::to_owned),
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
                eq(&change(
                    "step_title",
                    "Step title",
                    Some("Run in staging"),
                    Some("Run in prod")
                )),
                eq(&change("step_context", "Step context", Some("old"), None)),
                eq(&change("step_state", "Step state", Some("Open"), Some("Ticked"))),
            ]
        );
        expect_that!(
            describe_action(AuditAction::StepUpdated, false),
            eq(("changed a step of", true))
        );
    }
}
