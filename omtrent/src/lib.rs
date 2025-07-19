use std::{
    cmp::Ordering,
    fmt::{Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

pub enum Precision {
    Millenium,
    Century,
    Decade,
    Year,
    Month,
    Day,
    Hour,
    Minute,
}

#[derive(Default, PartialOrd, Eq, Hash, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct TimeStamp {
    millenium: u32,
    century: Option<u32>,
    decade: Option<u32>,
    year: Option<u32>,
    month: Option<u32>,
    day: Option<u32>,
    hour: Option<u32>,
    minute: Option<u32>,
    after_christ: bool,
}

impl Ord for TimeStamp {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.after_christ, other.after_christ) {
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            _ => {}
        }

        let ord = self
            .millenium
            .cmp(&other.millenium)
            .then_with(|| option_cmp(self.century, other.century))
            .then_with(|| option_cmp(self.decade, other.decade))
            .then_with(|| option_cmp(self.year, other.year))
            .then_with(|| option_cmp(self.month, other.month))
            .then_with(|| option_cmp(self.day, other.day))
            .then_with(|| option_cmp(self.hour, other.hour))
            .then_with(|| option_cmp(self.minute, other.minute));

        if !self.after_christ {
            ord.reverse()
        } else {
            ord
        }
    }
}

/// Compares two options, treating `None` as a wildcard that matches any value.
fn option_cmp(lhs: Option<u32>, rhs: Option<u32>) -> Ordering {
    match (lhs, rhs) {
        (Some(l), Some(r)) => l.cmp(&r),
        (None, _) | (_, None) => Ordering::Equal,
    }
}

impl Display for TimeStamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

impl TimeStamp {
    fn display(&self) -> String {
        let era = if self.after_christ { "AD" } else { "BC" };

        let cty = match self.century {
            Some(c) => self.millenium * 10 + c,
            None => {
                let num = self.millenium + 1;
                return format!("{}{} millenium {}", num, Self::suffix(num), era);
            }
        };

        let decade = match self.decade {
            Some(d) => cty * 10 + d,
            None => {
                if cty > 10 && self.after_christ {
                    return format!("{cty}00s");
                } else {
                    return format!("{}{} century {}", cty + 1, Self::suffix(cty + 1), era);
                }
            }
        };

        // if after year 1000, AD is implied
        let era = if decade > 100 && self.after_christ {
            ""
        } else {
            era
        };

        let year = match self.year {
            Some(y) => decade * 10 + y,
            None => {
                if decade % 10 == 0 {
                    return format!("First decade of the {decade}0s {era}");
                } else {
                    return format!("{decade}0s {era}");
                }
            }
        };

        let month = match self.month {
            Some(m) => m,
            None => {
                return format!("{year} {era}");
            }
        };

        let day = match self.day {
            Some(d) => d,
            None => {
                return format!("{} {} {}", Self::month_str(month), year, era);
            }
        };

        let hour = match self.hour {
            Some(h) => h,
            None => {
                return format!("{} {} {} {}", day, Self::month_str(month), year, era);
            }
        };

        match self.minute {
            Some(minute) => {
                format!(
                    "{:02}:{:02} {} {} {} {}",
                    hour,
                    minute,
                    day,
                    Self::month_str(month),
                    year,
                    era
                )
            }
            None => {
                format!(
                    "{} o' clock, {} {} {} {}",
                    hour,
                    day,
                    Self::month_str(month),
                    year,
                    era
                )
            }
        }
    }

