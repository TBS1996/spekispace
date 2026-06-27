use ordered_float::OrderedFloat;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

type Float = OrderedFloat<f32>;

fn prioritize_vec(items: Vec<Item>, cmps: Vec<Cmp>) -> Vec<(Item, Float)> {
    prioritize(items, cmps).into_vec()
}

fn prioritize(items: Vec<Item>, cmps: Vec<Cmp>) -> PriRes {
    let graph = ItemGraph::new(items);
    let mut scores = graph.dependency_scores();

    loop {
        let mut changed = false;

        for cmp in &cmps {
            let lower = scores[&cmp.lower];
            let min_higher = lower + float(1.0);
            let higher = scores.get_mut(&cmp.higher).unwrap();
            if *higher < min_higher {
                *higher = min_higher;
                changed = true;
            }
        }

        for (higher_name, lower_names) in &graph.dependents {
            let higher = scores[higher_name];
            let min_higher = lower_names
                .iter()
                .map(|lower_name| scores[lower_name] + float(1.0))
                .max()
                .unwrap_or(float(1.0));

            if higher < min_higher {
                scores.insert(higher_name.clone(), min_higher);
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }

    PriRes(
        graph
            .items
            .into_iter()
            .map(|(name, item)| {
                let score = scores[&name];
                (name, (item, score))
            })
            .collect(),
    )
}

struct ItemGraph {
    items: BTreeMap<String, Item>,
    /// Edges from an item to the cards which depend on it.
    dependents: BTreeMap<String, BTreeSet<String>>,
}

impl ItemGraph {
    fn new(items: Vec<Item>) -> Self {
        let mut graph = Self {
            items: BTreeMap::new(),
            dependents: BTreeMap::new(),
        };

        for item in items {
            graph.add_item(item);
        }

        graph
    }

    fn add_item(&mut self, item: Item) {
        let name = item.name.clone();
        let dependencies = item.dependencies.clone();
        let dependents = item.dependents.clone();

        self.items
            .entry(name.clone())
            .or_insert_with(|| item.clone());
        self.dependents.entry(name.clone()).or_default();

        for dependent in dependents {
            self.dependents
                .entry(name.clone())
                .or_default()
                .insert(dependent.name.clone());
            self.add_item((*dependent).clone());
        }

        for dependency in dependencies {
            self.dependents
                .entry(dependency.name.clone())
                .or_default()
                .insert(name.clone());
            self.add_item((*dependency).clone());
        }
    }

    fn dependency_scores(&self) -> BTreeMap<String, Float> {
        self.items
            .keys()
            .map(|name| {
                let score = self.recursive_dependents(name).len() as f32 + 1.0;
                (name.clone(), float(score))
            })
            .collect()
    }

    fn recursive_dependents(&self, name: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        self.collect_dependents(name, &mut seen);
        seen
    }

    fn collect_dependents(&self, name: &str, seen: &mut BTreeSet<String>) {
        if let Some(dependents) = self.dependents.get(name) {
            for dependent in dependents {
                if seen.insert(dependent.clone()) {
                    self.collect_dependents(dependent, seen);
                }
            }
        }
    }

    fn is_dependency_order(&self, higher: &str, lower: &str) -> bool {
        self.recursive_dependents(higher).contains(lower)
    }
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
        let mut entries: Vec<Option<Cmp>> = vec![];
        let mut seen: BTreeMap<(String, String), usize> = Default::default();

        for entry in &self.comparisons {
            match entry {
                CmpEntry::Cmp(cmp) => {
                    insert_cmp(&mut entries, &mut seen, cmp.clone());
                }
                CmpEntry::Fix(cmp_fix) => {
                    let fixed_items: BTreeSet<String> = cmp_fix.0.iter().cloned().collect();
                    let kept_identities: BTreeSet<(String, String)> = cmp_fix
                        .0
                        .windows(2)
                        .map(|chunk| Cmp::new(&chunk[0], &chunk[1]).identity())
                        .collect();
                    let seen_identities: Vec<(String, String)> = seen.keys().cloned().collect();

                    for identity in seen_identities {
                        if fixed_items.contains(&identity.0)
                            && fixed_items.contains(&identity.1)
                            && !kept_identities.contains(&identity)
                        {
                            if let Some(index) = seen.remove(&identity) {
                                entries[index] = None;
                            }
                        }
                    }

                    for chunk in cmp_fix.0.windows(2) {
                        let higher = &chunk[0];
                        let lower = &chunk[1];
                        let cmp = Cmp::new(higher, lower);
                        assert!(
                            seen.contains_key(&cmp.identity()),
                            "fixing a comparison that didn't exist"
                        );
                        insert_cmp(&mut entries, &mut seen, cmp);
                    }
                }
            }
        }
        entries.into_iter().flatten().collect()
    }

    /// Removes items that are invalidated by the dependency system.
    /// Tuple first of valid comparisons, second of invalid comparisons.
    /// Ensures that a dependency cannot be ranked higher than an item it depends on.
    /// The reason it's not a hard error is because the user can change the dependency graph after already
    /// having made comparisons.
    fn remove_invalid_entries(&self) -> (Vec<Cmp>, Vec<Cmp>) {
        let graph = ItemGraph::new(self.items.clone());
        let mut valid = vec![];
        let mut invalid = vec![];

        for cmp in self.fixed_entries() {
            if graph.is_dependency_order(&cmp.lower, &cmp.higher) {
                invalid.push(cmp);
            } else {
                valid.push(cmp);
            }
        }

        (valid, invalid)
    }

    /// Checks if comparisons are consistent, meaning no contradictions.
    /// Contradictions are for example if you write A > B, B > C, and A < C.
    /// Or if a dependent of an item is ranked as more important than the item itself.
    fn is_valid(&self) -> bool {
        self.invalid_cycle().is_none()
    }

    fn prioritize(&self) -> Result<PriRes, ErrState> {
        if let Some(cycle) = self.invalid_cycle() {
            return Err(ErrState::InvalidComparisons { cycle });
        } else {
            let (valid_comparisons, _invalid_comparisons) = self.remove_invalid_entries();
            Ok(prioritize(self.items.clone(), valid_comparisons))
        }
    }

    fn invalid_cycle(&self) -> Option<Vec<String>> {
        let (valid_comparisons, _) = self.remove_invalid_entries();
        find_cycle(&valid_comparisons)
    }
}

fn insert_cmp(
    entries: &mut Vec<Option<Cmp>>,
    seen: &mut BTreeMap<(String, String), usize>,
    cmp: Cmp,
) {
    let identity = cmp.identity();
    if let Some(index) = seen.get(&identity) {
        entries[*index] = Some(cmp);
    } else {
        seen.insert(identity, entries.len());
        entries.push(Some(cmp));
    }
}

fn find_cycle(comparisons: &[Cmp]) -> Option<Vec<String>> {
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for cmp in comparisons {
        graph
            .entry(cmp.higher.clone())
            .or_default()
            .push(cmp.lower.clone());
        graph.entry(cmp.lower.clone()).or_default();
    }

    for cmp in comparisons {
        let mut stack = vec![];
        let mut on_stack = BTreeSet::new();
        if let Some(cycle) = find_cycle_from(&cmp.higher, &graph, &mut stack, &mut on_stack) {
            return Some(cycle);
        }
    }

    None
}

fn find_cycle_from(
    node: &str,
    graph: &BTreeMap<String, Vec<String>>,
    stack: &mut Vec<String>,
    on_stack: &mut BTreeSet<String>,
) -> Option<Vec<String>> {
    if let Some(start) = stack.iter().position(|name| name == node) {
        return Some(stack[start..].to_vec());
    }

    if !on_stack.insert(node.to_string()) {
        return None;
    }

    stack.push(node.to_string());

    if let Some(nexts) = graph.get(node) {
        for next in nexts {
            if let Some(cycle) = find_cycle_from(next, graph, stack, on_stack) {
                return Some(cycle);
            }
        }
    }

    stack.pop();
    None
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
        expected.sort_by_key(|(name, score)| (*score, *name));
        received.sort_by_key(|(item, score)| (*score, item.name.clone()));
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
        assert_eq!(cmp.identity(), ("bar".to_string(), "foo".to_string()));
        let cmp2 = Cmp::new("bar", "foo");
        assert_eq!(cmp2.identity(), ("bar".to_string(), "foo".to_string()));
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
                ("bar".to_string(), "baz".to_string()),
                ("bar".to_string(), "foo".to_string()),
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

    #[test]
    fn realistic_flashcard_weights_debug() {
        let spanish = Item::new("spanish basics").with_dependents(vec![
            Item::new("spanish grammar").with_dependents(vec![
                Item::new("spanish adjective agreement"),
                Item::new("spanish ser vs estar"),
            ]),
            Item::new("spanish vocab").with_dependents(vec![
                Item::new("spanish vocab: gato -> cat"),
                Item::new("spanish vocab: biblioteca -> library"),
                Item::new("spanish vocab: trabajar -> to work"),
            ]),
            Item::new("spanish greetings").with_dependents(vec![
                Item::new("spanish phrase: buenos dias"),
                Item::new("spanish phrase: hasta luego"),
            ]),
        ]);

        let japanese = Item::new("japanese basics").with_dependents(vec![
            Item::new("japanese kana").with_dependents(vec![
                Item::new("japanese hiragana: ka"),
                Item::new("japanese katakana: neko"),
            ]),
            Item::new("japanese vocab").with_dependents(vec![
                Item::new("japanese vocab: neko -> cat"),
                Item::new("japanese vocab: mizu -> water"),
            ]),
        ]);

        let tech = Item::new("software engineering").with_dependents(vec![
            Item::new("rust").with_dependents(vec![
                Item::new("rust ownership"),
                Item::new("rust borrow checker"),
                Item::new("rust trait bounds"),
            ]),
            Item::new("kubernetes").with_dependents(vec![
                Item::new("kubernetes pod"),
                Item::new("kubernetes deployment"),
                Item::new("kubernetes service"),
            ]),
            Item::new("postgres").with_dependents(vec![
                Item::new("postgres btree index"),
                Item::new("postgres transaction isolation"),
            ]),
        ]);

        let history = Item::new("history").with_dependents(vec![
            Item::new("french revolution").with_dependents(vec![
                Item::new("french revolution: estates-general"),
                Item::new("french revolution: storming of the bastille"),
            ]),
            Item::new("world war ii").with_dependents(vec![
                Item::new("ww2 normandy landings"),
                Item::new("ww2 battle of britain"),
                Item::new("ww2 manhattan project"),
            ]),
        ]);

        let state = State::new(
            vec![spanish, japanese, tech, history],
            vec![
                CmpEntry::Cmp(Cmp::new("software engineering", "spanish basics")),
                CmpEntry::Cmp(Cmp::new("spanish basics", "japanese basics")),
                CmpEntry::Cmp(Cmp::new("world war ii", "french revolution")),
                CmpEntry::Cmp(Cmp::new("kubernetes", "spanish greetings")),
                CmpEntry::Cmp(Cmp::new(
                    "rust ownership",
                    "french revolution: estates-general",
                )),
                CmpEntry::Cmp(Cmp::new("japanese kana", "spanish adjective agreement")),
                CmpEntry::Cmp(Cmp::new("postgres btree index", "ww2 normandy landings")),
            ],
        );

        let res = state.prioritize().unwrap();
        let mut weights: Vec<_> = res
            .0
            .values()
            .map(|(item, score)| (item.name.as_str(), *score))
            .collect();
        weights.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));

        println!("\nrealistic flashcard importance weights:");
        for (name, score) in weights {
            println!("{:>5.1}  {}", score.into_inner(), name);
        }
    }
}
