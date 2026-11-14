//! Conversions from the service layer's item views to the API's item types, and the `If-Match` header's docs.
//!
//! The types themselves are in `api_types`, with their conversions from domain types; these need `service`, which
//! that crate doesn't depend on, or the request's time and timezone.

use api_types::comments::CommentDto;
use api_types::common::{
    BlockerGoneDto, ContributorDto, DueDto, HeapSummaryDto, HeldBackDto, ItemActionDto, ItemDto, ItemOptionDto,
    ItemOriginDto, ItemParameterDto, ItemRefDto, ItemTagDto, MadeItemDto, QuestionDto, QuestionReplyDto, RankedItemDto,
    ReviewDto, SizeDto, StageDto, StepDto, StepProgressDto, UserRefDto,
};
use api_types::turns::{GoalDto, TurnDto};
use api_types::waiting::ItemHeapDto;
use api_types::work::ItemRunDto;
use domain::Timestamp;
use domain::due::{describe_ago, describe_date, describe_due, describe_since, group_closed};
use domain::heap::Heap;
use domain::item::Kind;
use domain::team::Team;
use domain::turn::goal_choices;
use jiff::tz::TimeZone;
use service::items::{
    CommentView, GoneBlocker, ItemOrigin, ItemRunView, ItemView, ParameterValue, QuestionView, RankedEntry, TurnView,
};
use utoipa::IntoParams;

fn to_item_parameter_dto(value: ParameterValue) -> ItemParameterDto {
    ItemParameterDto {
        parameter_id: *value.parameter_id.as_uuid(),
        name: value.name.as_str().to_owned(),
        parameter_archived: value.parameter_archived,
        option: value.option.map(|option| ItemOptionDto {
            id: *option.id.as_uuid(),
            label: option.label.as_str().to_owned(),
            archived: option.archived,
        }),
    }
}

/// Renders an item with labels computed for `as_of` in `tz`.
pub(crate) fn to_item_dto(view: ItemView, as_of: Timestamp, tz: &TimeZone) -> ItemDto {
    let item = view.item;
    let closed_at = item.state.closed_at();
    // What a turn opened now may go to, the default first: for an open note or task (a reminder has none).
    let goal_choices: Vec<GoalDto> = if closed_at.is_none() {
        goal_choices(&item).into_iter().map(Into::into).collect()
    } else {
        Vec::new()
    };
    let next_goal = goal_choices.first().copied();
    ItemDto {
        id: *item.id.as_uuid(),
        key: item.key.to_string(),
        heap_id: *item.heap_id.as_uuid(),
        title: item.title.as_str().to_owned(),
        description: item.description.map(|d| d.as_str().to_owned()),
        kind: item.kind.into(),
        // The stage is shown while the item is open.
        stage: closed_at.is_none().then(|| item.stage.into()),
        stage_choices: view.stage_choices.into_iter().map(StageDto::from).collect(),
        state: item.state.kind().into(),
        blocked_by: view.blocked_by.map(ItemRefDto::from),
        blocked_reason: item.state.blocked_reason().map(|reason| reason.as_str().to_owned()),
        blocker_gone: view.blocker_gone.map(to_blocker_gone_dto),
        became: view
            .became
            .map(|became| became.into_iter().map(to_made_item_dto).collect()),
        need: item.need.into(),
        effort: item.effort.map(SizeDto::from),
        impact: item.impact.map(SizeDto::from),
        due: item.due.map(|due| {
            let view = describe_due(due, as_of, tz);
            let date = due.at.to_jiff().to_zoned(tz.clone()).date().to_string();
            DueDto {
                at: due.at.as_millis(),
                date,
                firm: due.firm,
                label: view.label,
                overdue: view.overdue,
            }
        }),
        // A task that was a reminder keeps its arrival in storage, but it is no longer a reminder to clients.
        arrives_at: (item.kind == Kind::Reminder)
            .then_some(item.arrives_at)
            .flatten()
            .map(Timestamp::as_millis),
        tags: view.tags.iter().map(ItemTagDto::from).collect(),
        parameters: view.parameters.into_iter().map(to_item_parameter_dto).collect(),
        created_at: item.created_at.as_millis(),
        created_label: describe_ago(item.created_at, as_of, tz),
        created_by: view.created_by.as_ref().map(UserRefDto::from),
        closed_at: closed_at.map(Timestamp::as_millis),
        closed_label: closed_at.map(|at| describe_date(at, as_of, tz)),
        closed_by: view.closed_by.as_ref().map(UserRefDto::from),
        closed_group: closed_at.map(|at| group_closed(at, as_of, tz).into()),
        steps: view.steps.as_ref().map(|steps| {
            steps
                .iter()
                .map(|step| StepDto::new(step, &view.step_actions))
                .collect()
        }),
        step_progress: view.step_progress.map(StepProgressDto::from),
        questions: view.questions.map(|questions| {
            questions
                .into_iter()
                .map(|question| to_question_dto(question, as_of, tz))
                .collect()
        }),
        comments: view.comments.map(|comments| {
            comments
                .into_iter()
                .map(|comment| to_comment_dto(comment, as_of, tz))
                .collect()
        }),
        runs_cost_usd: view.runs.as_deref().and_then(runs_cost_usd),
        runs: view
            .runs
            .map(|runs| runs.into_iter().map(|view| to_item_run_dto(view, as_of, tz)).collect()),
        waits_for_you: view.waits_for_you,
        turn: view.turn.map(|turn| to_turn_dto(turn, as_of, tz)),
        next_goal,
        goal_choices,
        with_agent: view.with_agent,
        origin: view.origin.map(to_item_origin_dto),
        added_in_run: item.created_run_id.is_some(),
        version: item.version.get(),
        allowed_actions: view.allowed_actions.into_iter().map(ItemActionDto::from).collect(),
    }
}