    fn month_str(m: u32) -> &'static str {
        match m {
            1 => "jan",
            2 => "feb",
            3 => "mar",
            4 => "apr",
            5 => "may",
            6 => "jun",
            7 => "jul",
            8 => "aug",
            9 => "sep",
            10 => "okt",
            11 => "nov",
            12 => "dec",
            _ => "INVALID MONTH",
        }
    }

    fn suffix(num: u32) -> &'static str {
        match num {
            0 => "th",
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    }

    fn parse_two_digits(iter: &mut impl Iterator<Item = char>) -> Option<u32> {
        let (d1, d2) = (iter.next()?, iter.next()?);
        Some(d1.to_string().parse::<u32>().ok()? * 10 + d2.to_string().parse::<u32>().ok()?)
    }

    pub fn serialize(&self) -> String {
        let mut s = String::new();
        if !self.after_christ {
            s.push('-');
        }

        s.push_str(&self.millenium.to_string());
        s.push_str(
            &self
                .century
                .map(|c| c.to_string())
                .unwrap_or("*".to_string()),
        );
        s.push_str(
            &self
                .decade
                .map(|c| c.to_string())
                .unwrap_or("*".to_string()),
        );
        s.push_str(&self.year.map(|c| c.to_string()).unwrap_or("*".to_string()));

        if let Some(month) = self.month {
            s.push_str(&format!("-{month:02}"));
        } else {
            return s;
        };

        if let Some(day) = self.day {
            s.push_str(&format!("-{day:02}"));
        } else {
            return s;
        };

        if let Some(hour) = self.hour {
            s.push_str(&format!(" {hour:02}"));
        } else {
            return s;
        };

        if let Some(minute) = self.minute {
            s.push_str(&format!(":{minute:02}"));
        }

        s
    }

    pub fn into_precision(self, pres: Precision) -> Self {
        match pres {
            Precision::Millenium => Self {
                millenium: self.millenium,
                ..Default::default()
            },
            Precision::Century => Self {
                millenium: self.millenium,
                century: self.century,
                ..Default::default()
            },
            Precision::Decade => Self {
                millenium: self.millenium,
                century: self.century,
                decade: self.decade,
                ..Default::default()
            },
            Precision::Year => Self {
                millenium: self.millenium,
                century: self.century,
                decade: self.decade,
                year: self.year,
                ..Default::default()
            },
            Precision::Month => Self {
                millenium: self.millenium,
                century: self.century,
                decade: self.decade,
                year: self.year,
                month: self.month,
                ..Default::default()
            },
            Precision::Day => Self {
                millenium: self.millenium,
                century: self.century,
                decade: self.decade,
                year: self.year,
                month: self.month,
                day: self.day,
                ..Default::default()
            },
            Precision::Hour => Self {
                millenium: self.millenium,
                century: self.century,
                decade: self.decade,
                year: self.year,
                month: self.month,
                day: self.day,
                hour: self.hour,
                ..Default::default()
            },
            Precision::Minute => self,
        }
    }

    pub fn clock_emoji(&self) -> &'static str {
        match self.hour {
            Some(hr) => {
                let hr = hr % 12;
                let minute = self.minute.unwrap_or_default();
                let half = minute >= 30;

                match (hr, half) {
                    (0, false) => "🕛",
                    (0, true) => "🕧",
                    (1, false) => "🕐",
                    (1, true) => "🕜",
                    (2, false) => "🕑",
                    (2, true) => "🕝",
                    (3, false) => "🕒",
                    (3, true) => "🕞",
                    (4, false) => "🕓",
                    (4, true) => "🕟",
                    (5, false) => "🕔",
                    (5, true) => "🕠",
                    (6, false) => "🕕",
                    (6, true) => "🕡",
                    (7, false) => "🕖",
                    (7, true) => "🕢",
                    (8, false) => "🕗",
                    (8, true) => "🕣",
                    (9, false) => "🕘",
                    (9, true) => "🕤",
                    (10, false) => "🕙",
                    (10, true) => "🕥",
                    (11, false) => "🕚",
                    (11, true) => "🕦",
                    _ => "🕓",
                }
            }
            None => "🕓",
        }
    }
}

impl FromStr for TimeStamp {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut selv = Self::default();
        let mut s: Vec<char> = s.chars().collect();
        let first = s.first().ok_or(())?;
        if first != &'+' && first != &'-' {
            s.insert(0, '+');
        }

        let mut iter = s.into_iter();

        match iter.next().ok_or(())? {
            '+' => selv.after_christ = true,
            '-' => selv.after_christ = false,
            _ => panic!(),
        }

        selv.millenium = iter.next().ok_or(())?.to_string().parse().ok().ok_or(())?;

        selv.century = match iter.next().ok_or(())? {
            '*' => None,
            num => Some(num.to_string().parse().ok().ok_or(())?),
        };

        selv.decade = match iter.next().ok_or(())? {
            '*' => None,
            num => Some(num.to_string().parse().ok().ok_or(())?),
        };

        selv.year = match iter.next().ok_or(())? {
            '*' => None,
            num => Some(num.to_string().parse().ok().ok_or(())?),
        };

        match iter.next() {
            Some('-') => {}
            Some(' ') => {}
            Some(_) => None.ok_or(())?,
            None => return Ok(selv),
        }

        selv.month = Some(Self::parse_two_digits(&mut iter).ok_or(())?);

        match iter.next() {
            Some('-') => {}
            Some(' ') => {}
            Some(_) => None.ok_or(())?,
            None => return Ok(selv),
        }

        selv.day = Some(Self::parse_two_digits(&mut iter).ok_or(())?);

        match iter.next() {
            Some('-') => {}
            Some(' ') => {}
            Some(_) => None.ok_or(())?,
            None => return Ok(selv),
        }

        selv.hour = Some(Self::parse_two_digits(&mut iter).ok_or(())?);

        match iter.next() {
            Some(':') => {}
            Some(_) => None.ok_or(())?,
            None => return Ok(selv),
        }

        selv.minute = Some(Self::parse_two_digits(&mut iter).ok_or(())?);

        Ok(selv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ord() {
        let foo = TimeStamp::from_str("1950").unwrap();
        let bar = TimeStamp::from_str("19**").unwrap();
        assert!(foo.cmp(&bar).is_eq());

        let foo = TimeStamp::from_str("1850").unwrap();
        let bar = TimeStamp::from_str("19**").unwrap();
        assert!(foo.cmp(&bar).is_le());

        let foo = TimeStamp::from_str("2050").unwrap();
        let bar = TimeStamp::from_str("19**").unwrap();
        assert!(foo.cmp(&bar).is_ge());

        let foo = TimeStamp::from_str("-1950").unwrap();
        let bar = TimeStamp::from_str("-19**").unwrap();
        assert!(foo.cmp(&bar).is_eq());

        let foo = TimeStamp::from_str("-1850").unwrap();
        let bar = TimeStamp::from_str("-19**").unwrap();
        assert!(foo.cmp(&bar).is_ge());
    }
}
