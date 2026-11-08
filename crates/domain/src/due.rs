//! Presentation of due times (fuzzy labels for loose due times, dates for exact deadlines) and of other instants.

use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::Timestamp;
use crate::item::Due;

/// How a due time is shown to the user at a given moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueView {
    pub label: String,
    /// Only exact deadlines become overdue.
    pub overdue: bool,
}

/// Describes `due` as seen at `as_of` by a user in `tz`.
///
/// Loose due times get a fuzzy label that never says "overdue": once their date has passed they keep the
/// nearest term. Exact deadlines get a date ("today", "tomorrow", "12 Mar") and become overdue after it.
pub fn describe_due(due: Due, as_of: Timestamp, tz: &TimeZone) -> DueView {
    if !due.firm {
        return DueView {
            label: fuzzy_label(due.at, as_of).to_owned(),
            overdue: false,
        };
    }
    DueView {
        label: describe_date(due.at, as_of, tz),
        overdue: due.at < as_of,
    }
}

/// A short calendar label for `at` relative to `as_of` in `tz`: "today", "tomorrow", "yesterday", "12 Mar", or
/// "12 Mar 2027" when the year differs.
pub fn describe_date(at: Timestamp, as_of: Timestamp, tz: &TimeZone) -> String {
    let date = local_date(at, tz);
    let today = local_date(as_of, tz);
    if date == today {
        return "today".to_owned();
    }
    if today.tomorrow().is_ok_and(|tomorrow| tomorrow == date) {
        return "tomorrow".to_owned();
    }
    if today.yesterday().is_ok_and(|yesterday| yesterday == date) {
        return "yesterday".to_owned();
    }
    let format = if date.year() == today.year() {
        "%-d %b"
    } else {
        "%-d %b %Y"
    };
    date.strftime(format).to_string()
}

/// `due` as a fixed calendar date in `tz`, for records that are read long after they were written, such as the audit
/// log: "12 Mar 2027" for an exact deadline, "around 12 Mar 2027" for a loose due time. Fuzzy and relative labels
/// would be wrong by the time anyone reads them.
pub fn describe_due_date(due: Due, tz: &TimeZone) -> String {
    let date = local_date(due.at, tz).strftime("%-d %b %Y").to_string();
    if due.firm { date } else { format!("around {date}") }
}

/// When something happened, in `tz`: "12 Mar 2027, 14:05".
pub fn describe_time(at: Timestamp, tz: &TimeZone) -> String {
    at.to_jiff()
        .to_zoned(tz.clone())
        .strftime("%-d %b %Y, %H:%M")
        .to_string()
}

/// How long ago `at` was, as seen at `as_of` by a user in `tz`, in calendar days: "today", "yesterday", "4 days ago",
/// "3 weeks ago", "5 months ago", "2 years ago". An `at` after `as_of` counts as today.
pub fn describe_ago(at: Timestamp, as_of: Timestamp, tz: &TimeZone) -> String {
    let days = local_date(at, tz)
        .until(local_date(as_of, tz))
        .map_or(0, |span| span.get_days());
    match days {
        ..1 => "today".to_owned(),
        1 => "yesterday".to_owned(),
        2..14 => format!("{days} days ago"),
        14..61 => format!("{} weeks ago", days / 7),
        61..730 => format!("{} months ago", days / 30),
        _ => format!("{} years ago", days / 365),
    }
}

/// The nearest fuzzy term for a loose due time. Past dates count as "in a few days".
///
/// The bands leave room around each of the [due choices](list_due_choices), so a picked choice reads back as itself
/// whatever the time of day.
pub fn fuzzy_label(at: Timestamp, as_of: Timestamp) -> &'static str {
    let days = at.since(as_of).as_days_f64();
    match days {
        d if d <= 5.0 => "in a few days",
        d if d <= 10.0 => "next week",
        d if d <= 24.0 => "in a few weeks",
        d if d <= 45.0 => "next month",
        d if d <= 182.0 => "in a few months",
        _ => "in several months",
    }
}

/// A rough due time offered in place of a date: the date it stands for and what it's called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueChoice {
    /// The fuzzy label a loose due time on `date` has when picked, such as "next week".
    pub label: &'static str,
    /// The calendar date, in the timezone the choices were listed for.
    pub date: Date,
}

/// Days from today that the due choices stand for: in a few days, next week, in a few weeks, next month, in a few
/// months.
const DUE_CHOICE_DAYS: [i8; 5] = [3, 7, 14, 30, 90];

/// The rough due times to offer at `as_of` to a user in `tz`, nearest first.
///
/// Each label is the one [`describe_due`] gives a loose due time on that date at `as_of`, stored as the end of the
/// day like any entered date, so a picked choice reads back with the label it was picked by.
pub fn list_due_choices(as_of: Timestamp, tz: &TimeZone) -> Vec<DueChoice> {
    let today = local_date(as_of, tz);
    DUE_CHOICE_DAYS
        .iter()
        .filter_map(|&days| {
            let date = today.checked_add(jiff::Span::new().days(days)).ok()?;
            let at = Timestamp::at_end_of_day(date, tz)?;
            Some(DueChoice {
                label: fuzzy_label(at, as_of),
                date,
            })
        })
        .collect()
}

