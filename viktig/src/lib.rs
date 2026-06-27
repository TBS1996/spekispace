use ordered_float::OrderedFloat;
use std::{collections::BTreeMap, sync::Arc};

use uuid::Uuid;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

type Float = OrderedFloat<f32>;

fn prioritize_vec(items: Vec<Item>, cmps: Vec<Cmp>) -> Vec<(Item, Float)> {
    prioritize(items, cmps).into_vec()
}

fn prioritize(items: Vec<Item>, cmps: Vec<Cmp>) -> PriRes {
    PriRes(BTreeMap::new())
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct Item {
    name: String,
    dependencies: Vec<Arc<Self>>,
    dependents: Vec<Arc<Self>>,
}

impl Item {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            dependencies: vec![],
            dependents: vec![],
        }
    }

    fn with_dependents(self, dependents: Vec<Self>) -> Self {
        Self {
            dependents: dependents.into_iter().map(Arc::new).collect(),
            ..self
        }
    }
    fn with_dependencies(self, dependencies: Vec<Self>) -> Self {
        Self {
            dependencies: dependencies.into_iter().map(Arc::new).collect(),
            ..self
        }
    }
}

/// State which is used to calculate the priority of items.
/// Invariants that must hold:
/// A card cannot have lower importance than the sum of all its recursive dependents.
/// Comparisons must be consistent, meaning no contradictions.
#[derive(Default)]
struct State {
    items: Vec<Item>,
    comparisons: Vec<CmpEntry>,
}

impl State {
    fn new(items: Vec<Item>, comparisons: Vec<CmpEntry>) -> Self {
        Self { items, comparisons }
    }

    /// Returns a list of comparisons that have been fixed.
    fn fixed_entries(&self) -> Vec<Cmp> {
        let mut seen: BTreeMap<(String, String), Cmp> = Default::default();

        for entry in &self.comparisons {
            match entry {
                CmpEntry::Cmp(cmp) => {
                    seen.insert(cmp.identity(), cmp.clone());
                }
                CmpEntry::Fix(cmp_fix) => {
                    for chunk in cmp_fix.0.clone().windows(2).into_iter() {
                        let higher = &chunk[0];
                        let lower = &chunk[1];
                        let cmp = Cmp::new(higher, lower);
                        let prev = seen.insert(cmp.identity(), cmp.clone());
                        assert!(prev.is_some(), "fixing a comparison that didn't exist");
                    }
                }
            }
        }
        seen.into_values().collect()
    }

    /// Removes items that are invalidated by the dependency system.
    /// Tuple first of valid comparisons, second of invalid comparisons.
    /// Ensures that a dependency cannot be ranked higher than an item it depends on.
    /// The reason it's not a hard error is because the user can change the dependency graph after already
    /// having made comparisons.
    fn remove_invalid_entries(&self) -> (Vec<Cmp>, Vec<Cmp>) {
        todo!()
    }

    /// Checks if comparisons are consistent, meaning no contradictions.
    /// Contradictions are for example if you write A > B, B > C, and A < C.
    /// Or if a dependent of an item is ranked as more important than the item itself.
    fn is_valid(&self) -> bool {
        todo!()
    }

