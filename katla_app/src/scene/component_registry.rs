//! Versioned application components with typed reference remapping.

use super::{CustomComponentDescriptor, SceneEntityId, SceneError};
use katla_ecs::{Component, EntityId, World};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    any::TypeId,
    collections::{BTreeMap, HashMap},
};

/// Maps live ECS entities to the keys written into this document.
pub struct SceneWriteContext {
    pub(crate) entities: HashMap<EntityId, SceneEntityId>,
}
impl SceneWriteContext {
    /// Encode an entity reference, rejecting references outside the saved scene.
    pub fn id(&self, entity: EntityId) -> Result<SceneEntityId, String> {
        self.entities
            .get(&entity)
            .copied()
            .ok_or_else(|| format!("Entity {entity} is outside this scene"))
    }
}

/// Maps persistent document keys to newly allocated live entities.
pub struct SceneReadContext {
    pub(crate) entities: HashMap<SceneEntityId, EntityId>,
}
impl SceneReadContext {
    /// Resolve a reference after all entities have been allocated.
    pub fn entity(&self, id: SceneEntityId) -> Result<EntityId, String> {
        self.entities
            .get(&id)
            .copied()
            .ok_or_else(|| format!("Scene entity {id} does not exist"))
    }
}

#[derive(katla_ecs::Component)]
pub(crate) struct SceneCustomData {
    #[inspect(skip)]
    pub(crate) components: BTreeMap<String, CustomComponentDescriptor>,
}

type Encoder = dyn Fn(&World, EntityId, &SceneWriteContext) -> Result<Option<String>, String>;
type PreparedComponent = Box<dyn FnOnce(&mut World)>;
type Decoder = dyn Fn(EntityId, &str, &SceneReadContext) -> Result<PreparedComponent, String>;
type Validator = dyn Fn(&str) -> Result<(), String>;

struct Entry {
    component_type: TypeId,
    version: u32,
    encode: Box<Encoder>,
    decode: Box<Decoder>,
    validate: Box<Validator>,
}

/// Application-owned codecs. Install them before loading scenes or entering play.
/// Unknown keys are retained as opaque RON payloads and are never silently dropped.
#[derive(Default)]
pub struct SceneComponentRegistry {
    entries: BTreeMap<String, Entry>,
}

impl SceneComponentRegistry {
    /// Register a serializable component that has no live ECS or native handles.
    /// Components with entity references must use `register_codec` and scene keys.
    pub fn register<T>(&mut self, key: impl Into<String>, version: u32) -> Result<(), SceneError>
    where
        T: Component + Serialize + DeserializeOwned,
    {
        self.install(
            key.into(),
            Entry {
                component_type: TypeId::of::<T>(),
                version,
                encode: Box::new(|world, entity, _| {
                    world
                        .get_component::<T>(entity)
                        .map(|component| {
                            ron::ser::to_string(component).map_err(|error| error.to_string())
                        })
                        .transpose()
                }),
                decode: Box::new(|entity, data, _| {
                    let component: T = ron::from_str(data).map_err(|error| error.to_string())?;
                    Ok(Box::new(move |world| {
                        world.add_component(entity, component);
                    }))
                }),
                validate: Box::new(|data| {
                    ron::from_str::<T>(data)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                }),
            },
        )
    }

    /// Register a wire DTO and map references explicitly in both directions.
    pub fn register_codec<T, D>(
        &mut self,
        key: impl Into<String>,
        version: u32,
        encode: impl Fn(&T, &SceneWriteContext) -> Result<D, String> + 'static,
        decode: impl Fn(D, &SceneReadContext) -> Result<T, String> + 'static,
    ) -> Result<(), SceneError>
    where
        T: Component,
        D: Serialize + DeserializeOwned + 'static,
    {
        self.install(
            key.into(),
            Entry {
                component_type: TypeId::of::<T>(),
                version,
                encode: Box::new(move |world, entity, context| {
                    world
                        .get_component::<T>(entity)
                        .map(|value| {
                            let data = encode(value, context)?;
                            ron::ser::to_string(&data).map_err(|error| error.to_string())
                        })
                        .transpose()
                }),
                decode: Box::new(move |entity, data, context| {
                    let data: D = ron::from_str(data).map_err(|error| error.to_string())?;
                    let component = decode(data, context)?;
                    Ok(Box::new(move |world| {
                        world.add_component(entity, component);
                    }))
                }),
                validate: Box::new(|data| {
                    ron::from_str::<D>(data)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                }),
            },
        )
    }

