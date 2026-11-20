//! The Data page's figures: what agents spent, and how items and runs came and went, by day or by week.
//!
//! Pure: it takes the rows the store read and the client's time zone, and buckets them by calendar day or week in
//! that zone.

use jiff::Span;
use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::Timestamp;
use crate::item::StateKind;
use crate::run::RunEnd;

/// How far back the page looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    /// The last 7 days, a bucket per day.
    Days7,
    /// The last 30 days, a bucket per day.
    Days30,
    /// The last 13 weeks, a bucket per week.
    Days90,
}

impl Period {
    /// How many buckets the period has.
    pub const fn buckets(self) -> usize {
        match self {
            Self::Days7 => 7,
            Self::Days30 => 30,
            Self::Days90 => 13,
        }
    }

    const fn is_weekly(self) -> bool {
        matches!(self, Self::Days90)
    }
}

/// A run, as the figures count it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunRow {
    pub started_at: Timestamp,
    /// `None` while the run hasn't ended.
    pub end: Option<RunEnd>,
    pub cost_micro_usd: Option<i64>,
}

/// An item that was closed, as the figures count it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosedRow {
    pub closed_at: Timestamp,
    /// One of the closed states.
    pub state: StateKind,
}

/// How many items closed each way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClosedCounts {
    pub done: u32,
    pub wont_do: u32,
    pub became: u32,
}

/// How many runs ended each way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunCounts {
    pub reached: u32,
    pub asked: u32,
    pub failed: u32,
    /// Taken back, handed back, or not ended yet.
    pub other: u32,
}

/// The figures of one bucket, or of the whole period.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Figures {
    pub spend_micro_usd: i64,
    /// Runs that ended without a cost.
    pub runs_without_cost: u32,
    pub added: u32,
    pub closed: ClosedCounts,
    pub runs: RunCounts,
}

/// One day or week.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bucket {
    /// "14 Nov" for a day, "Week of 10 Nov" for a week.
    pub label: String,
    /// The last bucket: today, or this week, so far.
    pub partial: bool,
    pub figures: Figures,
}

/// The period's figures, oldest bucket first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataFigures {
    pub totals: Figures,
    pub buckets: Vec<Bucket>,
}

/// The calendar days or weeks of a period in a time zone: where each starts, and where the last ends.
#[derive(Clone, Debug)]
pub struct Buckets {
    labels: Vec<String>,
    /// The start of each bucket, then the end of the last one.
    bounds: Vec<Timestamp>,
}

impl Buckets {
    /// The buckets of `period` ending with the one that holds `now` in `tz`. `None` only when a bound falls outside
    /// the supported range of time.
    pub fn new(now: Timestamp, tz: &TimeZone, period: Period) -> Option<Self> {
        let today = now.to_jiff().to_zoned(tz.clone()).date();
        let last_start = if period.is_weekly() {
            today.checked_sub(Span::new().days(i64::from(today.weekday().to_monday_zero_offset())))
        } else {
            Ok(today)
        }
        .ok()?;
        let step = if period.is_weekly() {
            Span::new().weeks(1)
        } else {
            Span::new().days(1)
        };
        let count = i32::try_from(period.buckets()).ok()?;
        let first_start = last_start
            .checked_sub(step.checked_mul(i64::from(count - 1)).ok()?)
            .ok()?;

        let mut labels = Vec::new();
        let mut bounds = Vec::new();
        let mut start = first_start;
        for _ in 0..count {
            labels.push(label(start, period));
            bounds.push(Timestamp::at_start_of_day(start, tz)?);
            start = start.checked_add(step).ok()?;
        }
        bounds.push(Timestamp::at_start_of_day(start, tz)?);
        Some(Self { labels, bounds })
    }

    /// When the first bucket starts: rows from here on are in the period.
    pub fn from(&self) -> Timestamp {
        self.bounds[0]
    }

    /// When the last bucket ends: rows before this are in the period.
    pub fn to(&self) -> Timestamp {
        self.bounds[self.bounds.len() - 1]
    }

    fn index_of(&self, at: Timestamp) -> Option<usize> {
        if at < self.from() || at >= self.to() {
            return None;
        }
        Some(self.bounds.partition_point(|start| *start <= at) - 1)
    }