    fn prioritize(&self) -> Result<PriRes, ErrState> {
        if !self.is_valid() {
            return Err(ErrState::InvalidComparisons { cycle: vec![] });
        } else {
            let fixed_entries = self.fixed_entries();
            let (valid_comparisons, invalid_comparisons) = self.remove_invalid_entries();
            Ok(prioritize(self.items.clone(), valid_comparisons))
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct PriRes(BTreeMap<String, (Item, Float)>);

impl PriRes {
    fn into_vec(self) -> Vec<(Item, Float)> {
        self.0
            .into_iter()
            .map(|(_, (item, score))| (item, score))
            .collect()
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ErrState {
    InvalidComparisons { cycle: Vec<String> },
}

enum CmpEntry {
    Cmp(Cmp),
    Fix(CmpFix),
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct Cmp {
    higher: String,
    lower: String,
}

impl Cmp {
    fn new(higher: &str, lower: &str) -> Self {
        Self {
            higher: higher.to_string(),
            lower: lower.to_string(),
        }
    }

    /// Returns a tuple of the two items, alphabetically
    fn identity(&self) -> (String, String) {
        if self.higher < self.lower {
            (self.higher.clone(), self.lower.clone())
        } else {
            (self.lower.clone(), self.higher.clone())
        }
    }
}

struct CmpFix(Vec<String>);

impl CmpFix {
    fn new(items: Vec<&str>) -> Self {
        Self(items.into_iter().map(|s| s.to_string()).collect())
    }
}

fn float(f: f32) -> Float {
    Float::from(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn test_eq(mut expected: Vec<(&str, Float)>, mut received: Vec<(Item, Float)>) {
        expected.sort_by_key(|k| k.1);
        received.sort_by_key(|k| k.1);
        assert_eq!(expected.len(), received.len());

        for (exp, rec) in expected.iter().zip(&received) {
            if exp.1 != rec.1 || exp.0 != rec.0.name.as_str() {
                dbg!(&expected, &received);
                dbg!(exp, rec);
                panic!();
            }
        }
    }

    #[test]
    fn test_cmp_identity() {
        let cmp = Cmp::new("foo", "bar");
        assert_eq!(cmp.identity(), ("foo".to_string(), "bar".to_string()));
        let cmp2 = Cmp::new("bar", "foo");
        assert_eq!(cmp2.identity(), ("foo".to_string(), "bar".to_string()));
    }

    #[test]
    fn default_value() {
        let item = Item::new("foo");
        let item2 = Item::new("bar");
        let res = prioritize_vec(vec![item, item2], vec![]);
        test_eq(vec![("foo", float(1.0)), ("bar", float(1.0))], res);
    }

    #[test]
    fn basic_deps() {
        let item = Item::new("foo").with_dependents(vec![Item::new("bar"), Item::new("baz")]);
        let state = State::new(vec![item], vec![]);
        let res = state.prioritize().unwrap();
        let fooval = res.0["foo"].1;
        let barval = res.0["bar"].1;
        let bazval = res.0["baz"].1;
        assert_eq!(fooval, barval + bazval + float(1.0));
    }

    #[test]
    fn recursive_deps_count_towards_parent() {
        let item = Item::new("foo").with_dependents(vec![
            Item::new("bar").with_dependents(vec![Item::new("baz")]),
        ]);
        let state = State::new(vec![item], vec![]);
        let res = state.prioritize().unwrap();

        assert_eq!(res.0["baz"].1, float(1.0));
        assert_eq!(res.0["bar"].1, float(2.0));
        assert_eq!(res.0["foo"].1, float(3.0));
    }

    #[test]
    fn shared_recursive_dependents_count_once() {
        let item = Item::new("foo").with_dependents(vec![
            Item::new("bar").with_dependents(vec![Item::new("shared")]),
            Item::new("baz").with_dependents(vec![Item::new("shared")]),
        ]);
        let state = State::new(vec![item], vec![]);
        let res = state.prioritize().unwrap();

        assert_eq!(res.0["shared"].1, float(1.0));
        assert_eq!(res.0["bar"].1, float(2.0));
        assert_eq!(res.0["baz"].1, float(2.0));
        assert_eq!(res.0["foo"].1, float(4.0));
    }

    #[test]
    fn basic_cmp() {
        let cmp = Cmp::new("foo", "bar");
        let foo = Item::new("foo");
        let bar = Item::new("bar");
        let state = State::new(vec![foo, bar], vec![CmpEntry::Cmp(cmp)]);
        let res = state.prioritize().unwrap();
        let fooval = res.0["foo"].1;
        let barval = res.0["bar"].1;
        assert!(fooval > barval);
    }

    #[test]
    fn simple_cmp_uses_default_spacing() {
        let foo = Item::new("foo");
        let bar = Item::new("bar");
        let state = State::new(vec![foo, bar], vec![CmpEntry::Cmp(Cmp::new("foo", "bar"))]);
        let res = state.prioritize().unwrap();

        assert_eq!(res.0["bar"].1, float(1.0));
        assert_eq!(res.0["foo"].1, float(2.0));
    }

    #[test]
    fn cmp_chain_uses_default_spacing() {
        let low = Item::new("low");
        let mid = Item::new("mid");
        let high = Item::new("high");
        let state = State::new(
            vec![low, mid, high],
            vec![
                CmpEntry::Cmp(Cmp::new("mid", "low")),
                CmpEntry::Cmp(Cmp::new("high", "mid")),
            ],
        );
        let res = state.prioritize().unwrap();

        assert_eq!(res.0["low"].1, float(1.0));
        assert_eq!(res.0["mid"].1, float(2.0));
        assert_eq!(res.0["high"].1, float(3.0));
    }

    /// Checks that we detect cycles, and provide a fix.
    #[test]
    fn invalid_cmp() {
        let cmp = Cmp::new("foo", "bar");
        let cmp2 = Cmp::new("bar", "baz");
        let cmp3 = Cmp::new("baz", "foo");
        let foo = Item::new("foo");
        let bar = Item::new("bar");
        let baz = Item::new("baz");
        let state = State::new(
            vec![foo, bar, baz],
            vec![CmpEntry::Cmp(cmp), CmpEntry::Cmp(cmp2), CmpEntry::Cmp(cmp3)],
        );
        match state.prioritize().unwrap_err() {
            ErrState::InvalidComparisons { cycle } => {
                assert_eq!(cycle, vec!["foo", "bar", "baz"]);
            }
        }
    }

    /// Checks that cycles do not have to be limited to three items.
    #[test]
    fn invalid_cmp_longer_cycle() {
        let foo = Item::new("foo");
        let bar = Item::new("bar");
        let baz = Item::new("baz");
        let quux = Item::new("quux");
        let state = State::new(
            vec![foo, bar, baz, quux],
            vec![
                CmpEntry::Cmp(Cmp::new("foo", "bar")),
                CmpEntry::Cmp(Cmp::new("bar", "baz")),
                CmpEntry::Cmp(Cmp::new("baz", "quux")),
                CmpEntry::Cmp(Cmp::new("quux", "foo")),
            ],
        );
        match state.prioritize().unwrap_err() {
            ErrState::InvalidComparisons { cycle } => {
                assert_eq!(cycle, vec!["foo", "bar", "baz", "quux"]);
            }
        }
    }

    /// Checks that if we have a cycle and a fix the state is valid.
    #[test]
    fn fix_override() {
        let cmp = Cmp::new("foo", "bar");
        let cmp2 = Cmp::new("bar", "baz");
        let cmp3 = Cmp::new("baz", "foo");
        let foo = Item::new("foo");
        let bar = Item::new("bar");
        let baz = Item::new("baz");
        let state = State::new(
            vec![foo, bar, baz],
            vec![
                CmpEntry::Cmp(cmp),
                CmpEntry::Cmp(cmp2),
                CmpEntry::Cmp(cmp3),
                CmpEntry::Fix(CmpFix::new(vec!["foo", "bar", "baz"])),
            ],
        );
        let res = state.fixed_entries();
        let identities: BTreeSet<(String, String)> = res.iter().map(|cmp| cmp.identity()).collect();

        // The fix removes the baz > foo comparison
        assert_eq!(
            identities,
            BTreeSet::from([
                ("foo".to_string(), "bar".to_string()),
                ("bar".to_string(), "baz".to_string()),
            ])
        );
        assert!(res.contains(&Cmp::new("foo", "bar")));
        assert!(res.contains(&Cmp::new("bar", "baz")));
        assert!(!res.contains(&Cmp::new("baz", "foo")));
    }

    /// Checks that a longer cycle can be replaced by one explicit ordering.
    #[test]
    fn fix_override_longer_cycle() {
        let foo = Item::new("foo");
        let bar = Item::new("bar");
        let baz = Item::new("baz");
        let quux = Item::new("quux");
        let state = State::new(
            vec![foo, bar, baz, quux],
            vec![
                CmpEntry::Cmp(Cmp::new("foo", "bar")),
                CmpEntry::Cmp(Cmp::new("bar", "baz")),
                CmpEntry::Cmp(Cmp::new("baz", "quux")),
                CmpEntry::Cmp(Cmp::new("quux", "foo")),
                CmpEntry::Fix(CmpFix::new(vec!["foo", "bar", "baz", "quux"])),
            ],
        );
        let res = state.fixed_entries();

        assert!(res.contains(&Cmp::new("foo", "bar")));
        assert!(res.contains(&Cmp::new("bar", "baz")));
        assert!(res.contains(&Cmp::new("baz", "quux")));
        assert!(!res.contains(&Cmp::new("quux", "foo")));
    }

    #[test]
    fn test_remove_invalid() {
        let foo = Item::new("foo").with_dependents(vec![Item::new("bar")]);
        let baz = Item::new("baz");

        let state = State::new(
            vec![foo, baz],
            vec![
                CmpEntry::Cmp(Cmp::new("foo", "baz")),
                // bar > foo is invalid because foo depends on bar
                CmpEntry::Cmp(Cmp::new("bar", "foo")),
            ],
        );
        let (valid_comparisons, invalid_comparisons) = state.remove_invalid_entries();
        assert_eq!(valid_comparisons, vec![Cmp::new("foo", "baz")]);
        assert_eq!(invalid_comparisons, vec![Cmp::new("bar", "foo")]);
    }
}
