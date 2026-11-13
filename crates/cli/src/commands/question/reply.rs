//! `upheap question reply <question>`: reply to a question or a request on an item, with picks, a comment, or both.
//!
//! The CLI reads the item first, for the question's version and its choices, which it needs to turn what was typed
//! into the indices the API takes. The server owns every other rule: how many picks, whether a comment is needed,
//! an open question on an open item. Repeating a command is safe: a question answered already with this very reply
//! is said to be, and makes no write. Another reply is refused, since it would be lost, and so is a withdrawn
//! question.

use api_types::common::{QuestionDto, QuestionStateDto};
use api_types::questions::{QuestionPatchDto, ReplyDto};
use clap::Args;

use crate::commands::heap_api::{HeapApi, read_context, read_text_flag};
use crate::commands::item::common::locate_item;
use crate::commands::{HeapArgs, SharedArgs, UsageError};
use crate::error::QuestionRefusal;
use crate::lookup::{QuestionName, match_choices};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ReplyArgs {
    /// The question: its item's key and its number, such as k3x9a.q2. The item may be team/heap/key or a link
    #[arg(value_name = "QUESTION")]
    pub question: String,
    /// A choice to pick, by its label ignoring case, or by its number, 1 for the first. Repeat for more where the
    /// question allows any number. A request's choices are Done and Can't be done, a review's Approved and Changes asked, a done proposal's Close as done and Not yet
    #[arg(long = "pick", value_name = "CHOICE")]
    pub picks: Vec<String>,
    /// A comment with the reply. `-` reads all of stdin. For longer text, use --comment-file
    #[arg(long, value_name = "TEXT", conflicts_with = "comment_file")]
    pub comment: Option<String>,
    /// The comment, read from this file, or from stdin for `-`
    #[arg(long, value_name = "FILE")]
    pub comment_file: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Replies to the question. Human output names it; `--quiet` prints nothing and `--json` prints the item.
pub async fn run_reply(args: ReplyArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = QuestionName::parse(&args.question)?;
    if args.picks.is_empty() && args.comment.is_none() && args.comment_file.is_none() {
        return Err(UsageError::EmptyReply.into());
    }
    let comment = match (&args.comment, &args.comment_file) {
        (Some(text), _) => Some(read_text_flag(text, "--comment", app).await?),
        (None, Some(path)) => Some(read_context(path, app).await?),
        (None, None) => None,
    };
    let at = locate_item(&name.item_typed, name.item.clone(), args.heap.name.as_deref(), app)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let item = api.get_item(&name.item_typed, &at.key).await?;
    let question = item
        .value
        .questions
        .iter()
        .flatten()
        .find(|question| question.number == name.number)
        .ok_or_else(|| api.no_such_question(&at.key, name.number))?;
    let full_name = format!("{}.q{}", at.key, question.number);
    if question.state == QuestionStateDto::Withdrawn {
        return Err(CliError::QuestionRefused {
            name: full_name,
            refusal: QuestionRefusal::Withdrawn,
        });
    }
    let picked = match_choices(&full_name, &args.picks, &question.choices)?;
    if question.state == QuestionStateDto::Answered {
        if !is_same_reply(question, &picked, comment.as_deref()) {
            return Err(CliError::QuestionRefused {
                name: full_name,
                refusal: QuestionRefusal::AnsweredDifferently,
            });
        }
        match out.mode() {
            Mode::Json => out.json(&item.body)?,
            Mode::Quiet => {}
            Mode::Human => out.line(&format!("{full_name} is already answered with that."))?,
        }
        return Ok(());
    }
    let patch = QuestionPatchDto {
        reply: Some(ReplyDto { picked, comment }),
    };
    let replied = api
        .patch_question(&at.key, question.number, question.version, &patch)
        .await?;
    match out.mode() {
        Mode::Json => out.json(&replied.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&format!(
            "Replied to {}.q{}: {}",
            at.key, question.number, question.text
        ))?,
    }
    Ok(())
}

/// Whether the answered `question` has the reply of `picked` and `comment`: the picks as a set, the comments
/// trimmed, none the same as an empty one.
fn is_same_reply(question: &QuestionDto, picked: &[u32], comment: Option<&str>) -> bool {
    let Some(reply) = &question.reply else {
        return false;
    };
    let mut have = reply.picked.clone();
    let mut want = picked.to_vec();
    have.sort_unstable();
    want.sort_unstable();
    have == want && reply.comment.as_deref().unwrap_or("").trim() == comment.unwrap_or("").trim()
}
