use ordered_float::OrderedFloat;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

type Float = OrderedFloat<f32>;

/// A node in a dependency graph that can be prioritized.
///
/// Implementors expose a stable identifier plus their *direct* dependencies and
/// dependents. Those are returned as `Arc<Self>` so the graph can keep walking
/// outwards, pulling further dependencies/dependents off each returned node.
pub trait Node {
    /// A stable, orderable identity for this node (e.g. a card id or name).
    type Id: Ord + Clone + std::fmt::Debug;

    /// This node's identifier.
    fn id(&self) -> Self::Id;

    /// The nodes this node directly depends on (its prerequisites).
    fn dependencies(&self) -> Vec<Arc<Self>>;

    /// The nodes that directly depend on this node.
    fn dependents(&self) -> Vec<Arc<Self>>;
}

/// Computes an importance score for every node reachable from `items`.
///
/// This is the public entry point: pass the root nodes (the graph is expanded
/// by walking each node's dependencies/dependents) and get back `(id, score)`
/// pairs. Scores use the soft "density" distribution pass. No pairwise
/// comparisons are applied here yet.
pub fn importance<N: Node>(items: Vec<Arc<N>>) -> Vec<(N::Id, f32)> {
    let graph = Graph::new(items);
    let scores = resolve_scores_distributed(&graph, &[]);
    scores
        .into_iter()
        .map(|(id, score)| (id, score.into_inner()))
        .collect()
}

fn prioritize_vec<N: Node>(items: Vec<N>, cmps: Vec<Cmp<N::Id>>) -> Vec<(Arc<N>, Float)> {
    prioritize(items.into_iter().map(Arc::new).collect(), cmps).into_vec()
}

fn prioritize<N: Node>(items: Vec<Arc<N>>, cmps: Vec<Cmp<N::Id>>) -> PriRes<N> {
    let graph = Graph::new(items);
    let scores = resolve_scores(&graph, &cmps);
    to_pri_res(graph, scores)
}

fn prioritize_distributed<N: Node>(
    items: Vec<Arc<N>>,
    cmps: Vec<Cmp<N::Id>>,
) -> PriRes<N> {
    let graph = Graph::new(items);
    let scores = resolve_scores_distributed(&graph, &cmps);
    to_pri_res(graph, scores)
}

fn to_pri_res<N: Node>(graph: Graph<N>, scores: BTreeMap<N::Id, Float>) -> PriRes<N> {
    PriRes(
        graph
            .items
            .into_iter()
            .map(|(id, item)| {
                let score = scores[&id];
                (id, (item, score))
            })
            .collect(),
    )
}

/// The pieces needed to score a graph: every node's dependency closure and the
/// pairwise ordering constraints.
struct Constraints<Id> {
    names: Vec<Id>,
    /// `closures[x]` = x plus every unique recursive dependent of x.
    closures: BTreeMap<Id, BTreeSet<Id>>,
    /// `greater[x]` = nodes x must rank above.
    greater: BTreeMap<Id, BTreeSet<Id>>,
    /// `lesser[x]` = nodes that must rank above x.
    lesser: BTreeMap<Id, BTreeSet<Id>>,
}

fn build_constraints<N: Node>(graph: &Graph<N>, cmps: &[Cmp<N::Id>]) -> Constraints<N::Id> {
    let names: Vec<N::Id> = graph.items.keys().cloned().collect();

    let closures: BTreeMap<N::Id, BTreeSet<N::Id>> = names
        .iter()
        .map(|name| {
            let mut closure = graph.recursive_dependents(name);
            closure.insert(name.clone());
            (name.clone(), closure)
        })
        .collect();

    let mut greater: BTreeMap<N::Id, BTreeSet<N::Id>> =
        names.iter().map(|n| (n.clone(), BTreeSet::new())).collect();
    let mut lesser: BTreeMap<N::Id, BTreeSet<N::Id>> =
        names.iter().map(|n| (n.clone(), BTreeSet::new())).collect();
    for cmp in cmps {
        greater
            .get_mut(&cmp.higher)
            .unwrap()
            .insert(cmp.lower.clone());
        lesser
            .get_mut(&cmp.lower)
            .unwrap()
            .insert(cmp.higher.clone());
    }

    Constraints {
        names,
        closures,
        greater,
        lesser,
    }
}

