use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::tree::{DockNode, DockTree, SplitDirection};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum SerializableDockNode {
    Split {
        direction: SerializableSplitDirection,
        ratio: f32,
        children: [Box<SerializableDockNode>; 2],
    },
    Leaf {
        tabs: Vec<serde_json::Value>,
        active: usize,
    },
    Empty,
}

#[derive(Serialize, Deserialize, Clone)]
enum SerializableSplitDirection {
    Horizontal,
    Vertical,
}

impl From<SplitDirection> for SerializableSplitDirection {
    fn from(dir: SplitDirection) -> Self {
        match dir {
            SplitDirection::Horizontal => SerializableSplitDirection::Horizontal,
            SplitDirection::Vertical => SerializableSplitDirection::Vertical,
        }
    }
}

impl From<&SerializableSplitDirection> for SplitDirection {
    fn from(dir: &SerializableSplitDirection) -> Self {
        match dir {
            SerializableSplitDirection::Horizontal => SplitDirection::Horizontal,
            SerializableSplitDirection::Vertical => SplitDirection::Vertical,
        }
    }
}

fn node_to_serializable<T: Clone + PartialEq + Serialize>(
    node: &DockNode<T>,
) -> SerializableDockNode {
    match node {
        DockNode::Split {
            direction,
            ratio,
            children,
        } => SerializableDockNode::Split {
            direction: (*direction).into(),
            ratio: *ratio,
            children: [
                Box::new(node_to_serializable(&children[0])),
                Box::new(node_to_serializable(&children[1])),
            ],
        },
        DockNode::Leaf { tabs, active } => SerializableDockNode::Leaf {
            tabs: tabs
                .iter()
                .map(|t| serde_json::to_value(t).unwrap())
                .collect(),
            active: *active,
        },
        DockNode::Empty => SerializableDockNode::Empty,
    }
}

fn node_from_serializable<T: Clone + PartialEq + DeserializeOwned>(
    node: &SerializableDockNode,
) -> Result<DockNode<T>, serde_json::Error> {
    match node {
        SerializableDockNode::Split {
            direction,
            ratio,
            children,
        } => Ok(DockNode::Split {
            direction: SplitDirection::from(direction),
            ratio: *ratio,
            children: [
                Box::new(node_from_serializable(&children[0])?),
                Box::new(node_from_serializable(&children[1])?),
            ],
        }),
        SerializableDockNode::Leaf { tabs, active } => Ok(DockNode::Leaf {
            tabs: tabs
                .iter()
                .map(|v| serde_json::from_value(v.clone()))
                .collect::<Result<Vec<_>, _>>()?,
            active: *active,
        }),
        SerializableDockNode::Empty => Ok(DockNode::Empty),
    }
}

pub fn to_json<T: Clone + PartialEq + Serialize>(
    tree: &DockTree<T>,
) -> Result<String, serde_json::Error> {
    let serializable = node_to_serializable(tree.root());
    serde_json::to_string_pretty(&serializable)
}

pub fn from_json<T: Clone + PartialEq + DeserializeOwned>(
    json: &str,
) -> Result<DockTree<T>, serde_json::Error> {
    let serializable: SerializableDockNode = serde_json::from_str(json)?;
    Ok(DockTree::new(node_from_serializable(&serializable)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::tree::{DockPath, DockZone};

    fn make_leaf(tabs: Vec<u32>) -> DockNode<u32> {
        DockNode::Leaf { tabs, active: 0 }
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

    #[test]
    fn test_roundtrip_single_leaf() {
        let tree = DockTree::new(make_leaf(vec![1, 2, 3]));
        let json = to_json(&tree).unwrap();
        let restored = from_json::<u32>(&json).unwrap();
        assert_eq!(*tree.root(), *restored.root());
    }

    #[test]
    fn test_roundtrip_complex_tree() {
        let tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1, 2]),
            make_split(
                SplitDirection::Vertical,
                0.3,
                make_leaf(vec![3]),
                DockNode::Empty,
            ),
        ));
        let json = to_json(&tree).unwrap();
        let restored = from_json::<u32>(&json).unwrap();
        assert_eq!(*tree.root(), *restored.root());
    }

    #[test]
    fn test_roundtrip_empty() {
        let tree: DockTree<u32> = DockTree::new(DockNode::Empty);
        let json = to_json(&tree).unwrap();
        let restored = from_json::<u32>(&json).unwrap();
        assert_eq!(*tree.root(), *restored.root());
    }

    #[test]
    fn test_roundtrip_string_type() {
        let tree = DockTree::new(DockNode::Leaf {
            tabs: vec!["hierarchy".into(), "inspector".into()],
            active: 1,
        });
        let json = to_json(&tree).unwrap();
        let restored = from_json::<String>(&json).unwrap();
        assert_eq!(*tree.root(), *restored.root());
    }

    #[test]
    fn test_json_is_human_readable() {
        let tree = DockTree::new(make_split(
            SplitDirection::Horizontal,
            0.5,
            make_leaf(vec![1]),
            make_leaf(vec![2]),
        ));
        let json = to_json(&tree).unwrap();
        assert!(json.contains("Split"));
        assert!(json.contains("Horizontal"));
        assert!(json.contains("Leaf"));
    }

    #[test]
    fn test_roundtrip_preserves_active_index() {
        let tree = DockTree::new(DockNode::Leaf {
            tabs: vec![10, 20, 30],
            active: 2,
        });
        let json = to_json(&tree).unwrap();
        let restored = from_json::<u32>(&json).unwrap();
        if let DockNode::Leaf { tabs, active } = restored.root() {
            assert_eq!(*tabs, vec![10, 20, 30]);
            assert_eq!(*active, 2);
        } else {
            panic!("Expected Leaf");
        }
    }

    #[test]
    fn test_serialization_integration_with_operations() {
        let mut tree = DockTree::new(make_leaf(vec![1, 2, 3]));
        tree.split_leaf(&DockPath::root(), SplitDirection::Horizontal, 0.5)
            .unwrap();
        tree.move_tab(
            &DockPath::root().child(0),
            &DockPath::root().child(1),
            DockZone::Center,
        )
        .unwrap();

        let json = to_json(&tree).unwrap();
        let restored = from_json::<u32>(&json).unwrap();
        assert_eq!(*tree.root(), *restored.root());
    }
}