fn local_date(at: Timestamp, tz: &TimeZone) -> Date {
    at.to_jiff().to_zoned(tz.clone()).date()
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;
    use crate::Duration;

    fn utc() -> TimeZone {
        TimeZone::UTC
    }

    fn day(n: i64) -> Timestamp {
        // 2027-01-01T12:00:00Z plus n days.
        Timestamp::from_millis(1_798_804_800_000 + Duration::from_days(n).as_millis()).expect("in range")
    }

    #[gtest]
    fn loose_due_times_are_fuzzy_and_never_overdue() {
        let past = Due {
            at: day(-10),
            firm: false,
        };
        let soon = Due {
            at: day(3),
            firm: false,
        };
        let later = Due {
            at: day(20),
            firm: false,
        };
        let far = Due {
            at: day(300),
            firm: false,
        };

        expect_that!(
            describe_due(past, day(0), &utc()),
            eq(&DueView {
                label: "in a few days".into(),
                overdue: false
            })
        );
        expect_that!(describe_due(soon, day(0), &utc()).label, eq("in a few days"));
        expect_that!(describe_due(later, day(0), &utc()).label, eq("in a few weeks"));
        expect_that!(describe_due(far, day(0), &utc()).label, eq("in several months"));
    }

    #[gtest]
    fn exact_deadlines_show_dates_and_go_overdue() {
        let tomorrow = Due { at: day(1), firm: true };
        let past = Due {
            at: day(-3),
            firm: true,
        };

        expect_that!(
            describe_due(tomorrow, day(0), &utc()),
            eq(&DueView {
                label: "tomorrow".into(),
                overdue: false
            })
        );
        expect_that!(
            describe_due(past, day(0), &utc()),
            eq(&DueView {
                label: "29 Dec 2026".into(),
                overdue: true
            })
        );
    }

    #[gtest]
    fn recorded_due_dates_are_absolute() {
        expect_that!(
            describe_due_date(Due { at: day(1), firm: true }, &utc()),
            eq("2 Jan 2027")
        );
        expect_that!(
            describe_due_date(
                Due {
                    at: day(1),
                    firm: false
                },
                &utc()
            ),
            eq("around 2 Jan 2027")
        );
    }

    #[gtest]
    fn due_choices_are_the_rough_terms_in_order() {
        let choices = list_due_choices(day(0), &utc());

        let labels: Vec<_> = choices.iter().map(|choice| choice.label).collect();
        expect_that!(
            labels,
            elements_are![
                eq(&"in a few days"),
                eq(&"next week"),
                eq(&"in a few weeks"),
                eq(&"next month"),
                eq(&"in a few months")
            ]
        );
        expect_that!(choices[1].date.to_string(), eq("2027-01-08"));
    }

    #[gtest]
    fn a_picked_due_choice_reads_back_with_its_label() {
        // At every hour of a day, in timezones either side of UTC and across a DST change, a choice stored as a loose
        // due time at the end of its day is described with the label it was offered with.
        let zones = [
            "UTC",
            "Pacific/Kiritimati",
            "Pacific/Pago_Pago",
            "Europe/Berlin",
            "America/New_York",
        ];
        // 2027-03-10, so the choices span the March DST changes.
        let start = day(68);
        for zone in zones {
            let tz = TimeZone::get(zone).expect("bundled");
            for hour in 0..24 {
                let as_of = start.checked_add(Duration::from_hours(hour)).expect("in range");
                for choice in list_due_choices(as_of, &tz) {
                    let due = Due {
                        at: Timestamp::at_end_of_day(choice.date, &tz).expect("in range"),
                        firm: false,
                    };
                    expect_that!(
                        describe_due(due, as_of, &tz).label,
                        eq(choice.label),
                        "{zone} at {hour}:00, {}",
                        choice.date
                    );
                }
            }
        }
    }

    #[gtest]
    fn times_are_shown_in_the_timezone() {
        let berlin = TimeZone::get("Europe/Berlin").expect("bundled");
        expect_that!(describe_time(day(0), &utc()), eq("1 Jan 2027, 12:00"));
        expect_that!(describe_time(day(0), &berlin), eq("1 Jan 2027, 13:00"));
    }

    #[gtest]
    fn how_long_ago_counts_calendar_days() {
        expect_that!(describe_ago(day(0), day(0), &utc()), eq("today"));
        expect_that!(describe_ago(day(1), day(0), &utc()), eq("today"));
        expect_that!(describe_ago(day(-1), day(0), &utc()), eq("yesterday"));
        expect_that!(describe_ago(day(-4), day(0), &utc()), eq("4 days ago"));
        expect_that!(describe_ago(day(-13), day(0), &utc()), eq("13 days ago"));
        expect_that!(describe_ago(day(-21), day(0), &utc()), eq("3 weeks ago"));
        expect_that!(describe_ago(day(-150), day(0), &utc()), eq("5 months ago"));
        expect_that!(describe_ago(day(-800), day(0), &utc()), eq("2 years ago"));
    }

    #[gtest]
    fn how_long_ago_depends_on_the_timezone() {
        // 2027-01-01T23:30Z is still 1 Jan in UTC but already 2 Jan in Berlin.
        let as_of = Timestamp::from_millis(1_798_846_200_000).expect("in range");
        let berlin = TimeZone::get("Europe/Berlin").expect("bundled");

        expect_that!(describe_ago(day(0), as_of, &utc()), eq("today"));
        expect_that!(describe_ago(day(0), as_of, &berlin), eq("yesterday"));
    }

    #[gtest]
    fn dates_depend_on_the_timezone() {
        // 2027-01-01T23:30Z is still 1 Jan in UTC but already 2 Jan in Berlin.
        let as_of = Timestamp::from_millis(1_798_846_200_000).expect("in range");
        let berlin = TimeZone::get("Europe/Berlin").expect("bundled");

        expect_that!(describe_date(as_of, day(0), &utc()), eq("today"));
        expect_that!(describe_date(as_of, day(0), &berlin), eq("tomorrow"));
    }
}
