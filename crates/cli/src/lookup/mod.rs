//! Resolving what the user typed, a heap handle, an item, a due time, tag names, a saved view's or a parameter's name,
//! to the handles, IDs and values the API takes. Plain functions over what the server returned; they don't call it.

use api_types::common::{DueInputDto, ItemDto};
use api_types::due_choices::DueChoicesDto;
use api_types::me::MeDto;
use api_types::parameters::ParameterListDto;
use api_types::tags::{TagDto, TagListDto};
use api_types::views::{SavedViewDto, SavedViewListDto};
use uuid::Uuid;

/// A heap as the user named it: `team/heap`, or a heap slug alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeapName<'a> {
    Full { team: &'a str, heap: &'a str },
    Bare(&'a str),
}

/// A heap's handles: the slugs in its paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeapHandle {
    pub team: String,
    pub heap: String,
}

impl std::fmt::Display for HeapHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.team, self.heap)
    }
}

/// An item as the user named it: its key in the current heap, or its heap and key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemName {
    Key(String),
    Full { heap: HeapHandle, key: String },
}

/// A step as the user named it: an item and the step's number, as `k3x9a.3`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepName {
    /// The item part as typed, which messages name the item by.
    pub item_typed: String,
    pub item: ItemName,
    /// The step's number: a positive integer.
    pub number: u32,
}

/// A due time as the user typed it for `--due`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DueWhen {
    /// A calendar date, `YYYY-MM-DD`: an exact deadline.
    Date(String),
    /// Anything else: the label of one of the server's rough choices, such as `next week`.
    Rough(String),
}

#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    #[error("`{name}` isn't a heap: give `team/heap`, or a heap's slug")]
    MalformedHeap { name: String },
    #[error("`{name}` isn't an item: give its key, team/heap/key, or a link to it")]
    MalformedItem { name: String },
    #[error("`{name}` isn't a step: give its item and number, such as k3x9a.3")]
    MalformedStep { name: String },
    #[error("`{typed}` isn't a due time: give a date, YYYY-MM-DD, or one of: {}", known.join(", "))]
    UnknownDueChoice { typed: String, known: Vec<String> },
    #[error("you have no heap `{name}`; `upheap heap list` lists yours")]
    UnknownHeap { name: String },
    #[error("more than one of your teams has a heap `{slug}`: say which with {}", candidates.join(" or "))]
    AmbiguousHeap { slug: String, candidates: Vec<String> },
    #[error("{heap} has no tag `{name}`{}", closest_tags(known, *total))]
    UnknownTag {
        name: String,
        heap: String,
        /// The names closest to `name`, closest first: all of the heap's tags when it has few.
        known: Vec<String>,
        /// How many tags the heap has.
        total: usize,
        /// Whether the user meant to put the tag on an item, so creating it is the likely next step.
        offer_add: bool,
    },
    #[error("{heap} has no saved view `{name}`{}", known_list(known))]
    UnknownView {
        name: String,
        heap: String,
        known: Vec<String>,
    },
    #[error("{heap} has more than one saved view called `{name}`: name one by its ID, {}", ids.join(" or "))]
    AmbiguousView {
        name: String,
        heap: String,
        ids: Vec<String>,
    },
    #[error("{heap} has no parameter `{name}`{}", known_list(known))]
    UnknownParameter {
        name: String,
        heap: String,
        known: Vec<String>,
    },
}

fn known_list(known: &[String]) -> String {
    if known.is_empty() {
        String::new()
    } else {
        format!("; it has {}", known.join(", "))
    }
}

/// How many tags an unknown-tag error names before it says they are only the closest.
const NAMED_TAGS: usize = 5;

fn closest_tags(known: &[String], total: usize) -> String {
    if known.is_empty() {
        String::new()
    } else if total <= NAMED_TAGS {
        format!("; it has {}", known.join(", "))
    } else {
        format!("; the closest are {}", known.join(", "))
    }
}

impl<'a> HeapName<'a> {
    /// Reads `team/heap` or a heap slug. Anything with more parts, or an empty one, names no heap.
    pub fn parse(raw: &'a str) -> Result<Self, LookupError> {
        let malformed = || LookupError::MalformedHeap { name: raw.to_owned() };
        let mut parts = raw.split('/');
        let name = match (parts.next(), parts.next(), parts.next()) {
            (Some(heap), None, None) => Self::Bare(heap),
            (Some(team), Some(heap), None) => Self::Full { team, heap },
            _ => return Err(malformed()),
        };
        let empty = match name {
            Self::Bare(heap) => heap.is_empty(),
            Self::Full { team, heap } => team.is_empty() || heap.is_empty(),
        };
        if empty { Err(malformed()) } else { Ok(name) }
    }

    /// The handle when the name is complete without asking the server.
    pub fn to_handle(self) -> Option<HeapHandle> {
        match self {
            Self::Full { team, heap } => Some(HeapHandle {
                team: team.to_owned(),
                heap: heap.to_owned(),
            }),
            Self::Bare(_) => None,
        }
    }
}

