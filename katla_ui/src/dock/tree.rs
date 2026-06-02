use katla_math::{Rect2D, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DockPath(pub Vec<usize>);

impl DockPath {
    pub fn root() -> Self {
        DockPath(Vec::new())
    }

    pub fn push(&mut self, index: usize) {
        self.0.push(index);
    }

    pub fn pop(&mut self) -> Option<usize> {
        self.0.pop()
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_slice(&self) -> &[usize] {
        &self.0
    }

    pub fn parent(&self) -> Option<DockPath> {
        if self.0.is_empty() {
            None
        } else {
            let mut parent = self.clone();
            parent.0.pop();
            Some(parent)
        }
    }

    pub fn child(&self, index: usize) -> DockPath {
        let mut child = self.clone();
        child.0.push(index);
        child
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockZone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DockNode<T: Clone + PartialEq> {
    Split {
        direction: SplitDirection,
        ratio: f32,
        children: [Box<DockNode<T>>; 2],
    },
    Leaf {
        tabs: Vec<T>,
        active: usize,
    },
    Empty,
}

impl<T: Clone + PartialEq> DockNode<T> {
    fn is_leaf_or_empty(&self) -> bool {
        matches!(self, DockNode::Leaf { .. } | DockNode::Empty)
    }

    fn get(&self, path: &[usize]) -> Option<&DockNode<T>> {
        if path.is_empty() {
            return Some(self);
        }
        match self {
            DockNode::Split { children, .. } => {
                let index = path[0];
                if index < 2 {
                    children[index].get(&path[1..])
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn get_mut(&mut self, path: &[usize]) -> Option<&mut DockNode<T>> {
        if path.is_empty() {
            return Some(self);
        }
        match self {
            DockNode::Split { children, .. } => {
                let index = path[0];
                if index < 2 {
                    children[index].get_mut(&path[1..])
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn leaf_bounds_recursive(&self, area: Rect2D, path: &mut DockPath) -> Vec<(DockPath, Rect2D)> {
        match self {
            DockNode::Empty => {
                vec![(path.clone(), area)]
            }
            DockNode::Leaf { .. } => {
                vec![(path.clone(), area)]
            }
            DockNode::Split {
                direction,
                ratio,
                children,
            } => {
                let clamped = ratio.clamp(0.0, 1.0);
                let (area0, area1) = match direction {
                    SplitDirection::Horizontal => {
                        let split_x = area.min.x() + area.width() * clamped;
                        let area0 = Rect2D::new(area.min, Vec2::new(split_x, area.max.y()));
                        let area1 = Rect2D::new(Vec2::new(split_x, area.min.y()), area.max);
                        (area0, area1)
                    }
                    SplitDirection::Vertical => {
                        let split_y = area.min.y() + area.height() * clamped;
                        let area0 = Rect2D::new(area.min, Vec2::new(area.max.x(), split_y));
                        let area1 = Rect2D::new(Vec2::new(area.min.x(), split_y), area.max);
                        (area0, area1)
                    }
                };

                let mut result = Vec::new();
                path.push(0);
                result.extend(children[0].leaf_bounds_recursive(area0, path));
                path.pop();

                path.push(1);
                result.extend(children[1].leaf_bounds_recursive(area1, path));
                path.pop();

                result
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DockError {
    NodeNotFound,
    NotALeaf,
    TabNotFound,
    InvalidPath,
    CannotSplitEmpty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DockTree<T: Clone + PartialEq> {
    root: DockNode<T>,
}

impl<T: Clone + PartialEq> DockTree<T> {
    pub fn new(root: DockNode<T>) -> Self {
        DockTree { root }
    }

    pub fn root(&self) -> &DockNode<T> {
        &self.root
    }

    pub fn root_mut(&mut self) -> &mut DockNode<T> {
        &mut self.root
    }

    pub fn get(&self, path: &DockPath) -> Option<&DockNode<T>> {
        self.root.get(path.as_slice())
    }

    pub fn get_mut(&mut self, path: &DockPath) -> Option<&mut DockNode<T>> {
        self.root.get_mut(path.as_slice())
    }

    pub fn leaf_bounds(&self, area: Rect2D) -> Vec<(DockPath, Rect2D)> {
        self.root.leaf_bounds_recursive(area, &mut DockPath::root())
    }

    pub fn split_leaf(
        &mut self,
        path: &DockPath,
        direction: SplitDirection,
        ratio: f32,
    ) -> Result<(), DockError> {
        let node = self
            .root
            .get_mut(path.as_slice())
            .ok_or(DockError::NodeNotFound)?;

        match node {
            DockNode::Leaf { tabs, .. } => {
                let old_leaf = DockNode::Leaf {
                    tabs: std::mem::take(tabs),
                    active: 0,
                };
                *node = DockNode::Split {
                    direction,
                    ratio: ratio.clamp(0.0, 1.0),
                    children: [Box::new(old_leaf), Box::new(DockNode::Empty)],
                };
                Ok(())
            }
            DockNode::Empty => Err(DockError::CannotSplitEmpty),
            DockNode::Split { .. } => Err(DockError::NotALeaf),
        }
    }

    pub fn remove_tab(&mut self, path: &DockPath, tab: &T) -> Result<(), DockError> {
        {
            let node = self
                .root
                .get_mut(path.as_slice())
                .ok_or(DockError::NodeNotFound)?;
            match node {
                DockNode::Leaf { tabs, active } => {
                    let pos = tabs
                        .iter()
                        .position(|t| t == tab)
                        .ok_or(DockError::TabNotFound)?;
                    tabs.remove(pos);
                    if tabs.is_empty() {
                        *active = 0;
                    } else if *active >= tabs.len() {
                        *active = tabs.len() - 1;
                    } else if pos < *active {
                        *active -= 1;
                    }
                }
                _ => return Err(DockError::NotALeaf),
            }
        }

        self.collapse_empty(path);
        Ok(())
    }

    pub fn move_tab(
        &mut self,
        from_path: &DockPath,
        to_path: &DockPath,
        zone: DockZone,
    ) -> Result<(), DockError> {
        let tab = {
            let node = self
                .root
                .get(from_path.as_slice())
                .ok_or(DockError::NodeNotFound)?;
            match node {
                DockNode::Leaf { tabs, .. } => tabs.first().cloned().ok_or(DockError::TabNotFound),
                _ => Err(DockError::NotALeaf),
            }
        }?;

        let _tab = match zone {
            DockZone::Center => {
                let from_node = self
                    .root
                    .get_mut(from_path.as_slice())
                    .ok_or(DockError::NodeNotFound)?;
                match from_node {
                    DockNode::Leaf { tabs, active } => {
                        let pos = tabs
                            .iter()
                            .position(|t| t == &tab)
                            .ok_or(DockError::TabNotFound)?;
                        tabs.remove(pos);
                        if tabs.is_empty() {
                            *active = 0;
                        } else if *active >= tabs.len() {
                            *active = tabs.len() - 1;
                        } else if pos < *active {
                            *active -= 1;
                        }
                    }
                    _ => return Err(DockError::NotALeaf),
                }

                let to_node = self
                    .root
                    .get_mut(to_path.as_slice())
                    .ok_or(DockError::NodeNotFound)?;
                match to_node {
                    DockNode::Leaf { tabs, active } => {
                        tabs.push(tab.clone());
                        *active = tabs.len() - 1;
                    }
                    DockNode::Empty => {
                        *to_node = DockNode::Leaf {
                            tabs: vec![tab.clone()],
                            active: 0,
                        };
                    }
                    _ => return Err(DockError::NotALeaf),
                }

                self.collapse_empty(from_path);
                return Ok(());
            }
            DockZone::Left | DockZone::Right => {
                let direction = SplitDirection::Horizontal;
                let (old_idx, new_idx) = match zone {
                    DockZone::Left => (1, 0),
                    DockZone::Right => (0, 1),
                    _ => unreachable!(),
                };

                let from_node = self
                    .root
                    .get_mut(from_path.as_slice())
                    .ok_or(DockError::NodeNotFound)?;
                match from_node {
                    DockNode::Leaf { tabs, active } => {
                        let pos = tabs
                            .iter()
                            .position(|t| t == &tab)
                            .ok_or(DockError::TabNotFound)?;
                        tabs.remove(pos);
                        if tabs.is_empty() {
                            *active = 0;
                        } else if *active >= tabs.len() {
                            *active = tabs.len() - 1;
                        } else if pos < *active {
                            *active -= 1;
                        }
                    }
                    _ => return Err(DockError::NotALeaf),
                }

                let mut children = [Box::new(DockNode::Empty), Box::new(DockNode::Empty)];
                *children[new_idx] = DockNode::Leaf {
                    tabs: vec![tab.clone()],
                    active: 0,
                };

                let to_node = self
                    .root
                    .get_mut(to_path.as_slice())
                    .ok_or(DockError::NodeNotFound)?;
                let old_child = std::mem::replace(to_node, DockNode::Empty);
                *children[old_idx] = old_child;

                *to_node = DockNode::Split {
                    direction,
                    ratio: 0.5,
                    children,
                };

                self.collapse_empty(from_path);
                return Ok(());
            }
            DockZone::Top | DockZone::Bottom => {
                let direction = SplitDirection::Vertical;
                let (old_idx, new_idx) = match zone {
                    DockZone::Top => (1, 0),
                    DockZone::Bottom => (0, 1),
                    _ => unreachable!(),
                };

                let from_node = self
                    .root
                    .get_mut(from_path.as_slice())
                    .ok_or(DockError::NodeNotFound)?;
                match from_node {
                    DockNode::Leaf { tabs, active } => {
                        let pos = tabs
                            .iter()
                            .position(|t| t == &tab)
                            .ok_or(DockError::TabNotFound)?;
                        tabs.remove(pos);
                        if tabs.is_empty() {
                            *active = 0;
                        } else if *active >= tabs.len() {
                            *active = tabs.len() - 1;
                        } else if pos < *active {
                            *active -= 1;
                        }
                    }
                    _ => return Err(DockError::NotALeaf),
                }

                let mut children = [Box::new(DockNode::Empty), Box::new(DockNode::Empty)];
                *children[new_idx] = DockNode::Leaf {
                    tabs: vec![tab.clone()],
                    active: 0,
                };

                let to_node = self
                    .root
                    .get_mut(to_path.as_slice())
                    .ok_or(DockError::NodeNotFound)?;
                let old_child = std::mem::replace(to_node, DockNode::Empty);
                *children[old_idx] = old_child;

                *to_node = DockNode::Split {
                    direction,
                    ratio: 0.5,
                    children,
                };

                self.collapse_empty(from_path);
                return Ok(());
            }
        };
    }

    pub fn set_ratio(&mut self, path: &DockPath, ratio: f32) -> Result<(), DockError> {
        let node = self
            .root
            .get_mut(path.as_slice())
            .ok_or(DockError::NodeNotFound)?;
        match node {
            DockNode::Split { ratio: r, .. } => {
                *r = ratio.clamp(0.0, 1.0);
                Ok(())
            }
            _ => Err(DockError::InvalidPath),
        }
    }

    pub fn activate_tab(&mut self, path: &DockPath, tab: &T) -> Result<(), DockError> {
        let node = self
            .root
            .get_mut(path.as_slice())
            .ok_or(DockError::NodeNotFound)?;
        match node {
            DockNode::Leaf { tabs, active } => {
                let index = tabs
                    .iter()
                    .position(|t| t == tab)
                    .ok_or(DockError::TabNotFound)?;
                *active = index;
                Ok(())
            }
            _ => Err(DockError::NotALeaf),
        }
    }

    pub fn find_leaf_with_tab(&self, tab: &T) -> Option<DockPath> {
        self.find_leaf_with_tab_recursive(&self.root, &mut DockPath::root(), tab)
    }

    fn find_leaf_with_tab_recursive(
        &self,
        node: &DockNode<T>,
        path: &mut DockPath,
        tab: &T,
    ) -> Option<DockPath> {
        match node {
            DockNode::Leaf { tabs, .. } => {
                if tabs.iter().any(|t| t == tab) {
                    Some(path.clone())
                } else {
                    None
                }
            }
            DockNode::Split { children, .. } => {
                path.push(0);
                if let Some(found) = self.find_leaf_with_tab_recursive(&children[0], path, tab) {
                    path.pop();
                    return Some(found);
                }
                path.pop();

                path.push(1);
                let found = self.find_leaf_with_tab_recursive(&children[1], path, tab);
                path.pop();
                found
            }
            DockNode::Empty => None,
        }
    }

    fn collapse_empty(&mut self, path: &DockPath) {
        if path.is_root() {
            return;
        }

        let parent_path = match path.parent() {
            Some(p) => p,
            None => return,
        };

        let child_index = *path.as_slice().last().unwrap();
        let sibling_index = 1 - child_index;

        let should_collapse = {
            let parent = match self.root.get(parent_path.as_slice()) {
                Some(n) => n,
                None => return,
            };

            match parent {
                DockNode::Split { children, .. } => {
                    let child = &children[child_index];
                    let sibling = &children[sibling_index];
                    let child_is_empty = matches!(**child, DockNode::Empty)
                        || matches!(&**child, DockNode::Leaf { tabs, .. } if tabs.is_empty());
                    child.is_leaf_or_empty() && child != sibling && child_is_empty
                }
                _ => false,
            }
        };

        if should_collapse {
            let parent = self.root.get_mut(parent_path.as_slice()).unwrap();
            if let DockNode::Split { children, .. } = parent {
                let replacement =
                    std::mem::replace(&mut children[sibling_index], Box::new(DockNode::Empty));
                *parent = *replacement;

                if !parent_path.is_root() {
                    self.collapse_empty(&parent_path);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_leaf(tabs: Vec<u32>) -> DockNode<u32> {
        let active = if tabs.is_empty() { 0 } else { 0 };
        DockNode::Leaf { tabs, active }
    }

    fn make_split(
        direction: SplitDirection,
        ratio: f32,
        left: DockNode<u32>,
        right: DockNode<u32>,
    ) -> DockNode<u32> {
        DockNode::Split {
            direction,
            ratio,
            children: [Box::new(left), Box::new(right)],
        }
    }

    fn path(indices: &[usize]) -> DockPath {
        DockPath(indices.to_vec())
    }

    const TOL: f32 = 1e-4;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < TOL
    }

    fn assert_rect_eq(actual: Rect2D, expected_min: (f32, f32), expected_max: (f32, f32)) {
        assert!(
            approx_eq(actual.min.x(), expected_min.0),
            "min.x: {} vs {}",
            actual.min.x(),
            expected_min.0
        );
        assert!(
            approx_eq(actual.min.y(), expected_min.1),
            "min.y: {} vs {}",
            actual.min.y(),
            expected_min.1
        );
        assert!(
            approx_eq(actual.max.x(), expected_max.0),
            "max.x: {} vs {}",
            actual.max.x(),
            expected_max.0
        );
        assert!(
            approx_eq(actual.max.y(), expected_max.1),
            "max.y: {} vs {}",
            actual.max.y(),
            expected_max.1
        );
    }

    #[test]
    fn test_dock_node_split_field_access() {
        let split = make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        );
        if let DockNode::Split {
            direction,
            ratio,
            children,
        } = &split
        {
            assert_eq!(*direction, SplitDirection::Horizontal);
            assert!(approx_eq(*ratio, 0.5));
            assert_eq!(children.len(), 2);
        } else {
            panic!("Expected Split variant");
        }
    }

    #[test]
    fn test_dock_node_leaf_field_access() {
        let leaf = make_leaf(vec![10, 20, 30]);
        if let DockNode::Leaf { tabs, active } = &leaf {
            assert_eq!(*tabs, vec![10, 20, 30]);
            assert_eq!(*active, 0);
        } else {
            panic!("Expected Leaf variant");
        }
    }

    #[test]
    fn test_empty_distinct_from_empty_leaf() {
        let empty = DockNode::<u32>::Empty;
        let empty_leaf = DockNode::Leaf {
            tabs: vec![],
            active: 0,
        };
        assert_ne!(empty, empty_leaf);

        match (&empty, &empty_leaf) {
            (DockNode::Empty, DockNode::Leaf { .. }) => {}
            _ => panic!("Pattern matching should distinguish Empty from empty Leaf"),
        }
    }

    #[test]
    fn test_dock_tree_generic_u32() {
        let tree: DockTree<u32> = DockTree::new(make_leaf(vec![1, 2, 3]));
        assert!(matches!(tree.root(), DockNode::Leaf { .. }));
    }

    #[test]
    fn test_dock_tree_generic_string() {
        let tree: DockTree<String> = DockTree::new(DockNode::Leaf {
            tabs: vec!["tab1".into(), "tab2".into()],
            active: 0,
        });
        assert!(matches!(tree.root(), DockNode::Leaf { .. }));
    }

    #[test]
    fn test_dock_path_root() {
        let p = DockPath::root();
        assert!(p.is_root());
        assert!(p.as_slice().is_empty());
    }

    #[test]
    fn test_dock_path_child_and_parent() {
        let root = DockPath::root();
        let child0 = root.child(0);
        assert_eq!(child0.as_slice(), &[0]);
        assert_eq!(child0.parent(), Some(DockPath::root()));

        let child01 = child0.child(1);
        assert_eq!(child01.as_slice(), &[0, 1]);
        assert_eq!(child01.parent(), Some(DockPath(vec![0])));
        assert_eq!(child01.parent().unwrap().parent(), Some(DockPath::root()));
    }

    #[test]
    fn test_leaf_bounds_single_leaf() {
        let tree = DockTree::new(make_leaf(vec![1]));
        let area = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1920.0, 1080.0));
        let bounds = tree.leaf_bounds(area);
        assert_eq!(bounds.len(), 1);
        assert_eq!(bounds[0].0, DockPath::root());
        assert_rect_eq(bounds[0].1, (0.0, 0.0), (1920.0, 1080.0));
    }

    #[test]
    fn test_leaf_bounds_horizontal_split() {
        let tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        ));
        let area = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1920.0, 1080.0));
        let bounds = tree.leaf_bounds(area);
        assert_eq!(bounds.len(), 2);

        assert_eq!(bounds[0].0, path(&[0]));
        assert_rect_eq(bounds[0].1, (0.0, 0.0), (960.0, 1080.0));

        assert_eq!(bounds[1].0, path(&[1]));
        assert_rect_eq(bounds[1].1, (960.0, 0.0), (1920.0, 1080.0));
    }

    #[test]
    fn test_leaf_bounds_vertical_split() {
        let tree = DockTree::new(make_split(
            SplitDirection::Vertical,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        ));
        let area = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1920.0, 1080.0));
        let bounds = tree.leaf_bounds(area);
        assert_eq!(bounds.len(), 2);

        assert_eq!(bounds[0].0, path(&[0]));
        assert_rect_eq(bounds[0].1, (0.0, 0.0), (1920.0, 540.0));

        assert_eq!(bounds[1].0, path(&[1]));
        assert_rect_eq(bounds[1].1, (0.0, 540.0), (1920.0, 1080.0));
    }

    #[test]
    fn test_leaf_bounds_nested_splits() {
        let tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_split(
                SplitDirection::Vertical,
                0.5,
                make_leaf(vec![2]),
                make_leaf(vec![3]),
            ),
        ));
        let area = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1920.0, 1080.0));
        let bounds = tree.leaf_bounds(area);
        assert_eq!(bounds.len(), 3);

        assert_eq!(bounds[0].0, path(&[0]));
        assert_rect_eq(bounds[0].1, (0.0, 0.0), (960.0, 1080.0));

        assert_eq!(bounds[1].0, path(&[1, 0]));
        assert_rect_eq(bounds[1].1, (960.0, 0.0), (1920.0, 540.0));

        assert_eq!(bounds[2].0, path(&[1, 1]));
        assert_rect_eq(bounds[2].1, (960.0, 540.0), (1920.0, 1080.0));
    }

    #[test]
    fn test_leaf_bounds_includes_empty() {
        let tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            DockNode::Empty,
        ));
        let area = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1920.0, 1080.0));
        let bounds = tree.leaf_bounds(area);
        assert_eq!(bounds.len(), 2);
        assert_eq!(bounds[1].0, path(&[1]));
    }

    #[test]
    fn test_split_leaf() {
        let mut tree = DockTree::new(make_leaf(vec![1]));
        tree.split_leaf(&DockPath::root(), SplitDirection::Vertical, 0.5)
            .unwrap();

        let expected = make_split(
            SplitDirection::Vertical,
            0.5,
            make_leaf(vec![1]),
            DockNode::Empty,
        );
        assert_eq!(*tree.root(), expected);
    }

    #[test]
    fn test_split_leaf_clamps_ratio() {
        let mut tree = DockTree::new(make_leaf(vec![1]));
        tree.split_leaf(&DockPath::root(), SplitDirection::Horizontal, 1.5)
            .unwrap();

        if let DockNode::Split { ratio, .. } = tree.root() {
            assert!(approx_eq(*ratio, 1.0));
        } else {
            panic!("Expected Split");
        }
    }

    #[test]
    fn test_split_leaf_errors_on_non_leaf() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        ));
        assert_eq!(
            tree.split_leaf(&DockPath::root(), SplitDirection::Horizontal, 0.5),
            Err(DockError::NotALeaf)
        );
    }

    #[test]
    fn test_split_leaf_errors_on_empty() {
        let mut tree: DockTree<u32> = DockTree::new(DockNode::Empty);
        assert_eq!(
            tree.split_leaf(&DockPath::root(), SplitDirection::Horizontal, 0.5),
            Err(DockError::CannotSplitEmpty)
        );
    }

    #[test]
    fn test_remove_tab_leaves_leaf_with_remaining_tabs() {
        let mut tree = DockTree::new(make_leaf(vec![1, 2, 3]));
        tree.remove_tab(&DockPath::root(), &2).unwrap();
        assert_eq!(
            *tree.root(),
            DockNode::Leaf {
                tabs: vec![1, 3],
                active: 0,
            }
        );
    }

    #[test]
    fn test_remove_tab_adjusts_active_index() {
        let mut tree = DockTree::new(DockNode::Leaf {
            tabs: vec![1, 2, 3],
            active: 2,
        });
        tree.remove_tab(&DockPath::root(), &2).unwrap();
        assert_eq!(
            *tree.root(),
            DockNode::Leaf {
                tabs: vec![1, 3],
                active: 1,
            }
        );
    }

    #[test]
    fn test_remove_last_tab_collapses_to_empty_in_split() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2, 3]),
        ));
        tree.remove_tab(&path(&[0]), &1).unwrap();
        assert_eq!(
            *tree.root(),
            DockNode::Leaf {
                tabs: vec![2, 3],
                active: 0,
            }
        );
    }

    #[test]
    fn test_remove_tab_collapses_nested_empty() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_split(
                SplitDirection::Vertical,
                0.5,
                make_leaf(vec![1]),
                make_leaf(vec![2]),
            ),
            make_leaf(vec![3]),
        ));

        tree.remove_tab(&path(&[0, 0]), &1).unwrap();
        assert_eq!(
            *tree.root(),
            make_split(
                SplitDirection::Horizontal,
                0.5,
                make_leaf(vec![2]),
                make_leaf(vec![3]),
            )
        );

        tree.remove_tab(&path(&[0]), &2).unwrap();
        assert_eq!(
            *tree.root(),
            DockNode::Leaf {
                tabs: vec![3],
                active: 0,
            }
        );
    }

    #[test]
    fn test_remove_tab_not_found() {
        let mut tree = DockTree::new(make_leaf(vec![1, 2]));
        assert_eq!(
            tree.remove_tab(&DockPath::root(), &99),
            Err(DockError::TabNotFound)
        );
    }

    #[test]
    fn test_move_tab_center() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        ));
        tree.move_tab(&path(&[0]), &path(&[1]), DockZone::Center)
            .unwrap();
        assert_eq!(
            *tree.root(),
            DockNode::Leaf {
                tabs: vec![2, 1],
                active: 1,
            },
        );
    }

    #[test]
    fn test_move_tab_left_zone() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        ));
        tree.move_tab(&path(&[0]), &path(&[1]), DockZone::Left)
            .unwrap();

        if let DockNode::Split {
            direction,
            children,
            ..
        } = tree.root()
        {
            assert_eq!(*direction, SplitDirection::Horizontal);
            assert!(matches!(&*children[0], DockNode::Leaf { tabs, .. } if tabs == &[1]));
            assert!(matches!(&*children[1], DockNode::Leaf { tabs, .. } if tabs == &[2]));
        } else {
            panic!("Expected Split");
        }
    }

    #[test]
    fn test_move_tab_right_zone() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        ));
        tree.move_tab(&path(&[0]), &path(&[1]), DockZone::Right)
            .unwrap();

        if let DockNode::Split {
            direction,
            children,
            ..
        } = tree.root()
        {
            assert_eq!(*direction, SplitDirection::Horizontal);
            assert!(matches!(&*children[0], DockNode::Leaf { tabs, .. } if tabs == &[2]));
            assert!(matches!(&*children[1], DockNode::Leaf { tabs, .. } if tabs == &[1]));
        } else {
            panic!("Expected Split");
        }
    }

    #[test]
    fn test_move_tab_top_zone() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        ));
        tree.move_tab(&path(&[0]), &path(&[1]), DockZone::Top)
            .unwrap();

        if let DockNode::Split {
            direction,
            children,
            ..
        } = tree.root()
        {
            assert_eq!(*direction, SplitDirection::Vertical);
            assert!(matches!(&*children[0], DockNode::Leaf { tabs, .. } if tabs == &[1]));
            assert!(matches!(&*children[1], DockNode::Leaf { tabs, .. } if tabs == &[2]));
        } else {
            panic!("Expected Split");
        }
    }

    #[test]
    fn test_move_tab_bottom_zone() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        ));
        tree.move_tab(&path(&[0]), &path(&[1]), DockZone::Bottom)
            .unwrap();

        if let DockNode::Split {
            direction,
            children,
            ..
        } = tree.root()
        {
            assert_eq!(*direction, SplitDirection::Vertical);
            assert!(matches!(&*children[0], DockNode::Leaf { tabs, .. } if tabs == &[2]));
            assert!(matches!(&*children[1], DockNode::Leaf { tabs, .. } if tabs == &[1]));
        } else {
            panic!("Expected Split");
        }
    }

    #[test]
    fn test_set_ratio() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        ));
        tree.set_ratio(&DockPath::root(), 0.7).unwrap();

        let area = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1000.0, 1000.0));
        let bounds = tree.leaf_bounds(area);
        assert_eq!(bounds.len(), 2);

        assert_rect_eq(bounds[0].1, (0.0, 0.0), (700.0, 1000.0));
        assert_rect_eq(bounds[1].1, (700.0, 0.0), (1000.0, 1000.0));
    }

    #[test]
    fn test_set_ratio_clamps() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        ));
        tree.set_ratio(&DockPath::root(), -0.5).unwrap();

        if let DockNode::Split { ratio, .. } = tree.root() {
            assert!(approx_eq(*ratio, 0.0));
        }
    }

    #[test]
    fn test_set_ratio_errors_on_non_split() {
        let mut tree = DockTree::new(make_leaf(vec![1]));
        assert_eq!(
            tree.set_ratio(&DockPath::root(), 0.5),
            Err(DockError::InvalidPath)
        );
    }

    #[test]
    fn test_activate_tab() {
        let mut tree = DockTree::new(DockNode::Leaf {
            tabs: vec![10, 20, 30],
            active: 0,
        });
        tree.activate_tab(&DockPath::root(), &20).unwrap();

        if let DockNode::Leaf { active, .. } = tree.root() {
            assert_eq!(*active, 1);
        } else {
            panic!("Expected Leaf");
        }
    }

    #[test]
    fn test_activate_tab_not_found() {
        let mut tree = DockTree::new(make_leaf(vec![1, 2, 3]));
        assert_eq!(
            tree.activate_tab(&DockPath::root(), &99),
            Err(DockError::TabNotFound)
        );
    }

    #[test]
    fn test_find_leaf_with_tab() {
        let tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1, 2]),
            make_leaf(vec![3, 4]),
        ));
        assert_eq!(tree.find_leaf_with_tab(&1), Some(path(&[0])));
        assert_eq!(tree.find_leaf_with_tab(&3), Some(path(&[1])));
        assert_eq!(tree.find_leaf_with_tab(&99), None);
    }

    #[test]
    fn test_get_node_by_path() {
        let tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_split(
                SplitDirection::Vertical,
                0.5,
                make_leaf(vec![2]),
                make_leaf(vec![3]),
            ),
        ));

        assert!(matches!(tree.get(&path(&[])), Some(DockNode::Split { .. })));
        assert!(matches!(tree.get(&path(&[0])), Some(DockNode::Leaf { .. })));
        assert!(matches!(
            tree.get(&path(&[1])),
            Some(DockNode::Split { .. })
        ));
        assert!(matches!(
            tree.get(&path(&[1, 0])),
            Some(DockNode::Leaf { .. })
        ));
        assert!(matches!(
            tree.get(&path(&[1, 1])),
            Some(DockNode::Leaf { .. })
        ));
        assert!(matches!(tree.get(&path(&[2])), None));
    }

    #[test]
    fn test_dock_path_uniquely_addresses_nodes() {
        let tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_split(
                SplitDirection::Vertical,
                0.5,
                make_leaf(vec![2]),
                make_leaf(vec![3]),
            ),
        ));

        let area = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1920.0, 1080.0));
        let bounds = tree.leaf_bounds(area);

        let paths: Vec<&DockPath> = bounds.iter().map(|(p, _)| p).collect();
        let unique_count = paths.len();
        let unique_set: std::collections::HashSet<&DockPath> = paths.into_iter().collect();
        assert_eq!(unique_count, unique_set.len(), "All paths must be unique");
    }

    // VAL-CROSS-012: DockTree leaf_bounds is O(n) efficient
    #[test]
    fn test_leaf_bounds_efficiency_100_leaves() {
        use std::time::Instant;

        // Build a tree with 100 leaves (50 horizontal splits)
        let mut root = make_leaf(vec![0]);
        for i in 1..100 {
            let old = std::mem::replace(&mut root, DockNode::Empty);
            root = make_split(SplitDirection::Horizontal, 0.5, old, make_leaf(vec![i]));
        }
        let tree = DockTree::new(root);

        let area = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1920.0, 1080.0));

        let start = Instant::now();
        let bounds = tree.leaf_bounds(area);
        let elapsed = start.elapsed();

        assert_eq!(bounds.len(), 100, "should have 100 leaf bounds");
        assert!(
            elapsed.as_millis() < 1,
            "leaf_bounds for 100 leaves should be sub-millisecond, took {:?}",
            elapsed
        );

        // Verify all bounds are within the screen area
        for (_, rect) in &bounds {
            assert!(rect.min.x() >= 0.0);
            assert!(rect.min.y() >= 0.0);
            assert!(rect.max.x() <= 1920.0 + 1.0); // floating point tolerance
            assert!(rect.max.y() <= 1080.0 + 1.0);
        }
    }

    // VAL-CROSS-036: DockTree with single tab has no splitter
    #[test]
    fn test_single_tab_no_splitter() {
        let tree = DockTree::new(make_leaf(vec![1]));
        assert!(
            matches!(tree.root(), DockNode::Leaf { .. }),
            "single tab should be a Leaf, not a Split"
        );

        let area = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1920.0, 1080.0));
        let bounds = tree.leaf_bounds(area);
        assert_eq!(bounds.len(), 1, "single leaf should produce one bound");

        // Verify the leaf fills the entire space
        assert_rect_eq(bounds[0].1, (0.0, 0.0), (1920.0, 1080.0));
    }

    #[test]
    fn test_single_tab_after_collapse() {
        // Start with two tabs, remove one — should end as single leaf
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        ));
        tree.remove_tab(&path(&[0]), &1).unwrap();

        assert!(
            matches!(tree.root(), DockNode::Leaf { .. }),
            "after removing one of two tabs, tree should collapse to a Leaf"
        );

        // No splitter means one leaf bound covering full area
        let area = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1920.0, 1080.0));
        let bounds = tree.leaf_bounds(area);
        assert_eq!(bounds.len(), 1);
    }

    // VAL-CROSS-037: DockTree collapse produces valid tree
    #[test]
    fn test_collapse_produces_valid_tree_no_empty_splits() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2, 3]),
        ));

        // Remove the tab from the left leaf
        tree.remove_tab(&path(&[0]), &1).unwrap();

        // Should collapse to just the right leaf
        assert_eq!(
            *tree.root(),
            DockNode::Leaf {
                tabs: vec![2, 3],
                active: 0,
            }
        );

        // Verify it's a valid tree — no splits with Empty children
        fn assert_no_empty_splits(node: &DockNode<u32>) {
            match node {
                DockNode::Split { children, .. } => {
                    // Neither child should be Empty or empty Leaf
                    for child in children {
                        match &**child {
                            DockNode::Empty => {
                                panic!("collapsed tree should not have Empty children in splits")
                            }
                            DockNode::Leaf { tabs, .. } if tabs.is_empty() => {
                                panic!("collapsed tree should not have empty Leaf children")
                            }
                            _ => assert_no_empty_splits(child),
                        }
                    }
                }
                _ => {}
            }
        }
        assert_no_empty_splits(tree.root());
    }

    #[test]
    fn test_collapse_deeply_nested_produces_valid_tree() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_split(
                SplitDirection::Vertical,
                0.5,
                make_leaf(vec![1]),
                make_leaf(vec![2]),
            ),
            make_split(
                SplitDirection::Vertical,
                0.5,
                make_leaf(vec![3]),
                make_leaf(vec![4]),
            ),
        ));

        // Remove tab 1 from [0, 0] — left inner leaf collapses,
        // parent [0] becomes Leaf([2])
        tree.remove_tab(&path(&[0, 0]), &1).unwrap();

        // Now tree: Split(H, 0.5, [Leaf([2]), Split(V, 0.5, [Leaf([3]), Leaf([4])])])
        assert!(
            matches!(tree.get(&path(&[0])), Some(DockNode::Leaf { tabs, .. }) if tabs == &[2]),
            "after removing tab 1, left side should be Leaf([2])"
        );

        // Remove tab 2 from [0] — now root collapses to right side
        tree.remove_tab(&path(&[0]), &2).unwrap();

        // Root should now be the right split
        if let DockNode::Split { children, .. } = tree.root() {
            assert!(
                matches!(&*children[0], DockNode::Leaf { tabs, .. } if tabs == &[3]),
                "left child should be Leaf([3])"
            );
            assert!(
                matches!(&*children[1], DockNode::Leaf { tabs, .. } if tabs == &[4]),
                "right child should be Leaf([4])"
            );
        } else {
            panic!("Expected remaining Split with tabs 3 and 4");
        }
    }

    #[test]
    fn test_collapse_preserves_non_empty_siblings() {
        let mut tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1, 2]),
            make_leaf(vec![3]),
        ));

        // Remove just one tab from left — should NOT collapse (still has tabs)
        tree.remove_tab(&path(&[0]), &1).unwrap();

        if let DockNode::Split { children, .. } = tree.root() {
            assert!(
                matches!(&*children[0], DockNode::Leaf { tabs, .. } if tabs == &[2]),
                "left leaf should still exist with remaining tab"
            );
            assert!(
                matches!(&*children[1], DockNode::Leaf { tabs, .. } if tabs == &[3]),
                "right leaf should be unchanged"
            );
        } else {
            panic!("Expected Split — should not collapse when both sides have tabs");
        }
    }
}