    fn install(&mut self, key: String, entry: Entry) -> Result<(), SceneError> {
        let error = |message: &str| SceneError::Component {
            key: key.clone(),
            message: message.into(),
        };
        if !valid_component_key(&key) {
            return Err(error("Use a namespaced key such as 'game.health'"));
        }
        if entry.version == 0 {
            return Err(error("Component versions start at 1"));
        }
        if self.entries.contains_key(&key) {
            return Err(error("Key is already registered"));
        }
        if self
            .entries
            .values()
            .any(|existing| existing.component_type == entry.component_type)
        {
            return Err(error("Component type is already registered"));
        }
        self.entries.insert(key, entry);
        Ok(())
    }

    pub(crate) fn capture(
        &self,
        world: &World,
        entity: EntityId,
        context: &SceneWriteContext,
    ) -> Result<BTreeMap<String, CustomComponentDescriptor>, SceneError> {
        let mut data = world
            .get_component::<SceneCustomData>(entity)
            .map(|value| value.components.clone())
            .unwrap_or_default();
        for (key, entry) in &self.entries {
            match (entry.encode)(world, entity, context).map_err(|message| {
                SceneError::Component {
                    key: key.clone(),
                    message,
                }
            })? {
                Some(payload) => {
                    data.insert(
                        key.clone(),
                        CustomComponentDescriptor {
                            version: entry.version,
                            data: payload,
                        },
                    );
                }
                None => {
                    data.remove(key);
                }
            }
        }
        Ok(data)
    }

    pub(crate) fn validate(
        &self,
        components: &BTreeMap<String, CustomComponentDescriptor>,
    ) -> Result<(), SceneError> {
        for (key, component) in components {
            if let Some(entry) = self.entries.get(key) {
                if entry.version != component.version {
                    return Err(SceneError::Component {
                        key: key.clone(),
                        message: format!(
                            "Version {} is required; the document has {}",
                            entry.version, component.version
                        ),
                    });
                }
                (entry.validate)(&component.data).map_err(|message| SceneError::Component {
                    key: key.clone(),
                    message,
                })?;
            }
        }
        Ok(())
    }

    pub(crate) fn restore(
        &self,
        world: &mut World,
        scene: &super::Scene,
        context: &SceneReadContext,
    ) -> Result<(), SceneError> {
        let mut prepared = Vec::new();
        let mut unknown = std::collections::BTreeSet::new();
        for desc in &scene.entities {
            let entity = context
                .entity(desc.id)
                .map_err(|message| SceneError::entity(desc.id, "id", message))?;
            for (key, data) in &desc.components {
                if let Some(entry) = self.entries.get(key) {
                    prepared.push((entry.decode)(entity, &data.data, context).map_err(
                        |message| SceneError::Component {
                            key: key.clone(),
                            message,
                        },
                    )?);
                } else {
                    unknown.insert(key);
                }
            }
        }
        for operation in prepared {
            operation(world);
        }
        for key in unknown {
            log::warn!("Scene component '{key}' has no installed codec; retaining its data");
        }
        Ok(())
    }
}