pub(crate) fn to_item_origin_dto(origin: ItemOrigin) -> ItemOriginDto {
    ItemOriginDto {
        id: *origin.item.id.as_uuid(),
        key: origin.item.key.to_string(),
        title: origin.item.title.as_str().to_owned(),
        kind: origin.item.kind.into(),
        heap: to_item_heap_dto(&origin.heap, &origin.team),
    }
}

pub(crate) fn to_item_heap_dto(heap: &Heap, team: &Team) -> ItemHeapDto {
    ItemHeapDto {
        id: *heap.id.as_uuid(),
        slug: heap.slug.to_string(),
        name: heap.name.as_str().to_owned(),
        team_slug: team.slug.to_string(),
        team_name: team.name.as_str().to_owned(),
    }
}

pub(crate) fn to_blocker_gone_dto(gone: GoneBlocker) -> BlockerGoneDto {
    BlockerGoneDto {
        how: gone.how.into(),
        moved_to: gone.moved_to.as_ref().map(HeapSummaryDto::from),
    }
}

pub(crate) fn to_turn_dto(view: TurnView, as_of: Timestamp, tz: &TimeZone) -> TurnDto {
    TurnDto::new(
        &view.turn,
        view.opened_by.as_ref().map(UserRefDto::from),
        describe_since(view.turn.opened_at, as_of, tz),
        view.runs,
        view.stuck_reason,
    )
}

pub(crate) fn to_question_dto(view: QuestionView, as_of: Timestamp, tz: &TimeZone) -> QuestionDto {
    let question = view.question;
    let replied_by = view.replied_by.as_ref().map(UserRefDto::from);
    QuestionDto {
        number: question.number.get(),
        kind: question.kind.into(),
        text: question.text.as_str().to_owned(),
        details: question.details.as_ref().map(|details| details.as_str().to_owned()),
        choices: question.choices.labels().to_vec(),
        many: question.many,
        step: question.step.map(|step| step.get()),
        review: question.review.as_ref().map(|review| ReviewDto {
            link: review.link.as_str().to_owned(),
            piece: review.piece.as_str().to_owned(),
            round: review.round,
        }),
        state: question.state().into(),
        asked_by: view.asked_by.as_ref().map(UserRefDto::from),
        asked_at: question.asked_at.as_millis(),
        asked_label: describe_ago(question.asked_at, as_of, tz),
        reply: question.reply.map(|reply| QuestionReplyDto {
            picked: reply.picked,
            comment: reply.comment.as_ref().map(|comment| comment.as_str().to_owned()),
            replied_by,
            replied_at: reply.replied_at.as_millis(),
            replied_label: describe_ago(reply.replied_at, as_of, tz),
        }),
        withdrawn_at: question
            .withdrawal
            .map(|withdrawal| withdrawal.withdrawn_at.as_millis()),
        withdrawn_by: view.withdrawn_by.as_ref().map(UserRefDto::from),
        version: question.version.get(),
        allowed_actions: view.allowed_actions.into_iter().map(Into::into).collect(),
    }
}

