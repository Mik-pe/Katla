use super::{Health, Position, Tag, Velocity};

#[derive(Default)]
struct Table {
    ids: Vec<usize>,
    positions: Vec<Position>,
    velocities: Vec<Velocity>,
    health: Vec<Health>,
    cold: Vec<[u64; 32]>,
}

impl Table {
    fn push(&mut self, id: usize, p: Position, v: Velocity, h: Health) -> usize {
        let row = self.ids.len();
        self.ids.push(id);
        self.positions.push(p);
        self.velocities.push(v);
        self.health.push(h);
        row
    }

    fn remove(&mut self, row: usize) -> (usize, Position, Velocity, Health) {
        (
            self.ids.swap_remove(row),
            self.positions.swap_remove(row),
            self.velocities.swap_remove(row),
            self.health.swap_remove(row),
        )
    }
}

/// Benchmark-only fixed-schema SoA tables, with full row migration and location repair.
pub(super) struct Archetypes {
    tables: [Table; 2],
    locations: Vec<(usize, usize)>,
    tags: Vec<Tag>,
}

impl Archetypes {
    pub(super) fn new(count: usize, tag_stride: usize) -> Self {
        let mut value = Self {
            tables: [Table::default(), Table::default()],
            locations: Vec::with_capacity(count),
            tags: Vec::new(),
        };
        for id in 0..count {
            let table = usize::from(id % tag_stride == 0);
            let (p, v, h) = super::components(id);
            let row = value.tables[table].push(id, p, v, h);
            value.locations.push((table, row));
            if table == 1 {
                value.tags.push(Tag);
            }
        }
        value
    }

    pub(super) fn with_cold_rows(mut self) -> Self {
        for table in &mut self.tables {
            table.cold = table.ids.iter().map(|&id| [id as u64; 32]).collect();
        }
        self
    }

    pub(super) fn query2(&self) -> f64 {
        self.tables
            .iter()
            .map(|table| {
                table
                    .positions
                    .iter()
                    .zip(&table.velocities)
                    .map(|(p, v)| super::score2(p, v))
                    .sum::<f64>()
            })
            .sum()
    }

    pub(super) fn query4(&self) -> f64 {
        let table = &self.tables[1];
        table
            .positions
            .iter()
            .zip(&table.velocities)
            .zip(&table.health)
            .zip(&self.tags)
            .map(|(((p, v), h), _)| super::score2(p, v) + h.0 as f64)
            .sum()
    }

    fn migrate(&mut self, id: usize, destination: usize) {
        let (source, row) = self.locations[id];
        if source == destination {
            return;
        }
        let cold = if self.tables[source].cold.is_empty() {
            None
        } else {
            Some(self.tables[source].cold.swap_remove(row))
        };
        let (id, p, v, h) = self.tables[source].remove(row);
        if row < self.tables[source].ids.len() {
            self.locations[self.tables[source].ids[row]] = (source, row);
        }
        if source == 1 {
            self.tags.swap_remove(row);
        }
        let target_row = self.tables[destination].push(id, p, v, h);
        if destination == 1 {
            self.tags.push(Tag);
        }
        if let Some(cold) = cold {
            self.tables[destination].cold.push(cold);
        }
        self.locations[id] = (destination, target_row);
    }

    pub(super) fn validate(&self) {
        for (index, table) in self.tables.iter().enumerate() {
            assert_eq!(table.ids.len(), table.positions.len());
            assert_eq!(table.ids.len(), table.velocities.len());
            assert_eq!(table.ids.len(), table.health.len());
            if !table.cold.is_empty() {
                assert_eq!(table.ids.len(), table.cold.len());
                for (&id, cold) in table.ids.iter().zip(&table.cold) {
                    assert_eq!(*cold, [id as u64; 32]);
                }
            }
            for (row, &id) in table.ids.iter().enumerate() {
                assert_eq!(self.locations[id], (index, row));
            }
        }
        assert_eq!(self.tags.len(), self.tables[1].ids.len());
    }

    pub(super) fn churn(&mut self, ids: &[usize]) -> usize {
        for &id in ids {
            self.migrate(id, 1);
        }
        for &id in ids {
            self.migrate(id, 0);
        }
        std::hint::black_box(&self.tables);
        self.tables[0].ids.len()
    }
}