pub(crate) fn valid_component_key(key: &str) -> bool {
    key.contains('.')
        && key.split('.').all(|part| !part.is_empty())
        && !key.starts_with("katla.")
        && key.len() <= 128
        && key
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Component, Debug, PartialEq, Serialize, serde::Deserialize)]
    struct Health {
        current: u32,
    }
    #[derive(Component)]
    struct Target {
        #[inspect(skip)]
        entity: EntityId,
    }
    #[derive(Serialize, serde::Deserialize)]
    struct TargetData {
        entity: SceneEntityId,
    }

    #[test]
    fn test_registry_rejects_duplicate_keys_types_and_invalid_versions() {
        let mut registry = SceneComponentRegistry::default();
        registry.register::<Health>("game.health", 1).unwrap();
        assert!(registry.register::<Health>("game.health", 1).is_err());
        assert!(registry.register::<Health>("game.other", 1).is_err());
        for key in ["health", "katla.health", ".health", "game..health"] {
            assert!(
                SceneComponentRegistry::default()
                    .register::<Health>(key, 1)
                    .is_err()
            );
        }
        assert!(
            SceneComponentRegistry::default()
                .register::<Health>("game.health", 0)
                .is_err()
        );
    }

    #[test]
    fn test_reference_codec_remaps_keys_and_rejects_dangling_references() {
        let mut registry = SceneComponentRegistry::default();
        registry
            .register_codec::<Target, TargetData>(
                "game.target",
                1,
                |component, context| {
                    Ok(TargetData {
                        entity: context.id(component.entity)?,
                    })
                },
                |data, context| {
                    Ok(Target {
                        entity: context.entity(data.entity)?,
                    })
                },
            )
            .unwrap();
        let mut world = World::new();
        let a = world.create_entity();
        let b = world.create_entity();
        world.add_component(a, Target { entity: b });
        let write = SceneWriteContext {
            entities: HashMap::from([(a, SceneEntityId(10)), (b, SceneEntityId(20))]),
        };
        let data = registry.capture(&world, a, &write).unwrap();
        let mut desc = super::super::EntityDescriptor::new(
            SceneEntityId(10),
            super::super::EntitySource::Empty,
        );
        desc.components = data;
        let mut scene = super::super::Scene::new("Refs");
        scene.next_entity_id = 21;
        scene.entities.push(desc);
        let new_a = world.create_entity();
        let new_b = world.create_entity();
        let read = SceneReadContext {
            entities: HashMap::from([(SceneEntityId(10), new_a), (SceneEntityId(20), new_b)]),
        };
        registry.restore(&mut world, &scene, &read).unwrap();
        assert_eq!(world.get_component::<Target>(new_a).unwrap().entity, new_b);
        let outside = world.create_entity();
        world.get_component_mut::<Target>(a).unwrap().entity = outside;
        assert!(registry.capture(&world, a, &write).is_err());
        let missing = SceneReadContext {
            entities: HashMap::from([(SceneEntityId(10), new_a)]),
        };
        assert!(registry.restore(&mut world, &scene, &missing).is_err());
    }

    #[test]
    fn test_unknown_data_is_preserved_and_known_removed_components_are_not_resurrected() {
        let mut registry = SceneComponentRegistry::default();
        registry.register::<Health>("game.health", 1).unwrap();
        let mut world = World::new();
        let entity = world.create_entity();
        let unknown = CustomComponentDescriptor {
            version: 7,
            data: "FutureVariant(value: 99)".into(),
        };
        world.add_component(
            entity,
            SceneCustomData {
                components: BTreeMap::from([
                    ("dlc.future".into(), unknown.clone()),
                    (
                        "game.health".into(),
                        CustomComponentDescriptor {
                            version: 1,
                            data: "(current:99)".into(),
                        },
                    ),
                ]),
            },
        );
        let context = SceneWriteContext {
            entities: HashMap::from([(entity, SceneEntityId(1))]),
        };
        let captured = registry.capture(&world, entity, &context).unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured["dlc.future"], unknown);
        world.add_component(entity, Health { current: 42 });
        let captured = registry.capture(&world, entity, &context).unwrap();
        assert_eq!(
            ron::from_str::<Health>(&captured["game.health"].data).unwrap(),
            Health { current: 42 }
        );
        registry.validate(&captured).unwrap();
        let mut incompatible = captured;
        incompatible.get_mut("game.health").unwrap().version = 2;
        assert!(registry.validate(&incompatible).is_err());
        incompatible.get_mut("game.health").unwrap().version = 1;
        incompatible.get_mut("game.health").unwrap().data = "(current: invalid)".into();
        assert!(registry.validate(&incompatible).is_err());
    }
}