pub(crate) fn to_comment_dto(view: CommentView, as_of: Timestamp, tz: &TimeZone) -> CommentDto {
    let comment = view.comment;
    CommentDto {
        number: comment.number.get(),
        body: comment.body.as_str().to_owned(),
        created_by: view.created_by.as_ref().map(UserRefDto::from),
        created_at: comment.created_at.as_millis(),
        created_label: describe_since(comment.created_at, as_of, tz),
    }
}

/// A cost in millionths of a US dollar as dollars.
pub(crate) fn micro_usd_to_usd(micro: i64) -> f64 {
    micro as f64 / 1_000_000.0
}

/// The sum of the known costs of `runs` in US dollars, or `None` when none has one.
fn runs_cost_usd(runs: &[ItemRunView]) -> Option<f64> {
    runs.iter()
        .filter_map(|view| view.run.cost_micro_usd)
        .reduce(|total, micro| total + micro)
        .map(micro_usd_to_usd)
}

fn to_item_run_dto(view: ItemRunView, as_of: Timestamp, tz: &TimeZone) -> ItemRunDto {
    let run = view.run;
    let ended = run.ended.as_ref();
    ItemRunDto {
        id: *run.id.as_uuid(),
        started_at: run.started_at.as_millis(),
        ended_at: ended.map(|ended| ended.at.as_millis()),
        tool_started_at: run.tool_started_at.map(Timestamp::as_millis),
        tool_ended_at: run.tool_ended_at.map(Timestamp::as_millis),
        started_label: describe_since(run.started_at, as_of, tz),
        from_stage: run.from_stage.into(),
        goal: view.goal.into(),
        end: ended.map(|ended| ended.end.into()),
        result: ended.and_then(|ended| ended.result.as_ref().map(|result| result.as_str().to_owned())),
        links: ended
            .map(|ended| ended.links.iter().map(|link| link.as_str().to_owned()).collect())
            .unwrap_or_default(),
        failure: ended.and_then(|ended| ended.failure.clone()),
        cost_usd: run.cost_micro_usd.map(micro_usd_to_usd),
    }
}

pub(crate) fn to_ranked_dto(entry: RankedEntry, as_of: Timestamp, tz: &TimeZone) -> RankedItemDto {
    RankedItemDto {
        position: entry.position,
        contributors: entry.contributors.into_iter().map(ContributorDto::from).collect(),
        held_back_by: entry.held_back_by.into_iter().map(HeldBackDto::from).collect(),
        item: to_item_dto(entry.view, as_of, tz),
    }
}

/// The `If-Match` request header, documented once.
#[derive(IntoParams)]
#[into_params(parameter_in = Header)]
#[allow(dead_code)]
pub(crate) struct IfMatchHeader {
    /// The `version` of what is being edited, for example `"3"`.
    #[param(rename = "If-Match")]
    if_match: String,
}

fn to_made_item_dto(made: service::items::MadeItem) -> MadeItemDto {
    MadeItemDto {
        id: *made.item.id.as_uuid(),
        key: made.item.key.to_string(),
        title: made.item.title.as_str().to_owned(),
        kind: made.item.kind.into(),
        state: made.item.state.kind().into(),
        heap: ItemHeapDto {
            id: *made.heap.id.as_uuid(),
            slug: made.heap.slug.to_string(),
            name: made.heap.name.as_str().to_owned(),
            team_slug: made.team.slug.to_string(),
            team_name: made.team.name.as_str().to_owned(),
        },
    }
}
