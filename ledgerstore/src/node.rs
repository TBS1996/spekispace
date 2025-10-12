use std::{collections::VecDeque, num::NonZero};

use nonempty::NonEmpty;

#[derive(Clone, Hash, Debug)]
pub enum Node<T> {
    Leaf(T),
    Branch(Box<NonEmpty<Self>>),
}

impl<T> Node<T> {
    pub fn new_leaf(item: T) -> Self {
        Self::Leaf(item)
    }

    pub fn last(&self) -> &T {
        match self {
            Node::Leaf(item) => &item,
            Node::Branch(list) => list.last().last(),
        }
    }

    pub fn iter(&self) -> NodeIterRef<'_, T> {
        self.into_iter()
    }

    pub fn new_branch(children: NonEmpty<Self>) -> Self {
        Self::Branch(Box::new(children))
    }

    pub fn into_flatten(self) -> Vec<T> {
        match self {
            Node::Leaf(item) => vec![item],
            Node::Branch(children) => children.into_iter().flat_map(Node::into_flatten).collect(),
        }
    }

    pub fn flatten(&self) -> Vec<&T> {
        match self {
            Node::Leaf(item) => vec![item],
            Node::Branch(children) => children.iter().flat_map(Node::flatten).collect(),
        }
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self, Node::Leaf(_))
    }

    pub fn is_branch(&self) -> bool {
        matches!(self, Node::Branch(_))
    }

    /// Returns the number of leaves in the tree. Will never return 0.
    pub fn len(&self) -> usize {
        match self {
            Node::Leaf(_) => 1,
            Node::Branch(children) => children.iter().map(Node::len).sum(),
        }
    }

    /// Just messing around lol
    pub fn nonzero_len(&self) -> NonZero<usize> {
        NonZero::new(self.len()).unwrap()
    }
}

impl<T> From<T> for Node<T> {
    fn from(t: T) -> Self {
        Node::Leaf(t)
    }
}

impl<T> From<NonEmpty<Node<T>>> for Node<T> {
    fn from(ne: NonEmpty<Node<T>>) -> Self {
        Node::Branch(Box::new(ne))
    }
}

impl<'a, T> IntoIterator for &'a Node<T> {
    type Item = &'a T;
    type IntoIter = NodeIterRef<'a, T>; // you’d implement this

    fn into_iter(self) -> Self::IntoIter {
        NodeIterRef::new(self)
    }
}

pub struct NodeIterRef<'a, T> {
    dq: VecDeque<&'a Node<T>>,
}

impl<'a, T> NodeIterRef<'a, T> {
    pub fn new(root: &'a Node<T>) -> Self {
        let mut dq = VecDeque::new();
        dq.push_back(root);
        Self { dq }
    }

    #[inline]
    fn expand_front(dq: &mut VecDeque<&'a Node<T>>, children: &'a NonEmpty<Node<T>>) {
        for c in children.iter().rev() {
            dq.push_front(c);
        }
    }

    #[inline]
    fn expand_back(dq: &mut VecDeque<&'a Node<T>>, children: &'a NonEmpty<Node<T>>) {
        for c in children.iter() {
            dq.push_back(c);
        }
    }
}

impl<'a, T> DoubleEndedIterator for NodeIterRef<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        while let Some(n) = self.dq.pop_back() {
            match n {
                Node::Leaf(v) => return Some(v),
                Node::Branch(ch) => Self::expand_back(&mut self.dq, ch),
            }
        }
        None
    }
}

impl<'a, T> Iterator for NodeIterRef<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(n) = self.dq.pop_front() {
            match n {
                Node::Leaf(v) => return Some(v),
                Node::Branch(ch) => Self::expand_front(&mut self.dq, ch),
            }
        }
        None
    }
}

pub struct NodeIter<T> {
    deque: VecDeque<Node<T>>,
}

impl<T> NodeIter<T> {
    fn new(root: Node<T>) -> Self {
        Self {
            deque: VecDeque::from(vec![root]),
        }
    }

    fn expand_front(children: Box<NonEmpty<Node<T>>>, deque: &mut VecDeque<Node<T>>) {
        let NonEmpty { head, mut tail } = *children;

        tail.push(head);
        for child in tail.into_iter().rev() {
            deque.push_front(child);
        }
    }

    fn expand_back(children: Box<NonEmpty<Node<T>>>, deque: &mut VecDeque<Node<T>>) {
        let NonEmpty { head, tail } = *children;

        deque.push_back(head);
        for child in tail.into_iter() {
            deque.push_back(child);
        }
    }
}

impl<T> DoubleEndedIterator for NodeIter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.deque.pop_back() {
            match node {
                Node::Leaf(t) => return Some(t),
                Node::Branch(children) => Self::expand_back(children, &mut self.deque),
            }
        }
        None
    }
}

impl<T> Iterator for NodeIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.deque.pop_front() {
            match node {
                Node::Leaf(t) => return Some(t),
                Node::Branch(children) => Self::expand_front(children, &mut self.deque),
            }
        }
        None
    }
}

impl<T> IntoIterator for Node<T> {
    type Item = T;
    type IntoIter = NodeIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        NodeIter::new(self)
    }
}
