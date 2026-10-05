use crate::system::{
    ComponentAccess, OrderedSystem, ResourceAccess, SystemExecutionOrder, SystemKind,
};

/// Cached dependency batches. Ordering tiers are absolute barriers.
pub(crate) struct SystemScheduler {
    groups: Vec<Vec<usize>>,
}

#[derive(Debug)]
pub(crate) enum SchedulerError {
    DependencyCycle,
}
impl std::fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cycle detected in system dependency graph")
    }
}
impl std::error::Error for SchedulerError {}

struct Claims<'a> {
    index: usize,
    components: &'a [ComponentAccess],
    resources: &'a [ResourceAccess],
    order: SystemExecutionOrder,
    exclusive: bool,
}
impl SystemScheduler {
    pub fn from_systems(systems: &[OrderedSystem]) -> Self {
        let claims: Vec<_> = systems
            .iter()
            .enumerate()
            .map(|(index, system)| Claims {
                index,
                components: &system.access.components,
                resources: &system.access.resources,
                order: system.order,
                exclusive: matches!(system.system, SystemKind::Exclusive(_)),
            })
            .collect();
        Self::build_claims(&claims).expect("forward-only system dependencies are acyclic")
    }
    #[cfg(test)]
    pub fn build(
        systems: &[(usize, Vec<ComponentAccess>, Vec<ResourceAccess>)],
    ) -> Result<Self, SchedulerError> {
        let claims: Vec<_> = systems
            .iter()
            .map(|(index, components, resources)| Claims {
                index: *index,
                components,
                resources,
                order: SystemExecutionOrder::NORMAL,
                exclusive: false,
            })
            .collect();
        Self::build_claims(&claims)
    }
    fn build_claims(systems: &[Claims<'_>]) -> Result<Self, SchedulerError> {
        let n = systems.len();
        let mut dependencies = vec![Vec::new(); n];
        for i in 0..n {
            for j in (i + 1)..n {
                let a = &systems[i];
                let b = &systems[j];
                if a.order != b.order
                    || a.exclusive
                    || b.exclusive
                    || conflicts(a.components, b.components, a.resources, b.resources)
                {
                    dependencies[j].push(i);
                }
            }
        }
        let mut done = vec![false; n];
        let mut remaining = n;
        let mut groups = Vec::new();
        while remaining != 0 {
            let ready: Vec<_> = (0..n)
                .filter(|&i| !done[i] && dependencies[i].iter().all(|&d| done[d]))
                .collect();
            if ready.is_empty() {
                return Err(SchedulerError::DependencyCycle);
            }
            for &i in &ready {
                done[i] = true;
            }
            remaining -= ready.len();
            groups.push(ready.into_iter().map(|i| systems[i].index).collect());
        }
        Ok(Self { groups })
    }
    pub fn groups(&self) -> &[Vec<usize>] {
        &self.groups
    }
}

fn conflicts(
    comp_a: &[ComponentAccess],
    comp_b: &[ComponentAccess],
    res_a: &[ResourceAccess],
    res_b: &[ResourceAccess],
) -> bool {
    component_conflicts(comp_a, comp_b) || resource_conflicts(res_a, res_b)
}

fn component_conflicts(a: &[ComponentAccess], b: &[ComponentAccess]) -> bool {
    a.iter().any(|a| b.iter().any(|b| a.conflicts_with(*b)))
}

fn resource_conflicts(a: &[ResourceAccess], b: &[ResourceAccess]) -> bool {
    a.iter().any(|a| b.iter().any(|b| a.conflicts_with(*b)))
}

#[cfg(test)]
mod tests;