    /// Counts the rows into their buckets, and all of them into the totals.
    pub fn count(&self, runs: &[RunRow], created: &[Timestamp], closed: &[ClosedRow]) -> DataFigures {
        let mut figures = vec![Figures::default(); self.labels.len()];
        for run in runs {
            if let Some(index) = self.index_of(run.started_at) {
                let bucket = &mut figures[index];
                if let Some(cost) = run.cost_micro_usd {
                    bucket.spend_micro_usd += cost;
                } else if run.end.is_some() {
                    bucket.runs_without_cost += 1;
                }
                match run.end {
                    Some(RunEnd::Reached) => bucket.runs.reached += 1,
                    Some(RunEnd::Asked) => bucket.runs.asked += 1,
                    Some(RunEnd::Failed) => bucket.runs.failed += 1,
                    Some(RunEnd::TakenBack | RunEnd::HandedBack | RunEnd::Limited | RunEnd::Watching) | None => {
                        bucket.runs.other += 1
                    }
                }
            }
        }
        for at in created {
            if let Some(index) = self.index_of(*at) {
                figures[index].added += 1;
            }
        }
        for row in closed {
            if let Some(index) = self.index_of(row.closed_at) {
                let counts = &mut figures[index].closed;
                match row.state {
                    StateKind::Done => counts.done += 1,
                    StateKind::WontDo => counts.wont_do += 1,
                    StateKind::Became => counts.became += 1,
                    StateKind::Open | StateKind::InProgress | StateKind::Blocked => {}
                }
            }
        }

        let mut totals = Figures::default();
        for bucket in &figures {
            totals.spend_micro_usd += bucket.spend_micro_usd;
            totals.runs_without_cost += bucket.runs_without_cost;
            totals.added += bucket.added;
            totals.closed.done += bucket.closed.done;
            totals.closed.wont_do += bucket.closed.wont_do;
            totals.closed.became += bucket.closed.became;
            totals.runs.reached += bucket.runs.reached;
            totals.runs.asked += bucket.runs.asked;
            totals.runs.failed += bucket.runs.failed;
            totals.runs.other += bucket.runs.other;
        }
        let last = figures.len() - 1;
        let buckets = self
            .labels
            .iter()
            .zip(figures)
            .enumerate()
            .map(|(index, (label, figures))| Bucket {
                label: label.clone(),
                partial: index == last,
                figures,
            })
            .collect();
        DataFigures { totals, buckets }
    }
}

