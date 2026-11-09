//! Resolving what the user typed, a heap handle, tag names, a saved view's or a parameter's name, to the handles and
//! IDs the API takes. Plain functions over what the server returned; they don't call it.

use api_types::common::ItemDto;
use api_types::me::MeDto;
use api_types::parameters::ParameterListDto;
use api_types::tags::TagListDto;
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

#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    #[error("`{name}` isn't a heap: give `team/heap`, or a heap's slug")]
    MalformedHeap { name: String },
    #[error("you have no heap `{name}`; `upheap heap list` lists yours")]
    UnknownHeap { name: String },
    #[error("more than one of your teams has a heap `{slug}`: say which with {}", candidates.join(" or "))]
    AmbiguousHeap { slug: String, candidates: Vec<String> },
    #[error("{heap} has no tag `{name}`{}", known_list(known))]
    UnknownTag {
        name: String,
        heap: String,
        known: Vec<String>,
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
        .map(|name| {
            tags.tags
                .iter()
                .find(|tag| is_named(name, &tag.name, tag.id))
                .map(|tag| tag.id)
                .ok_or_else(|| LookupError::UnknownTag {
                    name: name.clone(),
                    heap: heap.to_string(),
                    known: tags.tags.iter().map(|tag| tag.name.clone()).collect(),
                })
        })
        .collect()
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
