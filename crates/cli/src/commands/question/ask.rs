//! `upheap question ask <item> <text>`, `upheap question request <item> <text>`, `upheap question review <item> <link>
//! <text>` and `upheap question propose-done <item> <text>`: put a question, a request for something to do, a review of
//! work at a link, or a proposal to close the task, on an item.
//!
//! One `POST`, with no read before it: the server owns every rule (an open item, the text and the choices, at most
//! 100 questions, a step the item has), and it answers with the whole item, whose newest question, the one with the
//! highest number, is the one asked.

use api_types::common::{ItemDto, QuestionKindDto};
use api_types::questions::AskQuestionDto;
use clap::Args;

use crate::commands::heap_api::{HeapApi, read_context};
use crate::commands::item::common::locate_item;
use crate::commands::step::add::StepRef;
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::ItemName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct AskArgs {
    /// The item: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    /// The question in one line, 1 to 300 characters
    #[arg(value_name = "TEXT")]
    pub text: String,
    /// The long version, read from this file, or from stdin for `-`
    #[arg(long, value_name = "FILE")]
    pub details_file: Option<String>,
    /// An answer to pick from; give it 2 to 10 times. Left out, the answer is free text
    #[arg(long = "choice", value_name = "LABEL")]
    pub choices: Vec<String>,
    /// Allow picking any number of the choices, not just one
    #[arg(long, requires = "choices")]
    pub many: bool,
    /// The step it is about, named like k3x9a.3 or by its number, 3
    #[arg(long, value_name = "STEP")]
    pub step: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

#[derive(Args, Debug)]
pub struct ReviewArgs {
    /// The item: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    /// Where the work is to look at, such as a pull request: an http or https link
    #[arg(value_name = "LINK")]
    pub link: String,
    /// What it is, in one line, 1 to 300 characters
    #[arg(value_name = "TEXT")]
    pub text: String,
    /// What you did and how you checked it, read from this file, or from stdin for `-`
    #[arg(long, value_name = "FILE")]
    pub details_file: Option<String>,
    /// The piece of work this is about, if it isn't the link: asking again about the same piece is the next round
    #[arg(long, value_name = "TEXT")]
    pub piece: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

#[derive(Args, Debug)]
pub struct ProposeDoneArgs {
    /// The item: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    /// What was done, in one line, 1 to 300 characters
    #[arg(value_name = "TEXT")]
    pub text: String,
    /// The result and links, read from this file, or from stdin for `-`
    #[arg(long, value_name = "FILE")]
    pub details_file: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

#[derive(Args, Debug)]
pub struct RequestArgs {
    /// The item: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    /// What to do, in one line, 1 to 300 characters
    #[arg(value_name = "TEXT")]
    pub text: String,
    /// The long version, read from this file, or from stdin for `-`
    #[arg(long, value_name = "FILE")]
    pub details_file: Option<String>,
    /// The step it is about, named like k3x9a.3 or by its number, 3
    #[arg(long, value_name = "STEP")]
    pub step: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Asks a question. Human output names it; `--quiet` prints its name and `--json` the item.
pub async fn run_ask(args: AskArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let asking = Asking {
        kind: QuestionKindDto::Question,
        item: args.item,
        text: args.text,
        details_file: args.details_file,
        choices: args.choices,
        many: args.many,
        step: args.step,
        link: None,
        piece: None,
        heap: args.heap,
        shared: args.shared,
    };
    run(asking, app, out).await
}

/// Makes a request. Output as for [`run_ask`].
pub async fn run_request(args: RequestArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let asking = Asking {
        kind: QuestionKindDto::Request,
        item: args.item,
        text: args.text,
        details_file: args.details_file,
        choices: Vec::new(),
        many: false,
        step: args.step,
        link: None,
        piece: None,
        heap: args.heap,
        shared: args.shared,
    };
    run(asking, app, out).await
}

/// Asks for a review of the work at a link. Human output names the review and its round; `--quiet` prints its name
/// and `--json` the item.
pub async fn run_review(args: ReviewArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let asking = Asking {
        kind: QuestionKindDto::Review,
        item: args.item,
        text: args.text,
        details_file: args.details_file,
        choices: Vec::new(),
        many: false,
        step: None,
        link: Some(args.link),
        piece: args.piece,
        heap: args.heap,
        shared: args.shared,
    };
    run(asking, app, out).await
}

/// Proposes closing the task. Human output names the proposal; `--quiet` prints its name and `--json` the item.
pub async fn run_propose_done(args: ProposeDoneArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let asking = Asking {
        kind: QuestionKindDto::DoneProposal,
        item: args.item,
        text: args.text,
        details_file: args.details_file,
        choices: Vec::new(),
        many: false,
        step: None,
        link: None,
        piece: None,
        heap: args.heap,
        shared: args.shared,
    };
    run(asking, app, out).await
}

/// What any verb asks for.
struct Asking {
    kind: QuestionKindDto,
    item: String,
    text: String,
    details_file: Option<String>,
    choices: Vec<String>,
    many: bool,
    step: Option<String>,
    link: Option<String>,
    piece: Option<String>,
    heap: HeapArgs,
    shared: SharedArgs,
}

async fn run(asking: Asking, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = ItemName::parse(&asking.item)?;
    let step = asking.step.as_deref().map(StepRef::parse).transpose()?;
    let at = locate_item(&asking.item, name, asking.heap.name.as_deref(), app)?;
    let details = match &asking.details_file {
        Some(path) => Some(read_context(path, app).await?),
        None => None,
    };
    let client = app.client(app.server(&asking.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let step = step
        .map(|step| step.number_in(&asking.item, &at.key, &heap))
        .transpose()?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let body = AskQuestionDto {
        kind: asking.kind,
        text: asking.text,
        details,
        choices: (!asking.choices.is_empty()).then_some(asking.choices),
        many: asking.many.then_some(true),
        step,
        link: asking.link,
        piece: asking.piece,
    };
    let asked = api.post_question(&asking.item, &at.key, &body).await?;
    report(&asked.value, asking.kind, &asked.body, out)
}

/// Says what was asked: human output names the question, `--quiet` prints its name and `--json` the item.
fn report(item: &ItemDto, kind: QuestionKindDto, body: &str, out: &mut Output) -> Result<(), CliError> {
    // Numbers only grow, so the question just asked has the highest.
    let question = item.questions.iter().flatten().max_by_key(|question| question.number);
    match (out.mode(), question) {
        (Mode::Json, _) => out.json(body)?,
        (Mode::Quiet, Some(question)) => out.line(&format!("{}.q{}", item.key, question.number))?,
        (Mode::Human, Some(question)) => {
            let line = match (kind, &question.review) {
                (QuestionKindDto::Review, Some(review)) => format!(
                    "Asked for review {}.q{} (round {} of {}).",
                    item.key, question.number, review.round, review.piece
                ),
                (QuestionKindDto::DoneProposal, _) => {
                    format!("Proposed closing {} ({}.q{}).", item.key, item.key, question.number)
                }
                (QuestionKindDto::Request, _) => {
                    format!("Requested {}.q{}: {}", item.key, question.number, question.text)
                }
                _ => format!("Asked {}.q{}: {}", item.key, question.number, question.text),
            };
            out.line(&line)?;
        }
        (Mode::Quiet | Mode::Human, None) => {}
    }
    Ok(())
}
