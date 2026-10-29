//! A locale's plural rules and number and date formatters, made the first
//! time a message needs them and kept (`I18n` caches one set per locale).
//!
//! With the `i18n` feature they are ICU4X's, from its compiled data for
//! every locale; without it, numbers show as Rust writes them and plurals
//! follow English (`one` for 1, `other` otherwise). Dates need
//! `i18n-dates`; without it they show as ISO 8601 (`2026-10-28`).
use super::message::{DateLength, OTHER};
#[cfg(feature = "i18n")]
use std::cell::OnceCell;

/// What formats a message's numbers, dates and plurals in one locale.
pub(crate) struct Formatters {
    #[cfg_attr(not(feature = "i18n"), allow(dead_code))]
    locale: Box<str>,
    #[cfg(feature = "i18n")]
    cardinal: OnceCell<Option<icu_plurals::PluralRules>>,
    #[cfg(feature = "i18n")]
    ordinal: OnceCell<Option<icu_plurals::PluralRules>>,
    #[cfg(feature = "i18n")]
    decimal: OnceCell<Option<icu_decimal::DecimalFormatter>>,
    /// Whether this locale writes digits as ASCII's: then a whole number
    /// below 1,000 (ungrouped) is written as Rust writes it.
    #[cfg(feature = "i18n")]
    ascii_digits: OnceCell<bool>,
    /// The categories of whole numbers below `SMALL`, cardinal then
    /// ordinal, as they are first asked (`u8::MAX`: not yet).
    #[cfg(feature = "i18n")]
    small: std::cell::RefCell<Vec<u8>>,
    #[cfg(feature = "i18n-dates")]
    dates: std::cell::RefCell<Vec<(DateLength, DateFormatter)>>,
}

#[cfg(feature = "i18n-dates")]
type DateFormatter = icu_datetime::FixedCalendarDateTimeFormatter<
    icu_calendar::Gregorian,
    icu_datetime::fieldsets::YMD,
>;

impl Formatters {
    pub(crate) fn new(locale: &str) -> Self {
        Formatters {
            locale: locale.into(),
            #[cfg(feature = "i18n")]
            cardinal: OnceCell::new(),
            #[cfg(feature = "i18n")]
            ordinal: OnceCell::new(),
            #[cfg(feature = "i18n")]
            decimal: OnceCell::new(),
            #[cfg(feature = "i18n")]
            ascii_digits: OnceCell::new(),
            #[cfg(feature = "i18n")]
            small: Default::default(),
            #[cfg(feature = "i18n-dates")]
            dates: Default::default(),
        }
    }

    #[cfg(feature = "i18n")]
    fn icu_locale(&self) -> icu_locale_core::Locale {
        icu_locale_core::Locale::try_from_str(&self.locale)
            .unwrap_or(icu_locale_core::locale!("und"))
    }

    /// `n`'s plural category here: its number in `message::CATEGORIES`
    /// (`zero`, `one`, `two`, `few`, `many`, `other`).
    pub(crate) fn plural(&self, n: f64, ordinal: bool) -> usize {
        #[cfg(feature = "i18n")]
        {
            let cell = if ordinal {
                &self.ordinal
            } else {
                &self.cardinal
            };
            let rules = cell.get_or_init(|| {
                let prefs = (&self.icu_locale()).into();
                if ordinal {
                    icu_plurals::PluralRules::try_new_ordinal(prefs).ok()
                } else {
                    icu_plurals::PluralRules::try_new_cardinal(prefs).ok()
                }
            });
            let Some(rules) = rules else {
                return OTHER;
            };
            // A count's category, as a list's rows ask it: once each.
            let small = (n.fract() == 0. && (0. ..SMALL as f64).contains(&n))
                .then(|| n as usize + if ordinal { SMALL } else { 0 });
            if let Some(at) = small {
                let mut cache = self.small.borrow_mut();
                if cache.is_empty() {
                    cache.resize(2 * SMALL, u8::MAX);
                }
                if cache[at] == u8::MAX {
                    cache[at] = category_number(rules.category_for(n as u64)) as u8;
                }
                return usize::from(cache[at]);
            }
            match to_decimal(n) {
                Some(decimal) => category_number(rules.category_for(&decimal)),
                None => OTHER,
            }
        }
        #[cfg(not(feature = "i18n"))]
        {
            // English's, as `CATEGORIES` numbers them.
            const ONE: usize = 1;
            const TWO: usize = 2;
            const FEW: usize = 3;
            if ordinal {
                return match (n as i64 % 10, n as i64 % 100) {
                    _ if n.fract() != 0. => OTHER,
                    (1, r) if r != 11 => ONE,
                    (2, r) if r != 12 => TWO,
                    (3, r) if r != 13 => FEW,
                    _ => OTHER,
                };
            }
            if n == 1. { ONE } else { OTHER }
        }
    }