impl ItemName {
    /// Reads a key, `team/heap/key`, or a link to an item (`http://` or `https://`, whose path ends in
    /// `/h/<team>/<heap>/<key>`, the short link, or `/teams/<team>/heaps/<heap>/items/<key>`, which the web app's
    /// address bar shows after redirecting the short one; its host, query and fragment are ignored, and so is one
    /// trailing slash). Anything else, or an empty part, names no item.
    pub fn parse(raw: &str) -> Result<Self, LookupError> {
        let malformed = || LookupError::MalformedItem { name: raw.to_owned() };
        if raw.starts_with("http://") || raw.starts_with("https://") {
            let url = reqwest::Url::parse(raw).map_err(|_| malformed())?;
            let mut segments: Vec<&str> = url.path_segments().ok_or_else(malformed)?.collect();
            if segments.last() == Some(&"") {
                segments.pop();
            }
            return match segments.as_slice() {
                [.., "h", team, heap, key] | [.., "teams", team, "heaps", heap, "items", key] => {
                    Self::from_parts(&[*team, *heap, *key]).ok_or_else(malformed)
                }
                _ => Err(malformed()),
            };
        }
        let parts: Vec<&str> = raw.split('/').collect();
        Self::from_parts(&parts).ok_or_else(malformed)
    }

    /// `[key]` or `[team, heap, key]`, none of them empty.
    fn from_parts(parts: &[&str]) -> Option<Self> {
        if parts.contains(&"") {
            return None;
        }
        match parts {
            [key] => Some(Self::Key((*key).to_owned())),
            [team, heap, key] => Some(Self::Full {
                heap: HeapHandle {
                    team: (*team).to_owned(),
                    heap: (*heap).to_owned(),
                },
                key: (*key).to_owned(),
            }),
            _ => None,
        }
    }
}

impl StepName {
    /// Reads `<item>.<number>`, where `<item>` is anything [`ItemName::parse`] takes and `<number>` a positive
    /// integer. It splits at the last `.` after the last `/`, since a link's host and path may hold dots before the
    /// key, and a key never does.
    pub fn parse(raw: &str) -> Result<Self, LookupError> {
        let malformed = || LookupError::MalformedStep { name: raw.to_owned() };
        let tail_start = raw.rfind('/').map_or(0, |slash| slash + 1);
        let (key, number) = raw[tail_start..].rsplit_once('.').ok_or_else(malformed)?;
        let number = parse_step_number(number)
            .filter(|_| !key.contains('.'))
            .ok_or_else(malformed)?;
        let item_typed = format!("{}{key}", &raw[..tail_start]);
        let item = ItemName::parse(&item_typed).map_err(|_| malformed())?;
        Ok(Self {
            item_typed,
            item,
            number,
        })
    }
}

