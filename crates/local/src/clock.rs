use domain::Timestamp;
use service::Clock;

/// The browser's wall clock. `SystemTime::now()` panics on `wasm32-unknown-unknown`.
#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserClock;

impl Clock for BrowserClock {
    fn now(&self) -> Timestamp {
        // `Date.now()` is whole milliseconds since the epoch, so the cast loses nothing.
        let millis = js_sys::Date::now() as i64;
        Timestamp::from_millis(millis).unwrap_or(Timestamp::EPOCH)
    }
}
