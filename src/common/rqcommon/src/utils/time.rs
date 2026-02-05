use std::time::Instant;
use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::{Tz};

pub fn localtimeonly2future_datetime_tz(tz: Tz, target_time_tz: NaiveTime) -> DateTime<Tz> { // target_time_tz (11:59) is local in the tz timezone. Use today or tommorrow, whichever is in the future.
    let now_tz = Utc::now().with_timezone(&tz);
    let today_target_tz = now_tz.date_naive().and_time(target_time_tz);
    let future_target_tz = if now_tz.time() < target_time_tz { // if target_time_tz is later today in the future, use that. Otherwise, use tomorrow
        today_target_tz
    } else {
        (now_tz + Duration::days(1)).date_naive().and_time(target_time_tz)
    };
    // Handle DST (ambiguous/invalid) by picking earliest valid local time; Note that the calculalated local time can be invalid, when we set the time 1 hour forward during spring forward.
    tz.from_local_datetime(&future_target_tz)
        .earliest()
        .unwrap_or(DateTime::<Utc>::MIN_UTC.with_timezone(&tz)) // If the local time is invalid (e.g., during spring forward), return a very old datetime as a fallback. This should be handled properly by the caller.
}

pub fn benchmark_elapsed_time(name: &str, f: impl FnOnce()) {
    let start = Instant::now();
    f();
    let elapsed_microsec = start.elapsed().as_secs_f64() * 1_000_000.0;
    println!("Elapsed Time of {}: {:.2}us", name, elapsed_microsec); // TODO: no native support thousand separators in float or int. Use crate 'num-format' or 'thousands' or better: write a lightweight formatter train in RqCommon
}

pub async fn benchmark_elapsed_time_async<F, Fut>(name: &str, f: F)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let start = Instant::now();
    f().await;
    let elapsed_microsec = start.elapsed().as_secs_f64() * 1_000_000.0;
    println!("Elapsed Time of {}: {:.2}us", name, elapsed_microsec);
}