/// A step number written in digits: positive, without a sign or a leading zero.
pub fn parse_step_number(raw: &str) -> Option<u32> {
    if raw.starts_with('0') || raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

impl DueWhen {
    /// A calendar date as it's written back (`2027-03-12`, not `2027-3-12` or a date and time) is a date; anything
    /// else is taken for a rough choice.
    pub fn parse(raw: &str) -> Self {
        match raw.parse::<jiff::civil::Date>() {
            Ok(date) if date.to_string() == raw => Self::Date(raw.to_owned()),
            _ => Self::Rough(raw.to_owned()),
        }
    }
}

/// The due time for a date: an exact deadline, so firm.
pub fn due_on(date: &str) -> DueInputDto {
    DueInputDto {
        date: date.to_owned(),
        firm: true,
    }
}

/// The due time for the rough choice `typed` names, comparing labels ignoring case and treating `-` as a space.
/// It carries the choice's date, loose, so the item reads back with the choice's label.
pub fn match_due_choice(typed: &str, choices: &DueChoicesDto) -> Result<DueInputDto, LookupError> {
    let wanted = normalize_label(typed);
    choices
        .choices
        .iter()
        .find(|choice| normalize_label(&choice.label) == wanted)
        .map(|choice| DueInputDto {
            date: choice.date.clone(),
            firm: false,
        })
        .ok_or_else(|| LookupError::UnknownDueChoice {
            typed: typed.to_owned(),
            known: choices.choices.iter().map(|choice| choice.label.clone()).collect(),
        })
}

/// Lower case, `-` as a space, runs of whitespace as one space, the ends trimmed.
fn normalize_label(label: &str) -> String {
    label
        .to_lowercase()
        .replace('-', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The caller's heap that `name` names, among the teams and heaps of `me`. A bare slug works when only one of the
/// caller's teams has a heap by it.
pub fn resolve_heap(name: HeapName<'_>, me: &MeDto) -> Result<HeapHandle, LookupError> {
    let found: Vec<HeapHandle> = me
        .teams
        .iter()
        .flat_map(|team| team.heaps.iter().map(move |heap| (team, heap)))
        .filter(|(team, heap)| match name {
            HeapName::Full { team: t, heap: h } => team.slug == t && heap.heap.slug == h,
            HeapName::Bare(h) => heap.heap.slug == h,
        })
        .map(|(team, heap)| HeapHandle {
            team: team.slug.clone(),
            heap: heap.heap.slug.clone(),
        })
        .collect();
    let typed = || match name {
        HeapName::Full { team, heap } => format!("{team}/{heap}"),
        HeapName::Bare(heap) => heap.to_owned(),
    };
    match <[HeapHandle; 1]>::try_from(found) {
        Ok([handle]) => Ok(handle),
        Err(found) if found.is_empty() => Err(LookupError::UnknownHeap { name: typed() }),
        Err(found) => Err(LookupError::AmbiguousHeap {
            slug: typed(),
            candidates: found.iter().map(ToString::to_string).collect(),
        }),
    }
}

/// The IDs of the tags `names` names, each by name ignoring case, as the server compares them, or by ID.
pub fn resolve_tags(names: &[String], tags: &TagListDto, heap: &HeapHandle) -> Result<Vec<Uuid>, LookupError> {
    names
        .iter()
        .map(|name| find_tag(name, tags, heap).map(|tag| tag.id))
        .collect()
}

/// The tag `name` names, trimmed, by name ignoring case, as the server compares them, or by ID. The first match wins. An
/// unknown one carries the names closest to it, by edit distance, ties in the server's order.
pub fn find_tag<'t>(name: &str, tags: &'t TagListDto, heap: &HeapHandle) -> Result<&'t TagDto, LookupError> {
    // The server trims names, so `"ops "` is the tag Ops.
    let trimmed = name.trim();
    tags.tags
        .iter()
        .find(|tag| is_named(trimmed, &tag.name, tag.id))
        .ok_or_else(|| {
            let typed = trimmed.to_lowercase();
            let mut closest: Vec<(usize, &str)> = tags
                .tags
                .iter()
                .map(|tag| (levenshtein(&typed, &tag.name.to_lowercase()), tag.name.as_str()))
                .collect();
            closest.sort_by_key(|(distance, _)| *distance);
            LookupError::UnknownTag {
                name: name.to_owned(),
                heap: heap.to_string(),
                known: closest
                    .into_iter()
                    .take(NAMED_TAGS)
                    .map(|(_, name)| name.to_owned())
                    .collect(),
                total: tags.tags.len(),
                offer_add: false,
            }
        })
}

impl LookupError {
    /// An unknown tag that the user meant to put on an item, whose hint offers the command that creates it. Other
    /// errors come back unchanged.
    pub fn offering_add(self) -> Self {
        match self {
            Self::UnknownTag {
                name,
                heap,
                known,
                total,
                ..
            } => Self::UnknownTag {
                name,
                heap,
                known,
                total,
                offer_add: true,
            },
            other => other,
        }
    }
}

/// The edit distance between `a` and `b`, in chars: the fewest single-char insertions, deletions and
/// substitutions that turn one into the other.
fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != *cb)).min(above + 1).min(row[j] + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The saved view `name` names, by name ignoring case, or by ID.
pub fn find_view<'v>(
    name: &str,
    views: &'v SavedViewListDto,
    heap: &HeapHandle,
) -> Result<&'v SavedViewDto, LookupError> {
    let found: Vec<&SavedViewDto> = views
        .views
        .iter()
        .filter(|view| is_named(name, &view.name, view.id))
        .collect();
    match found.as_slice() {
        [view] => Ok(view),
        [] => Err(LookupError::UnknownView {
            name: name.to_owned(),
            heap: heap.to_string(),
            known: views.views.iter().map(|view| view.name.clone()).collect(),
        }),
        several => Err(LookupError::AmbiguousView {
            name: name.to_owned(),
            heap: heap.to_string(),
            ids: several.iter().map(|view| view.id.to_string()).collect(),
        }),
    }
}

/// The ID of the parameter in use that `name` names, by name ignoring case, or by ID.
pub fn resolve_parameter(name: &str, parameters: &ParameterListDto, heap: &HeapHandle) -> Result<Uuid, LookupError> {
    parameters
        .parameters
        .iter()
        .find(|parameter| is_named(name, &parameter.name, parameter.id))
        .map(|parameter| parameter.id)
        .ok_or_else(|| LookupError::UnknownParameter {
            name: name.to_owned(),
            heap: heap.to_string(),
            known: parameters.parameters.iter().map(|p| p.name.clone()).collect(),
        })
}

fn is_named(typed: &str, name: &str, id: Uuid) -> bool {
    typed.to_lowercase() == name.to_lowercase() || typed.parse::<Uuid>().is_ok_and(|typed| typed == id)
}

/// Whether `item`'s title or description contains `text`, ignoring case: the client's one kind of filtering.
pub fn matches_text(item: &ItemDto, text: &str) -> bool {
    let text = text.to_lowercase();
    item.title.to_lowercase().contains(&text)
        || item
            .description
            .as_deref()
            .is_some_and(|description| description.to_lowercase().contains(&text))
}

#[cfg(test)]
mod tests;