    /// Append `n` as this locale writes it.
    pub(crate) fn number(&self, out: &mut String, n: f64) {
        #[cfg(feature = "i18n")]
        {
            let formatter = self.decimal.get_or_init(|| {
                icu_decimal::DecimalFormatter::try_new(
                    (&self.icu_locale()).into(),
                    Default::default(),
                )
                .ok()
            });
            let Some(formatter) = formatter else {
                return plain_number(out, n);
            };
            let ascii = *self.ascii_digits.get_or_init(|| {
                (0..10_i64).all(|d| {
                    let written = formatter
                        .format(&fixed_decimal::Decimal::from(d))
                        .to_string();
                    written.len() == 1 && written.as_bytes()[0] == b'0' + d as u8
                })
            });
            if ascii && n.fract() == 0. && (0. ..1000.).contains(&n) {
                return plain_number(out, n);
            }
            if let Some(decimal) = to_decimal(n) {
                use std::fmt::Write;
                let _ = write!(out, "{}", formatter.format(&decimal));
                return;
            }
        }
        plain_number(out, n);
    }

    /// Append the date `ms` milliseconds after 1970 began (local time, as
    /// `local_time` knows it) at `length`.
    pub(crate) fn date(&self, out: &mut String, ms: f64, length: DateLength) {
        let (year, month, day) = civil_date(ms);
        #[cfg(feature = "i18n-dates")]
        {
            use icu_datetime::fieldsets::YMD;
            let mut dates = self.dates.borrow_mut();
            let at = match dates.iter().position(|(l, _)| *l == length) {
                Some(at) => Some(at),
                None => {
                    let fields = match length {
                        DateLength::Short => YMD::short(),
                        DateLength::Medium => YMD::medium(),
                        DateLength::Long | DateLength::Full => YMD::long(),
                    };
                    DateFormatter::try_new((&self.icu_locale()).into(), fields)
                        .ok()
                        .map(|formatter| {
                            dates.push((length, formatter));
                            dates.len() - 1
                        })
                }
            };
            if let (Some(at), Ok(date)) =
                (at, icu_calendar::Date::try_new_gregorian(year, month, day))
            {
                use std::fmt::Write;
                let _ = write!(out, "{}", dates[at].1.format(&date));
                return;
            }
        }
        let _ = length;
        use std::fmt::Write;
        let _ = write!(out, "{year:04}-{month:02}-{day:02}");
    }
}

/// `n` as Rust writes it, a whole number without its `.0`.
fn plain_number(out: &mut String, n: f64) {
    use std::fmt::Write;
    if n.fract() == 0. && n.abs() < 1e15 {
        let _ = write!(out, "{}", n as i64);
    } else {
        let _ = write!(out, "{n}");
    }
}

#[cfg(feature = "i18n")]
fn to_decimal(n: f64) -> Option<fixed_decimal::Decimal> {
    if n.fract() == 0. && n.abs() < 1e15 {
        return Some(fixed_decimal::Decimal::from(n as i64));
    }
    fixed_decimal::Decimal::try_from_f64(n, fixed_decimal::FloatPrecision::RoundTrip).ok()
}

/// Whole numbers below this have their plural categories kept.
#[cfg(feature = "i18n")]
const SMALL: usize = 1000;

/// `category`'s number in `message::CATEGORIES`.
#[cfg(feature = "i18n")]
fn category_number(category: icu_plurals::PluralCategory) -> usize {
    use icu_plurals::PluralCategory::*;
    match category {
        Zero => 0,
        One => 1,
        Two => 2,
        Few => 3,
        Many => 4,
        Other => OTHER,
    }
}

#[cfg(feature = "i18n")]
fn category_name(category: icu_plurals::PluralCategory) -> &'static str {
    use icu_plurals::PluralCategory::*;
    match category {
        Zero => "zero",
        One => "one",
        Two => "two",
        Few => "few",
        Many => "many",
        Other => "other",
    }
}

/// The plural categories `locale` uses (cardinal, or `ordinal`): what its
/// messages' plurals need cases for. Without the `i18n` feature, English's.
pub fn plural_categories(locale: &str, ordinal: bool) -> Vec<&'static str> {
    #[cfg(feature = "i18n")]
    {
        let Ok(locale) = icu_locale_core::Locale::try_from_str(locale) else {
            return vec!["other"];
        };
        let prefs = (&locale).into();
        let rules = if ordinal {
            icu_plurals::PluralRules::try_new_ordinal(prefs)
        } else {
            icu_plurals::PluralRules::try_new_cardinal(prefs)
        };
        match rules {
            Ok(rules) => rules.categories().map(category_name).collect(),
            Err(_) => vec!["other"],
        }
    }
    #[cfg(not(feature = "i18n"))]
    {
        let _ = locale;
        if ordinal {
            vec!["one", "two", "few", "other"]
        } else {
            vec!["one", "other"]
        }
    }
}

/// The local calendar date `ms` milliseconds after 1970 began.
fn civil_date(ms: f64) -> (i32, u8, u8) {
    let seconds = (ms / 1000.).floor() as i64 + i64::from(crate::local_time::offset());
    // Howard Hinnant's days-to-civil.
    let z = seconds.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    (year, month, day)
}

#[cfg(test)]
mod tests {
    #[test]
    fn civil_dates_round_known_days() {
        assert_eq!(super::civil_date(0.), (1970, 1, 1));
        // 2026-10-28T12:00:00Z.
        assert_eq!(super::civil_date(1_793_188_800_000.), (2026, 10, 28));
        assert_eq!(super::civil_date(-86_400_000.), (1969, 12, 31));
        // A leap day.
        assert_eq!(super::civil_date(951_782_400_000.), (2000, 2, 29));
    }
}
