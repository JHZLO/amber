// src/lib/date.ts 와 holidays.ts 를 옮긴 것. 날짜는 'YYYY-MM-DD' 문자열로 주고받는다(지금 앱과 같다).
// 주는 일요일에 시작한다(WEEK_STARTS_ON = 0, 앱 전체의 '주' 정의). 음력 공휴일 표와 임시공휴일 표는
// holidays.ts 에서 빌드할 때 읽어 온다(build.rs).

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone};
use serde::Serialize;
use serde_json::{Map, Value as Json, json};

use crate::i18n::{is_ko, t};

include!(concat!(env!("OUT_DIR"), "/holidays.rs"));

pub const WEEK_STARTS_ON: u32 = 0;

const WEEKDAYS_KO: [&str; 7] = ["일", "월", "화", "수", "목", "금", "토"];
const WEEKDAYS_EN: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WEEKDAYS_EN_LONG: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTHS_EN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTHS_EN_LONG: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub fn parse(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

pub fn ymd(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// 일요일 = 0 (JS 의 getDay)
fn dow(d: NaiveDate) -> u32 {
    d.weekday().num_days_from_sunday()
}

/// 앱 시계(ms)의 로컬 날짜
pub fn today_str(now_ms: i64) -> String {
    Local
        .timestamp_millis_opt(now_ms)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

pub fn shift_day(s: &str, delta: i64) -> String {
    parse(s).map_or_else(|| "NaN-NaN-NaN".into(), |d| ymd(d + Duration::days(delta)))
}

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
pub struct YearMonth {
    pub year: i32,
    pub month: u32,
}

pub fn add_months(cur: YearMonth, delta: i32) -> YearMonth {
    let total = cur.year * 12 + cur.month as i32 - 1 + delta;
    YearMonth {
        year: total.div_euclid(12),
        month: total.rem_euclid(12) as u32 + 1,
    }
}

/// 그 날짜가 속한 주의 시작(일요일)
pub fn week_start_of(s: &str) -> String {
    parse(s).map_or_else(
        || "NaN-NaN-NaN".into(),
        |d| ymd(d - Duration::days(((dow(d) + 7 - WEEK_STARTS_ON) % 7) as i64)),
    )
}

pub fn week_days(week_start: &str) -> Vec<String> {
    (0..7).map(|i| shift_day(week_start, i)).collect()
}

pub fn month_of(s: &str) -> Option<YearMonth> {
    parse(s).map(|d| YearMonth {
        year: d.year(),
        month: d.month(),
    })
}

/// 로컬 날짜의 [자정, 다음날 자정) — UTC ms
pub fn day_range_ms(s: &str) -> Option<[i64; 2]> {
    let d = parse(s)?;
    let midnight = |d: NaiveDate| {
        Local
            .from_local_datetime(&d.and_hms_opt(0, 0, 0)?)
            .earliest()
            .map(|t| t.timestamp_millis())
    };
    Some([midnight(d)?, midnight(d + Duration::days(1))?])
}

/// 월 달력 격자의 날짜들 — 필요한 주(4~6주)만, 앞뒤 달 날짜가 섞인다. starts_on 이 한 행의 첫 요일
pub fn month_grid_dates(year: i32, month: u32, starts_on: u32) -> Vec<String> {
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return Vec::new();
    };
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap();
    let last = next - Duration::days(1);
    let start = first - Duration::days(((dow(first) + 7 - starts_on) % 7) as i64);
    let end = last + Duration::days(((starts_on + 6 + 7 - dow(last)) % 7) as i64);
    start
        .iter_days()
        .take_while(|d| *d <= end)
        .map(ymd)
        .collect()
}

/// ko "11월 5일 목요일" / en "Thursday, November 5"
pub fn format_day_long(s: &str) -> String {
    let Some(d) = parse(s) else {
        return "Invalid Date".into();
    };
    if is_ko() {
        format!(
            "{}월 {}일 {}요일",
            d.month(),
            d.day(),
            WEEKDAYS_KO[dow(d) as usize]
        )
    } else {
        format!(
            "{}, {} {}",
            WEEKDAYS_EN_LONG[dow(d) as usize],
            MONTHS_EN_LONG[d.month0() as usize],
            d.day()
        )
    }
}

/// "목" / "Thu"
pub fn weekday_short(s: &str) -> String {
    let Some(d) = parse(s) else {
        return "Invalid Date".into();
    };
    (if is_ko() { WEEKDAYS_KO } else { WEEKDAYS_EN })[dow(d) as usize].to_string()
}

/// "7/14"
pub fn format_day_short(s: &str) -> String {
    parse(s).map_or_else(
        || "NaN/NaN".into(),
        |d| format!("{}/{}", d.month(), d.day()),
    )
}

/// ko "2026년 11월" / en "November 2026"
pub fn format_month_title(year: i32, month: u32) -> String {
    if is_ko() {
        format!("{year}년 {month}월")
    } else {
        format!("{} {year}", MONTHS_EN_LONG[(month as usize + 11) % 12])
    }
}

pub fn format_year_title(year: i32) -> String {
    if is_ko() {
        format!("{year}년")
    } else {
        year.to_string()
    }
}

pub fn months_short() -> Vec<String> {
    if is_ko() {
        (1..=12).map(|m| format!("{m}월")).collect()
    } else {
        MONTHS_EN.iter().map(|m| m.to_string()).collect()
    }
}

/// 요일 짧은 표기 — starts_on 이 1 이면 월요일부터
pub fn weekdays_short(starts_on: u32) -> Vec<String> {
    let base = if is_ko() { WEEKDAYS_KO } else { WEEKDAYS_EN };
    (0..7)
        .map(|i| base[(i + starts_on as usize) % 7].to_string())
        .collect()
}

// ---- 공휴일 (holidays.ts) ----

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Holiday {
    pub name: String,
    pub substitute: bool,
}

/// 대체공휴일 규칙 — 「관공서의 공휴일에 관한 규정」 제3조(holidays.ts 의 SubRule)
#[derive(Clone, Copy, PartialEq)]
enum Sub {
    None,
    Sun,
    Weekend,
}

fn base_holidays(year: i32) -> Vec<(String, &'static str, Sub)> {
    let d = |m: u32, day: u32| {
        NaiveDate::from_ymd_opt(year, m, day)
            .map(ymd)
            .unwrap_or_default()
    };
    let mut out = vec![
        (d(1, 1), "todos.hol.newYear", Sub::None),
        (d(3, 1), "todos.hol.independence", Sub::Weekend),
        (d(5, 5), "todos.hol.children", Sub::Weekend),
        (d(6, 6), "todos.hol.memorial", Sub::None),
    ];
    if year >= CONSTITUTION_DAY_FROM {
        out.push((d(7, 17), "todos.hol.constitution", Sub::Weekend));
    }
    out.extend([
        (d(8, 15), "todos.hol.liberation", Sub::Weekend),
        (d(10, 3), "todos.hol.foundation", Sub::Weekend),
        (d(10, 9), "todos.hol.hangeul", Sub::Weekend),
        (d(12, 25), "todos.hol.christmas", Sub::Weekend),
    ]);
    if let Some((_, [seollal, buddha, chuseok])) = LUNAR.iter().find(|(y, _)| *y == year) {
        let (seollal, buddha, chuseok) = (
            format!("{year}-{seollal}"),
            format!("{year}-{buddha}"),
            format!("{year}-{chuseok}"),
        );
        out.extend([
            (
                shift_day(&seollal, -1),
                "todos.hol.seollalHoliday",
                Sub::Sun,
            ),
            (seollal.clone(), "todos.hol.seollal", Sub::Sun),
            (shift_day(&seollal, 1), "todos.hol.seollalHoliday", Sub::Sun),
            (buddha, "todos.hol.buddha", Sub::Weekend),
            (
                shift_day(&chuseok, -1),
                "todos.hol.chuseokHoliday",
                Sub::Sun,
            ),
            (chuseok.clone(), "todos.hol.chuseok", Sub::Sun),
            (shift_day(&chuseok, 1), "todos.hol.chuseokHoliday", Sub::Sun),
        ]);
    }
    for (date, key) in ONE_OFF {
        if date.starts_with(&format!("{year}-")) {
            out.push((date.to_string(), key, Sub::None));
        }
    }
    // 안정 정렬 — 같은 날짜는 넣은 순서대로(JS 의 sort 와 같다)
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn needs_substitute(entries: &[Sub], date: &str) -> bool {
    let w = parse(date).map_or(0, dow);
    if w == 0 || w == 6 {
        entries
            .iter()
            .any(|e| *e == Sub::Weekend || (*e == Sub::Sun && w == 0))
    } else {
        entries.len() > 1 && entries.iter().any(|e| *e != Sub::None)
    }
}

/// 그 해의 공휴일 전부(대체공휴일 포함). 넣은 순서 = 본래 공휴일 날짜순, 그 뒤 대체공휴일
pub fn holidays_of_year(year: i32) -> Vec<(String, Holiday)> {
    let mut by_date: Vec<(String, Vec<(&'static str, Sub)>)> = Vec::new();
    for (date, key, sub) in base_holidays(year) {
        match by_date.iter_mut().find(|(d, _)| *d == date) {
            Some((_, list)) => list.push((key, sub)),
            None => by_date.push((date, vec![(key, sub)])),
        }
    }
    let mut out: Vec<(String, Holiday)> = by_date
        .iter()
        .map(|(date, entries)| {
            let mut names: Vec<String> = Vec::new();
            for (key, _) in entries {
                let name = t(key);
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            // 겹친 날의 이름은 지금 앱과 같은 구분자(U+00B7)로 잇는다
            (
                date.clone(),
                Holiday {
                    name: names.join("\u{b7}"),
                    substitute: false,
                },
            )
        })
        .collect();
    let mut dates: Vec<&(String, Vec<(&str, Sub)>)> = by_date.iter().collect();
    dates.sort_by(|a, b| a.0.cmp(&b.0));
    for (date, entries) in dates {
        let subs: Vec<Sub> = entries.iter().map(|(_, s)| *s).collect();
        if !needs_substitute(&subs, date) {
            continue;
        }
        let mut d = shift_day(date, 1);
        while parse(&d).is_some_and(|x| dow(x) == 0) || out.iter().any(|(o, _)| *o == d) {
            d = shift_day(&d, 1);
        }
        out.push((
            d,
            Holiday {
                name: t("todos.hol.substitute"),
                substitute: true,
            },
        ));
    }
    out
}

/// 그 날짜의 공휴일. 연도를 못 읽으면 없음(표 밖 연도는 양력 공휴일만 안다)
pub fn holiday_of(date: &str) -> Option<Holiday> {
    let year: i32 = date.get(..4)?.parse().ok()?;
    holidays_of_year(year)
        .into_iter()
        .find(|(d, _)| d == date)
        .map(|(_, h)| h)
}

pub fn has_lunar_data(year: i32) -> bool {
    LUNAR.iter().any(|(y, _)| *y == year)
}

/// 하네스가 부르는 이름 → 결과(JSON). lib.rs 의 call 이 넘긴다
pub fn call(op: &str, args: &[Json], now_ms: i64) -> Option<Result<Json, String>> {
    let s = |i: usize| args.get(i).and_then(Json::as_str).unwrap_or_default();
    let n = |i: usize| args.get(i).and_then(Json::as_i64);
    let to = |v: Json| Some(Ok(v));
    match op {
        "date.todayStr" => to(json!(today_str(now_ms))),
        "date.shiftDay" => to(json!(shift_day(s(0), n(1).unwrap_or(0)))),
        "date.addMonths" => {
            let cur = args.first()?;
            let ym = YearMonth {
                year: cur["year"].as_i64()? as i32,
                month: cur["month"].as_u64()? as u32,
            };
            to(json!(add_months(ym, n(1)? as i32)))
        }
        "date.weekStartOf" => to(json!(week_start_of(s(0)))),
        "date.weekDays" => to(json!(week_days(s(0)))),
        "date.monthOf" => to(json!(month_of(s(0)))),
        "date.dayRangeMs" => to(json!(day_range_ms(s(0)))),
        "date.monthGridDates" => to(json!(month_grid_dates(
            n(0)? as i32,
            n(1)? as u32,
            n(2).unwrap_or(WEEK_STARTS_ON as i64) as u32
        ))),
        "date.formatDayLong" => to(json!(format_day_long(s(0)))),
        "date.weekdayShort" => to(json!(weekday_short(s(0)))),
        "date.formatDayShort" => to(json!(format_day_short(s(0)))),
        "date.formatMonthTitle" => to(json!(format_month_title(n(0)? as i32, n(1)? as u32))),
        "date.formatYearTitle" => to(json!(format_year_title(n(0)? as i32))),
        "date.monthsShort" => to(json!(months_short())),
        "date.weekdaysShort" => to(json!(weekdays_short(
            n(0).unwrap_or(WEEK_STARTS_ON as i64) as u32
        ))),
        "holidays.holidaysOfYear" => {
            let map: Map<String, Json> = holidays_of_year(n(0)? as i32)
                .into_iter()
                .map(|(d, h)| (d, json!(h)))
                .collect();
            to(Json::Object(map))
        }
        "holidays.holidayOf" => to(json!(holiday_of(s(0)))),
        "holidays.hasLunarData" => to(json!(has_lunar_data(n(0)? as i32))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_grid_starts_on_sunday_and_fills_weeks() {
        let g = month_grid_dates(2026, 11, 0);
        assert_eq!(g.first().map(String::as_str), Some("2026-11-01"));
        assert_eq!(g.last().map(String::as_str), Some("2026-12-05"));
        assert_eq!(g.len() % 7, 0);
    }

    #[test]
    fn add_months_wraps_years() {
        let ym = |year, month| YearMonth { year, month };
        assert_eq!(add_months(ym(2026, 1), -13), ym(2024, 12));
        assert_eq!(add_months(ym(2026, 12), 1), ym(2027, 1));
    }
}
