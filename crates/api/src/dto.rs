//! Conversions from the service layer's item views to the API's item types, and the `If-Match` header's docs.
//!
//! The types themselves are in `api_types`, with their conversions from domain types; these need `service`, which
//! that crate doesn't depend on, or the request's time and timezone.

use api_types::common::{
    ContributorDto, DueDto, HeldBackDto, ItemActionDto, ItemDto, ItemOptionDto, ItemParameterDto, ItemRefDto,
    ItemTagDto, QuestionDto, QuestionReplyDto, RankedItemDto, SizeDto, StageDto, StepDto, StepProgressDto, UserRefDto,
};
use api_types::turns::TurnDto;
use domain::Timestamp;
use domain::due::{describe_ago, describe_date, describe_due, describe_since, group_closed};
use domain::permission::ItemAction;
use domain::turn::goal_for;
use jiff::tz::TimeZone;
use service::items::{ItemView, ParameterValue, QuestionView, RankedEntry, TurnView};
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
    // What a turn opened now would go to: for an open item, in a build that has agents.
    let next_goal = (cfg!(feature = "agents") && closed_at.is_none()).then(|| goal_for(&item).into());
    ItemDto {
        id: *item.id.as_uuid(),
        key: item.key.to_string(),
        heap_id: *item.heap_id.as_uuid(),
        title: item.title.as_str().to_owned(),
        description: item.description.map(|d| d.as_str().to_owned()),
        kind: item.kind.into(),
        // The stage is shown while the item is open, and only in a build that has agents.
        stage: (cfg!(feature = "agents") && closed_at.is_none()).then(|| item.stage.into()),
        stage_choices: if cfg!(feature = "agents") {
            view.stage_choices.into_iter().map(StageDto::from).collect()
        } else {
            Vec::new()
        },
        state: item.state.kind().into(),
        blocked_by: view.blocked_by.map(ItemRefDto::from),
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
        waits_for_you: view.waits_for_you,
        turn: view.turn.map(|turn| to_turn_dto(turn, as_of, tz)),
        next_goal,
        with_agent: view.with_agent,
        version: item.version.get(),
        // Sending an item to an agent and taking it back are offered only in a build that has agents.
        allowed_actions: view
            .allowed_actions
            .into_iter()
            .filter(|action| {
                cfg!(feature = "agents") || !matches!(action, ItemAction::SendToAgent | ItemAction::TakeBack)
            })
            .map(ItemActionDto::from)
            .collect(),
    }
}

pub(crate) fn to_turn_dto(view: TurnView, as_of: Timestamp, tz: &TimeZone) -> TurnDto {
    TurnDto::new(
        &view.turn,
        view.opened_by.as_ref().map(UserRefDto::from),
        describe_since(view.turn.opened_at, as_of, tz),
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
