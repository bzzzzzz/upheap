//! The commands, one module per category and one file per verb. Each file holds its args and its `run`, the flow
//! that makes the calls and writes the output.

pub mod auth;
pub mod heap;
pub(crate) mod heap_api;
pub mod item;
pub mod step;
pub mod tag;

use clap::Args;
use serde::de::DeserializeOwned;

/// The flag of the commands that work in a heap.
#[derive(Args, Clone, Debug)]
pub struct HeapArgs {
    /// The heap to work in: team/heap, or a heap slug only one of your teams has. Beats UPHEAP_HEAP and the
    /// default from `upheap heap use`
    #[arg(long = "heap", value_name = "HEAP")]
    pub name: Option<String>,
}

/// The flags every command takes.
#[derive(Args, Clone, Debug)]
pub struct SharedArgs {
    /// Print the server's response body as it came, and a refusal's problem body on stderr
    #[arg(long, conflicts_with = "quiet")]
    pub json: bool,
    /// Print only what a script needs next, such as item keys one per line
    #[arg(long, short)]
    pub quiet: bool,
    /// The server's address, such as https://upheap.example.com. Beats UPHEAP_SERVER and the config file
    #[arg(long, value_name = "URL")]
    pub server: Option<String>,
}

/// A command line that clap accepts but that asks for two things that don't go together. Checked before any request.
#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    #[error("--sort-parameter sorts by a custom parameter, so it doesn't go with --sort {sort}; leave out one of them")]
    SortWithSortParameter { sort: String },
    #[error("--sort parameter needs the parameter to sort by: add --sort-parameter <name>")]
    SortWithoutParameter,
    #[error("`{item}` is in {item_heap}, but --heap says {flag_heap}; leave out one of them")]
    ItemInOtherHeap {
        item: String,
        item_heap: String,
        flag_heap: String,
    },
    #[error("--tag and --untag both name {name}")]
    TagAndUntag { name: String },
    #[error("can't read stdin as text for {flag} -")]
    UnreadableStdin {
        flag: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("can't read {path} for --context-file")]
    UnreadableContextFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`{step}` is a step of another item than `{item}`; name a step of {item}, or just its number")]
    StepInOtherItem { step: String, item: String },
    #[error("nothing to change: give --title, --context-file or --no-context")]
    NothingToChangeInStep,
}

/// Reads a flag's value the way the API spells it, such as `in_progress` or `nice_to_have`, into the API's type.
pub fn parse_api_value<T: DeserializeOwned>(raw: &str) -> Result<T, String> {
    T::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(raw))
        .map_err(|err| err.to_string())
}