/// A node's value is the sum of the weights across its dependency closure.
fn closure_value<Id: Ord>(
    closures: &BTreeMap<Id, BTreeSet<Id>>,
    weights: &BTreeMap<Id, f32>,
    name: &Id,
) -> f32 {
    closures[name].iter().map(|dep| weights[dep]).sum()
}

/// Raises node weights from `init` until every pairwise comparison holds.
///
/// Because a prerequisite's closure is a superset of the closures of everything
/// that depends on it, two hard invariants hold regardless of the weights:
///   1. a node is at least as valuable as the unique nodes that depend on it
///      (a shared dependent is only counted once), and
///   2. a node never outranks a node it depends on.
///
/// Weights only ever *increase* from `init`, never below it. A node squeezed
/// between a lower and an upper anchor is placed at the midpoint of the two,
/// which makes several nodes between the same anchors spread out evenly. A
/// comparison with no upper anchor to interpolate against just bumps the node
/// 1.0 above the thing it must beat.
fn relax_weights<Id: Ord + Clone>(
    c: &Constraints<Id>,
    init: &BTreeMap<Id, f32>,
) -> BTreeMap<Id, f32> {
    let mut weights = init.clone();

    // Weights only increase and are bounded above by the comparison anchors, so
    // this relaxation converges; the loop bound is just a safety net.
    for _ in 0..10_000 {
        let values: BTreeMap<Id, f32> = c
            .names
            .iter()
            .map(|name| (name.clone(), closure_value(&c.closures, &weights, name)))
            .collect();

        let mut changed = false;
        for name in &c.names {
            let below = &c.greater[name];
            let above = &c.lesser[name];
            if below.is_empty() && above.is_empty() {
                continue;
            }

            let lower_anchor = below.iter().map(|y| values[y]).reduce(f32::max);
            let upper_anchor = above.iter().map(|z| values[z]).reduce(f32::min);

            let current = values[name];
            let dependents_weight = current - weights[name];

            let target = match (lower_anchor, upper_anchor) {
                (Some(lo), Some(hi)) if hi > lo => (lo + hi) / 2.0,
                (Some(lo), _) => lo + 1.0,
                (None, _) => current,
            };

            let new_weight = (target - dependents_weight).max(weights[name]);
            if new_weight > weights[name] + f32::EPSILON {
                weights.insert(name.clone(), new_weight);
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }

    weights
}

fn scores_from_weights<Id: Ord + Clone>(
    c: &Constraints<Id>,
    weights: &BTreeMap<Id, f32>,
) -> BTreeMap<Id, Float> {
    c.names
        .iter()
        .map(|name| {
            (
                name.clone(),
                float(closure_value(&c.closures, weights, name)),
            )
        })
        .collect()
}

/// The strict resolver: dependency closures plus pairwise comparisons, with
/// every node starting at the default weight of 1.0.
fn resolve_scores<N: Node>(graph: &Graph<N>, cmps: &[Cmp<N::Id>]) -> BTreeMap<N::Id, Float> {
    let c = build_constraints(graph, cmps);
    let init: BTreeMap<N::Id, f32> = c.names.iter().map(|n| (n.clone(), 1.0_f32)).collect();
    let weights = relax_weights(&c, &init);
    scores_from_weights(&c, &weights)
}

/// The strict resolver followed by a soft "importance density" pass.
///
/// When a comparison pushes a node with dependents above its structural value,
/// that extra importance is spread down across its closure as a better default
/// for the dependents we haven't directly ranked: each node's baseline weight
/// becomes the density (`total / closure size`) of its most concentrated
/// ancestor category. An un-bumped category has density ~1.0, so it leaves its
/// dependents untouched; only an expressed preference lifts a whole area. The
/// strict relaxation then runs again on top of those baselines, so any actual
/// pairwise comparison still wins over the estimate.
fn resolve_scores_distributed<N: Node>(
    graph: &Graph<N>,
    cmps: &[Cmp<N::Id>],
) -> BTreeMap<N::Id, Float> {
    let c = build_constraints(graph, cmps);

    let strict_init: BTreeMap<N::Id, f32> = c.names.iter().map(|n| (n.clone(), 1.0_f32)).collect();
    let strict = relax_weights(&c, &strict_init);

    // Each node inherits the highest density among the categories it belongs to.
    let mut baseline: BTreeMap<N::Id, f32> =
        c.names.iter().map(|n| (n.clone(), 1.0_f32)).collect();
    for category in &c.names {
        let total = closure_value(&c.closures, &strict, category);
        let density = total / c.closures[category].len() as f32;
        for member in &c.closures[category] {
            let entry = baseline.get_mut(member).unwrap();
            if density > *entry {
                *entry = density;
            }
        }
    }

    let weights = relax_weights(&c, &baseline);
    scores_from_weights(&c, &weights)
}

struct Graph<N: Node> {
    items: BTreeMap<N::Id, Arc<N>>,
    /// Edges from a node to the nodes which depend on it.
    dependents: BTreeMap<N::Id, BTreeSet<N::Id>>,
}

impl<N: Node> Graph<N> {
    fn new(items: Vec<Arc<N>>) -> Self {
        let mut graph = Self {
            items: BTreeMap::new(),
            dependents: BTreeMap::new(),
        };

        for item in items {
            graph.add_item(item);
        }

        graph
    }

    fn add_item(&mut self, item: Arc<N>) {
        let id = item.id();

        // Guard against re-processing. Real graphs populate edges in *both*
        // directions (a node lists its dependencies and its dependents), so
        // without this the traversal would loop forever. Every node records
        // all of its own edges the first (and only) time it is added, which is
        // enough to capture every edge in the graph.
        if self.items.contains_key(&id) {
            return;
        }
        self.items.insert(id.clone(), item.clone());
        self.dependents.entry(id.clone()).or_default();

        for dependent in item.dependents() {
            let dependent_id = dependent.id();
            self.dependents
                .entry(id.clone())
                .or_default()
                .insert(dependent_id);
            self.add_item(dependent);
        }

        for dependency in item.dependencies() {
            let dependency_id = dependency.id();
            self.dependents
                .entry(dependency_id)
                .or_default()
                .insert(id.clone());
            self.add_item(dependency);
        }
    }

    #[allow(dead_code)]
    fn dependency_scores(&self) -> BTreeMap<N::Id, Float> {
        self.items
            .keys()
            .map(|name| {
                let score = self.recursive_dependents(name).len() as f32 + 1.0;
                (name.clone(), float(score))
            })
            .collect()
    }

    fn recursive_dependents(&self, name: &N::Id) -> BTreeSet<N::Id> {
        let mut seen = BTreeSet::new();
        self.collect_dependents(name, &mut seen);
        seen
    }

    fn collect_dependents(&self, name: &N::Id, seen: &mut BTreeSet<N::Id>) {
        if let Some(dependents) = self.dependents.get(name) {
            for dependent in dependents {
                if seen.insert(dependent.clone()) {
                    self.collect_dependents(dependent, seen);
                }
            }
        }
    }

    fn is_dependency_order(&self, higher: &N::Id, lower: &N::Id) -> bool {
        self.recursive_dependents(higher).contains(lower)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct Item {
    name: String,
    dependencies: Vec<Arc<Self>>,
    dependents: Vec<Arc<Self>>,
}

impl Node for Item {
    type Id = String;

    fn id(&self) -> String {
        self.name.clone()
    }

    fn dependencies(&self) -> Vec<Arc<Self>> {
        self.dependencies.clone()
    }

    fn dependents(&self) -> Vec<Arc<Self>> {
        self.dependents.clone()
    }
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

/// State which is used to calculate the priority of nodes.
/// Invariants that must hold:
/// A node cannot have lower importance than the sum of all its recursive dependents.
/// Comparisons must be consistent, meaning no contradictions.
struct State<N: Node> {
    items: Vec<Arc<N>>,
    comparisons: Vec<CmpEntry<N::Id>>,
}

impl<N: Node> Default for State<N> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            comparisons: Vec::new(),
        }
    }
}

impl<N: Node> State<N> {
    fn new(items: Vec<N>, comparisons: Vec<CmpEntry<N::Id>>) -> Self {
        Self {
            items: items.into_iter().map(Arc::new).collect(),
            comparisons,
        }
    }

    /// Returns a list of comparisons that have been fixed.
    fn fixed_entries(&self) -> Vec<Cmp<N::Id>> {
        let mut entries: Vec<Option<Cmp<N::Id>>> = vec![];
        let mut seen: BTreeMap<(N::Id, N::Id), usize> = Default::default();

        for entry in &self.comparisons {
            match entry {
                CmpEntry::Cmp(cmp) => {
                    insert_cmp(&mut entries, &mut seen, cmp.clone());
                }
                CmpEntry::Fix(cmp_fix) => {
                    let fixed_items: BTreeSet<N::Id> = cmp_fix.0.iter().cloned().collect();
                    let kept_identities: BTreeSet<(N::Id, N::Id)> = cmp_fix
                        .0
                        .windows(2)
                        .map(|chunk| Cmp::new(chunk[0].clone(), chunk[1].clone()).identity())
                        .collect();
                    let seen_identities: Vec<(N::Id, N::Id)> = seen.keys().cloned().collect();

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
                        let cmp = Cmp::new(chunk[0].clone(), chunk[1].clone());
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

    /// Removes comparisons that are invalidated by the dependency system.
    /// Tuple first of valid comparisons, second of invalid comparisons.
    /// Ensures that a dependency cannot be ranked higher than a node it depends on.
    /// The reason it's not a hard error is because the user can change the dependency graph after already
    /// having made comparisons.
    fn remove_invalid_entries(&self) -> (Vec<Cmp<N::Id>>, Vec<Cmp<N::Id>>) {
        let graph = Graph::new(self.items.clone());
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
    /// Or if a dependent of a node is ranked as more important than the node itself.
    #[allow(dead_code)]
    fn is_valid(&self) -> bool {
        self.invalid_cycle().is_none()
    }

    fn prioritize(&self) -> Result<PriRes<N>, ErrState<N::Id>> {
        if let Some(cycle) = self.invalid_cycle() {
            return Err(ErrState::InvalidComparisons { cycle });
        } else {
            let (valid_comparisons, _invalid_comparisons) = self.remove_invalid_entries();
            Ok(prioritize(self.items.clone(), valid_comparisons))
        }
    }

    /// Like [`prioritize`](Self::prioritize) but spreads comparison-driven
    /// importance down onto a bumped node's dependents (the soft density pass).
    fn prioritize_distributed(&self) -> Result<PriRes<N>, ErrState<N::Id>> {
        if let Some(cycle) = self.invalid_cycle() {
            return Err(ErrState::InvalidComparisons { cycle });
        } else {
            let (valid_comparisons, _invalid_comparisons) = self.remove_invalid_entries();
            Ok(prioritize_distributed(self.items.clone(), valid_comparisons))
        }
    }

    fn invalid_cycle(&self) -> Option<Vec<N::Id>> {
        let (valid_comparisons, _) = self.remove_invalid_entries();
        find_cycle(&valid_comparisons)
    }
}

fn insert_cmp<Id: Ord + Clone>(
    entries: &mut Vec<Option<Cmp<Id>>>,
    seen: &mut BTreeMap<(Id, Id), usize>,
    cmp: Cmp<Id>,
) {
    let identity = cmp.identity();
    if let Some(index) = seen.get(&identity) {
        entries[*index] = Some(cmp);
    } else {
        seen.insert(identity, entries.len());
        entries.push(Some(cmp));
    }
}

fn find_cycle<Id: Ord + Clone>(comparisons: &[Cmp<Id>]) -> Option<Vec<Id>> {
    let mut graph: BTreeMap<Id, Vec<Id>> = BTreeMap::new();

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

fn find_cycle_from<Id: Ord + Clone>(
    node: &Id,
    graph: &BTreeMap<Id, Vec<Id>>,
    stack: &mut Vec<Id>,
    on_stack: &mut BTreeSet<Id>,
) -> Option<Vec<Id>> {
    if let Some(start) = stack.iter().position(|name| name == node) {
        return Some(stack[start..].to_vec());
    }

    if !on_stack.insert(node.clone()) {
        return None;
    }

    stack.push(node.clone());

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
struct PriRes<N: Node>(BTreeMap<N::Id, (Arc<N>, Float)>);

impl<N: Node> PriRes<N> {
    fn into_vec(self) -> Vec<(Arc<N>, Float)> {
        self.0
            .into_iter()
            .map(|(_, (item, score))| (item, score))
            .collect()
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ErrState<Id> {
    InvalidComparisons { cycle: Vec<Id> },
}

enum CmpEntry<Id> {
    Cmp(Cmp<Id>),
    Fix(CmpFix<Id>),
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct Cmp<Id> {
    higher: Id,
    lower: Id,
}

impl<Id: Ord + Clone> Cmp<Id> {
    fn new(higher: impl Into<Id>, lower: impl Into<Id>) -> Self {
        Self {
            higher: higher.into(),
            lower: lower.into(),
        }
    }

    /// Returns a tuple of the two node ids, in sorted order.
    fn identity(&self) -> (Id, Id) {
        if self.higher < self.lower {
            (self.higher.clone(), self.lower.clone())
        } else {
            (self.lower.clone(), self.higher.clone())
        }
    }
}

struct CmpFix<Id>(Vec<Id>);

impl<Id> CmpFix<Id> {
    fn new(items: Vec<impl Into<Id>>) -> Self {
        Self(items.into_iter().map(Into::into).collect())
    }
}

fn float(f: f32) -> Float {
    Float::from(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn test_eq(mut expected: Vec<(&str, Float)>, mut received: Vec<(Arc<Item>, Float)>) {
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

    fn approx(actual: Float, expected: f32) -> bool {
        (actual.into_inner() - expected).abs() < 1e-3
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

    /// A card compared between a low and a high anchor lands at the midpoint,
    /// rather than just barely above the thing it must beat.
    #[test]
    fn cmp_interpolates_between_anchors() {
        let big = Item::new("big").with_dependents(vec![
            Item::new("d1"),
            Item::new("d2"),
            Item::new("d3"),
            Item::new("d4"),
        ]);
        let mid = Item::new("mid");
        let low = Item::new("low");

        let state = State::new(
            vec![big, mid, low],
            vec![
                CmpEntry::Cmp(Cmp::new("big", "mid")),
                CmpEntry::Cmp(Cmp::new("mid", "low")),
            ],
        );
        let res = state.prioritize().unwrap();

        // big is anchored high by its 4 dependents, low stays at the floor.
        assert_eq!(res.0["low"].1, float(1.0));
        assert_eq!(res.0["big"].1, float(5.0));
        // mid interpolates to the middle of low (1) and big (5).
        assert_eq!(res.0["mid"].1, float(3.0));
    }

    /// Several cards between the same two anchors spread out evenly.
    #[test]
    fn cmp_even_spacing_between_anchors() {
        let big = Item::new("big").with_dependents(vec![
            Item::new("d1"),
            Item::new("d2"),
            Item::new("d3"),
            Item::new("d4"),
            Item::new("d5"),
            Item::new("d6"),
        ]);
        let m1 = Item::new("m1");
        let m2 = Item::new("m2");
        let low = Item::new("low");

        let state = State::new(
            vec![big, m1, m2, low],
            vec![
                CmpEntry::Cmp(Cmp::new("big", "m1")),
                CmpEntry::Cmp(Cmp::new("m1", "m2")),
                CmpEntry::Cmp(Cmp::new("m2", "low")),
            ],
        );
        let res = state.prioritize().unwrap();

        // Anchors at low=1 and big=7, with m1/m2 evenly spaced between: 1,3,5,7.
        assert_eq!(res.0["low"].1, float(1.0));
        assert_eq!(res.0["big"].1, float(7.0));
        assert!(approx(res.0["m2"].1, 3.0), "m2 = {:?}", res.0["m2"].1);
        assert!(approx(res.0["m1"].1, 5.0), "m1 = {:?}", res.0["m1"].1);
    }

    /// Raising a card via a comparison lifts the cards that depend on it (its
    /// prerequisites), and a shared dependent is still only counted once.
    #[test]
    fn cmp_bump_propagates_up_dependency() {
        let foo = Item::new("foo").with_dependents(vec![
            Item::new("bar").with_dependents(vec![Item::new("shared")]),
            Item::new("baz").with_dependents(vec![Item::new("shared")]),
        ]);

        let state = State::new(vec![foo], vec![CmpEntry::Cmp(Cmp::new("bar", "baz"))]);
        let res = state.prioritize().unwrap();

        assert_eq!(res.0["shared"].1, float(1.0));
        assert_eq!(res.0["baz"].1, float(2.0));
        // bar is bumped one above baz.
        assert_eq!(res.0["bar"].1, float(3.0));
        // foo rises to reflect bar's bump, but shared is still counted once
        // (1 own + bar's extra weight + baz + shared = 1 + 2 + 1 + 1).
        assert_eq!(res.0["foo"].1, float(5.0));
        assert!(res.0["foo"].1 > res.0["bar"].1);
    }

    /// A card can be bumped by a comparison but never ends up outranking a card
    /// it depends on; the prerequisite simply grows to stay above it.
    #[test]
    fn cmp_cannot_exceed_prerequisite() {
        let prereq = Item::new("prereq").with_dependents(vec![Item::new("dependent")]);
        let other = Item::new("other");

        let state = State::new(
            vec![prereq, other],
            vec![CmpEntry::Cmp(Cmp::new("dependent", "other"))],
        );
        let res = state.prioritize().unwrap();

        assert_eq!(res.0["other"].1, float(1.0));
        assert_eq!(res.0["dependent"].1, float(2.0));
        assert_eq!(res.0["prereq"].1, float(3.0));
        assert!(res.0["prereq"].1 > res.0["dependent"].1);
    }

    /// Comparing one sibling against another leaves the untouched sibling at the
    /// floor, and the shared prerequisite stays above the sum of its dependents.
    #[test]
    fn cmp_between_siblings() {
        let vocab = Item::new("vocab").with_dependents(vec![
            Item::new("w1"),
            Item::new("w2"),
            Item::new("w3"),
        ]);

        let state = State::new(vec![vocab], vec![CmpEntry::Cmp(Cmp::new("w1", "w2"))]);
        let res = state.prioritize().unwrap();

        assert_eq!(res.0["w2"].1, float(1.0));
        assert_eq!(res.0["w3"].1, float(1.0));
        assert_eq!(res.0["w1"].1, float(2.0));
        assert_eq!(res.0["vocab"].1, float(5.0));
        let words = res.0["w1"].1 + res.0["w2"].1 + res.0["w3"].1;
        assert!(res.0["vocab"].1 > words);
    }

    /// Comparisons never drag a card below its structural floor.
    #[test]
    fn cmp_never_below_floor() {
        // foo has two dependents, floor 3; even though we say leaf > foo, foo
        // stays at its floor and leaf is lifted above it.
        let foo = Item::new("foo").with_dependents(vec![Item::new("bar"), Item::new("baz")]);
        let leaf = Item::new("leaf");

        let state = State::new(
            vec![foo, leaf],
            vec![CmpEntry::Cmp(Cmp::new("leaf", "foo"))],
        );
        let res = state.prioritize().unwrap();

        assert_eq!(res.0["foo"].1, float(3.0));
        assert_eq!(res.0["leaf"].1, float(4.0));
        assert!(res.0["leaf"].1 > res.0["foo"].1);
    }

    /// The density pass is a no-op when nothing was bumped: every category has
    /// density ~1.0, so dependents keep their structural values.
    #[test]
    fn distribution_inactive_without_bump() {
        let foo = Item::new("foo").with_dependents(vec![Item::new("bar"), Item::new("baz")]);
        let state = State::new(vec![foo], vec![]);
        let res = state.prioritize_distributed().unwrap();

        assert_eq!(res.0["foo"].1, float(3.0));
        assert_eq!(res.0["bar"].1, float(1.0));
        assert_eq!(res.0["baz"].1, float(1.0));
    }

    /// Bumping a topic above its structural value spreads that importance down
    /// onto its dependents, which rise from the default 1.0 toward the topic's
    /// per-card density.
    #[test]
    fn distribution_raises_dependents_when_topic_bumped() {
        let cat = Item::new("cat").with_dependents(vec![
            Item::new("w1"),
            Item::new("w2"),
            Item::new("w3"),
        ]);
        // A bigger topic to compare against; cat must outrank it, lifting cat to
        // 10 (closure of 4 -> density 2.5).
        let big =
            Item::new("big").with_dependents((0..8).map(|i| Item::new(&format!("d{i}"))).collect());

        let state = State::new(vec![cat, big], vec![CmpEntry::Cmp(Cmp::new("cat", "big"))]);
        let res = state.prioritize_distributed().unwrap();

        assert_eq!(res.0["cat"].1, float(10.0));
        assert_eq!(res.0["w1"].1, float(2.5));
        assert_eq!(res.0["w2"].1, float(2.5));
        assert_eq!(res.0["w3"].1, float(2.5));
        // big was never bumped, so its dependents keep the default weight.
        assert_eq!(res.0["big"].1, float(9.0));
        assert_eq!(res.0["d0"].1, float(1.0));
    }

    /// A preferred but smaller topic gives each of its cards more importance
    /// than a larger, un-preferred topic gives each of its own.
    #[test]
    fn distribution_prefers_denser_topic() {
        let spanish = Item::new("spanish").with_dependents(vec![Item::new("s1"), Item::new("s2")]);
        let japanese = Item::new("japanese").with_dependents(vec![
            Item::new("j1"),
            Item::new("j2"),
            Item::new("j3"),
            Item::new("j4"),
        ]);

        let state = State::new(
            vec![spanish, japanese],
            vec![CmpEntry::Cmp(Cmp::new("spanish", "japanese"))],
        );
        let res = state.prioritize_distributed().unwrap();

        // spanish bumped to 6 over a closure of 3 -> density 2 for its words.
        assert_eq!(res.0["spanish"].1, float(6.0));
        assert_eq!(res.0["s1"].1, float(2.0));
        assert_eq!(res.0["s2"].1, float(2.0));
        // japanese is the larger, un-preferred topic, so its words stay at 1.
        assert_eq!(res.0["japanese"].1, float(5.0));
        assert_eq!(res.0["j1"].1, float(1.0));
        assert!(res.0["s1"].1 > res.0["j1"].1);
    }

    /// A direct comparison between dependents still wins after the density pass:
    /// the estimate lifts both, then the strict relaxation re-separates them.
    #[test]
    fn distribution_respects_leaf_comparison() {
        let cat = Item::new("cat").with_dependents(vec![
            Item::new("w1"),
            Item::new("w2"),
            Item::new("w3"),
        ]);
        let big =
            Item::new("big").with_dependents((0..8).map(|i| Item::new(&format!("d{i}"))).collect());

        let state = State::new(
            vec![cat, big],
            vec![
                CmpEntry::Cmp(Cmp::new("cat", "big")),
                CmpEntry::Cmp(Cmp::new("w1", "w2")),
            ],
        );
        let res = state.prioritize_distributed().unwrap();

        // The density estimate lifts every word above the default 1.0, and the
        // uncompared sibling w3 sits at the same level as the lower of the pair.
        assert!(res.0["w2"].1 > float(1.0));
        assert_eq!(res.0["w2"].1, res.0["w3"].1);
        // The strict comparison still wins: w1 is re-separated above w2.
        assert!(res.0["w1"].1 > res.0["w2"].1);
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

        let math = Item::new("mathematics").with_dependents(vec![
            Item::new("calculus")
                .with_dependents(vec![Item::new("derivative"), Item::new("integral")]),
            Item::new("linear algebra").with_dependents(vec![
                Item::new("matrix multiplication"),
                Item::new("eigenvalue"),
            ]),
        ]);

        let music = Item::new("music theory").with_dependents(vec![
            Item::new("scales")
                .with_dependents(vec![Item::new("major scale"), Item::new("minor scale")]),
            Item::new("chords")
                .with_dependents(vec![Item::new("triad"), Item::new("seventh chord")]),
        ]);

        let state = State::new(
            vec![spanish, japanese, tech, history, math, music],
            vec![
                // High-level cross-topic chain. software engineering and
                // japanese basics act as anchors, so the cards strung between
                // them interpolate to fractional, evenly-spaced values.
                CmpEntry::Cmp(Cmp::new("software engineering", "mathematics")),
                CmpEntry::Cmp(Cmp::new("mathematics", "music theory")),
                CmpEntry::Cmp(Cmp::new("music theory", "japanese basics")),
                // history also bracketed between software engineering and japanese.
                CmpEntry::Cmp(Cmp::new("software engineering", "history")),
                CmpEntry::Cmp(Cmp::new("history", "japanese basics")),
                // Topic-level preferences that are already satisfied structurally
                // (no-ops, kept for realism).
                CmpEntry::Cmp(Cmp::new("software engineering", "spanish basics")),
                CmpEntry::Cmp(Cmp::new("spanish basics", "japanese basics")),
                CmpEntry::Cmp(Cmp::new("world war ii", "french revolution")),
                CmpEntry::Cmp(Cmp::new("kubernetes", "spanish greetings")),
                // Leaf bumps that propagate up into their prerequisites.
                CmpEntry::Cmp(Cmp::new(
                    "rust ownership",
                    "french revolution: estates-general",
                )),
                CmpEntry::Cmp(Cmp::new("postgres btree index", "ww2 normandy landings")),
                CmpEntry::Cmp(Cmp::new(
                    "spanish vocab: gato -> cat",
                    "spanish vocab: trabajar -> to work",
                )),
                CmpEntry::Cmp(Cmp::new("japanese kana", "spanish adjective agreement")),
                // A leaf chain within one topic: even 1/2/3 spacing, no anchor above.
                CmpEntry::Cmp(Cmp::new("kubernetes pod", "kubernetes deployment")),
                CmpEntry::Cmp(Cmp::new("kubernetes deployment", "kubernetes service")),
            ],
        );

        let res = state.prioritize_distributed().unwrap();
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
        panic!();
    }
}