fn label(start: Date, period: Period) -> String {
    let day = start.strftime("%-d %b");
    if period.is_weekly() {
        format!("Week of {day}")
    } else {
        day.to_string()
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn at(date: &str, tz: &TimeZone) -> Timestamp {
        let zoned = date
            .parse::<jiff::civil::DateTime>()
            .expect("valid")
            .to_zoned(tz.clone())
            .expect("zoned");
        Timestamp::from_millis(zoned.timestamp().as_millisecond()).expect("in range")
    }

    fn utc() -> TimeZone {
        TimeZone::UTC
    }

    fn berlin() -> TimeZone {
        TimeZone::get("Europe/Berlin").expect("bundled tzdb has Berlin")
    }

    fn run(started_at: Timestamp, end: Option<RunEnd>, cost: Option<i64>) -> RunRow {
        RunRow {
            started_at,
            end,
            cost_micro_usd: cost,
        }
    }

    #[gtest]
    fn a_week_and_a_month_of_days_end_today() {
        // 2026-11-14 is a Saturday.
        let now = at("2026-11-14T10:00:00", &utc());

        for (period, count) in [(Period::Days7, 7), (Period::Days30, 30)] {
            let figures = Buckets::new(now, &utc(), period)
                .expect("in range")
                .count(&[], &[], &[]);

            expect_that!(figures.buckets.len(), eq(count));
            let last = figures.buckets.last().expect("has buckets");
            expect_that!(last.label.as_str(), eq("14 Nov"));
            expect_that!(last.partial, eq(true));
            expect_that!(figures.buckets.iter().filter(|b| b.partial).count(), eq(1));
        }
        let week = Buckets::new(now, &utc(), Period::Days7)
            .expect("in range")
            .count(&[], &[], &[]);
        expect_that!(week.buckets[0].label.as_str(), eq("8 Nov"));
    }

    #[gtest]
    fn ninety_days_is_thirteen_weeks_starting_monday_ending_this_week() {
        let now = at("2026-11-14T10:00:00", &utc());

        let figures = Buckets::new(now, &utc(), Period::Days90)
            .expect("in range")
            .count(&[], &[], &[]);

        expect_that!(figures.buckets.len(), eq(13));
        // This week's Monday is 9 Nov; 12 weeks before it is 17 Aug.
        expect_that!(figures.buckets[0].label.as_str(), eq("Week of 17 Aug"));
        expect_that!(figures.buckets[12].label.as_str(), eq("Week of 9 Nov"));
        expect_that!(figures.buckets[12].partial, eq(true));
        expect_that!(figures.buckets[11].partial, eq(false));
    }

    #[gtest]
    fn a_late_evening_falls_on_the_local_day() {
        // 23:30 on 13 Nov in New York is 04:30 on 14 Nov in UTC.
        let new_york = TimeZone::get("America/New_York").expect("bundled tzdb");
        let now = at("2026-11-14T10:00:00", &new_york);
        let evening = at("2026-11-13T23:30:00", &new_york);

        let figures = Buckets::new(now, &new_york, Period::Days7)
            .expect("in range")
            .count(&[], &[evening], &[]);

        expect_that!(figures.buckets[5].label.as_str(), eq("13 Nov"));
        expect_that!(figures.buckets[5].figures.added, eq(1));
        expect_that!(figures.buckets[6].figures.added, eq(0));
    }

    #[gtest]
    fn a_day_with_a_clock_change_is_one_bucket() {
        // Europe/Berlin goes back on 25 Oct 2026: that day has 25 hours.
        let now = at("2026-10-26T12:00:00", &berlin());
        let buckets = Buckets::new(now, &berlin(), Period::Days7).expect("in range");
        let late = at("2026-10-25T23:30:00", &berlin());
        let next = at("2026-10-26T00:00:00", &berlin());

        let figures = buckets.count(&[], &[late, next], &[]);

        expect_that!(figures.buckets[5].label.as_str(), eq("25 Oct"));
        expect_that!(figures.buckets[5].figures.added, eq(1));
        expect_that!(figures.buckets[6].figures.added, eq(1));
        expect_that!(
            buckets.bounds[6].since(buckets.bounds[5]).as_millis(),
            eq(25 * 3_600_000)
        );
    }

    #[gtest]
    fn rows_outside_the_period_are_not_counted() {
        let now = at("2026-11-14T10:00:00", &utc());
        let before = at("2026-11-07T23:59:59", &utc());
        let after = at("2026-11-15T00:00:00", &utc());

        let figures = Buckets::new(now, &utc(), Period::Days7)
            .expect("in range")
            .count(&[], &[before, after], &[]);

        expect_that!(figures.totals.added, eq(0));
    }

    #[gtest]
    fn runs_count_by_how_they_ended_and_what_they_cost() {
        let now = at("2026-11-14T10:00:00", &utc());
        let day = at("2026-11-14T01:00:00", &utc());
        let runs = [
            run(day, Some(RunEnd::Reached), Some(1_500_000)),
            run(day, Some(RunEnd::Asked), None),
            run(day, Some(RunEnd::Failed), Some(250_000)),
            run(day, Some(RunEnd::TakenBack), None),
            run(day, Some(RunEnd::HandedBack), Some(0)),
            run(day, Some(RunEnd::Limited), Some(0)),
            run(day, None, None),
        ];

        let figures = Buckets::new(now, &utc(), Period::Days7)
            .expect("in range")
            .count(&runs, &[], &[]);

        let today = figures.buckets[6].figures;
        expect_that!(today.spend_micro_usd, eq(1_750_000));
        // The asked and taken-back runs ended without a cost; the running one hasn't had the chance to report.
        expect_that!(today.runs_without_cost, eq(2));
        expect_that!(
            today.runs,
            eq(RunCounts {
                reached: 1,
                asked: 1,
                failed: 1,
                other: 4
            })
        );
        expect_that!(figures.totals, eq(today));
        expect_that!(figures.buckets[0].figures, eq(Figures::default()));
    }

    #[gtest]
    fn items_count_by_how_they_closed() {
        let now = at("2026-11-14T10:00:00", &utc());
        let day = at("2026-11-13T09:00:00", &utc());
        let closed = [
            ClosedRow {
                closed_at: day,
                state: StateKind::Done,
            },
            ClosedRow {
                closed_at: day,
                state: StateKind::WontDo,
            },
            ClosedRow {
                closed_at: day,
                state: StateKind::Became,
            },
            ClosedRow {
                closed_at: day,
                state: StateKind::Done,
            },
        ];

        let figures = Buckets::new(now, &utc(), Period::Days7)
            .expect("in range")
            .count(&[], &[day], &closed);

        let yesterday = figures.buckets[5].figures;
        expect_that!(
            yesterday.closed,
            eq(ClosedCounts {
                done: 2,
                wont_do: 1,
                became: 1
            })
        );
        expect_that!(yesterday.added, eq(1));
    }

    #[gtest]
    fn a_week_bucket_takes_every_day_of_the_week() {
        let now = at("2026-11-14T10:00:00", &utc());
        let monday = at("2026-11-09T00:00:00", &utc());
        let sunday_before = at("2026-11-08T23:59:59", &utc());

        let figures =
            Buckets::new(now, &utc(), Period::Days90)
                .expect("in range")
                .count(&[], &[monday, sunday_before], &[]);

        expect_that!(figures.buckets[12].figures.added, eq(1));
        expect_that!(figures.buckets[11].figures.added, eq(1));
    }
}
