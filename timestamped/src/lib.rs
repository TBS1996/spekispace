use std::{
    cmp::Ordering,
    fmt::{Display, Formatter},
};

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

#[derive(Default, PartialOrd, Eq, Hash, PartialEq, Debug, Clone)]
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
                if cty > 10 {
                    return format!("{}00s", cty);
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
                    return format!("First decade of the {}0s {}", decade, era);
                } else {
                    return format!("{}0s {}", decade, era);
                }
            }
        };

        let month = match self.month {
            Some(m) => m,
            None => {
                return format!("{} {}", year, era);
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
            s.push_str(&format!("-{:02}", month));
        } else {
            return s;
        };

        if let Some(day) = self.day {
            s.push_str(&format!("-{:02}", day));
        } else {
            return s;
        };

        if let Some(hour) = self.hour {
            s.push_str(&format!(" {:02}", hour));
        } else {
            return s;
        };

        if let Some(minute) = self.minute {
            s.push_str(&format!(":{:02}", minute));
        }

        s
    }

    pub fn from_string(s: String) -> Option<Self> {
        let mut selv = Self::default();
        let mut s: Vec<char> = s.chars().collect();
        let first = s.first()?;
        if first != &'+' && first != &'-' {
            s.insert(0, '+');
        }

        let mut iter = s.into_iter();

        match iter.next()? {
            '+' => selv.after_christ = true,
            '-' => selv.after_christ = false,
            _ => panic!(),
        }

        selv.millenium = iter.next()?.to_string().parse().ok()?;

        selv.century = match iter.next()? {
            '*' => None,
            num => Some(num.to_string().parse().ok()?),
        };

        selv.decade = match iter.next()? {
            '*' => None,
            num => Some(num.to_string().parse().ok()?),
        };

        selv.year = match iter.next()? {
            '*' => None,
            num => Some(num.to_string().parse().ok()?),
        };

        match iter.next() {
            Some('-') => {}
            Some(' ') => {}
            Some(_) => None?,
            None => return Some(selv),
        }

        selv.month = Some(Self::parse_two_digits(&mut iter)?);

        match iter.next() {
            Some('-') => {}
            Some(' ') => {}
            Some(_) => None?,
            None => return Some(selv),
        }

        selv.day = Some(Self::parse_two_digits(&mut iter)?);

        match iter.next() {
            Some('-') => {}
            Some(' ') => {}
            Some(_) => None?,
            None => return Some(selv),
        }

        selv.hour = Some(Self::parse_two_digits(&mut iter)?);

        match iter.next() {
            Some(':') => {}
            Some(_) => None?,
            None => return Some(selv),
        }

        selv.minute = Some(Self::parse_two_digits(&mut iter)?);

        Some(selv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ord() {
        let foo = TimeStamp::from_string("1950".to_string()).unwrap();
        let bar = TimeStamp::from_string("19**".to_string()).unwrap();
        assert!(foo.cmp(&bar).is_eq());

        let foo = TimeStamp::from_string("1850".to_string()).unwrap();
        let bar = TimeStamp::from_string("19**".to_string()).unwrap();
        assert!(foo.cmp(&bar).is_le());

        let foo = TimeStamp::from_string("2050".to_string()).unwrap();
        let bar = TimeStamp::from_string("19**".to_string()).unwrap();
        assert!(foo.cmp(&bar).is_ge());

        let foo = TimeStamp::from_string("-1950".to_string()).unwrap();
        let bar = TimeStamp::from_string("-19**".to_string()).unwrap();
        assert!(foo.cmp(&bar).is_eq());

        let foo = TimeStamp::from_string("-1850".to_string()).unwrap();
        let bar = TimeStamp::from_string("-19**".to_string()).unwrap();
        assert!(foo.cmp(&bar).is_ge());
    }
}
