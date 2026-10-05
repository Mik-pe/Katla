use std::any::TypeId;

use super::*;

fn make_systems(
    access: Vec<Vec<ComponentAccess>>,
) -> Vec<(usize, Vec<ComponentAccess>, Vec<ResourceAccess>)> {
    access
        .into_iter()
        .enumerate()
        .map(|(i, a)| (i, a, Vec::new()))
        .collect()
}

fn make_systems_with_resources(
    access: Vec<(Vec<ComponentAccess>, Vec<ResourceAccess>)>,
) -> Vec<(usize, Vec<ComponentAccess>, Vec<ResourceAccess>)> {
    access
        .into_iter()
        .enumerate()
        .map(|(i, (a, r))| (i, a, r))
        .collect()
}

mod components;
mod resources;
