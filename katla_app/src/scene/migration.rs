use super::descriptors::Scene;
use log::info;

/// Trait for migrating scene data between format versions.
///
/// Each implementation handles a single version transition (e.g. v1 → v2).
/// Migrations are applied in order when a scene's version is below
/// [`SCENE_VERSION`](super::SCENE_VERSION).
pub trait SceneMigrator {
    /// The version this migration upgrades **from**.
    fn source_version(&self) -> u32;

    /// The version this migration upgrades **to**.
    fn target_version(&self) -> u32;

    /// Apply the migration to the given scene in place.
    fn migrate(&self, scene: &mut Scene);
}

/// Error returned when a scene's version is newer than what this build supports.
#[derive(Debug)]
pub struct SceneVersionTooNew {
    pub scene_version: u32,
    pub supported_version: u32,
}

impl std::fmt::Display for SceneVersionTooNew {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "scene version {} is newer than supported version {}; \
             save the scene from the newer engine or update Katla",
            self.scene_version, self.supported_version
        )
    }
}

impl std::error::Error for SceneVersionTooNew {}

/// Built-in v0 → v1 migration stub.
///
/// Currently a no-op placeholder. This handles scenes created before
/// the version field was introduced (version defaults to 0 when absent).
struct MigrateV0ToV1;

impl SceneMigrator for MigrateV0ToV1 {
    fn source_version(&self) -> u32 {
        0
    }

    fn target_version(&self) -> u32 {
        1
    }

    fn migrate(&self, _scene: &mut Scene) {
        // Stub: no data transformation needed yet.
    }
}

/// Built-in v1 → v2 migration stub.
///
/// Currently a no-op placeholder. When scene format fields are added
/// or renamed in a future version, this migration will transform the data.
struct MigrateV1ToV2;

impl SceneMigrator for MigrateV1ToV2 {
    fn source_version(&self) -> u32 {
        1
    }

    fn target_version(&self) -> u32 {
        2
    }

    fn migrate(&self, _scene: &mut Scene) {
        // Stub: no data transformation needed yet.
        // Future migrations will modify entity descriptors,
        // add new fields, or rename existing ones here.
    }
}

/// Build the ordered list of all known migrations.
///
/// Migrations must be sorted by `source_version` ascending. The migration
/// pipeline iterates this list and applies each matching migration in order.
fn built_in_migrations() -> Vec<Box<dyn SceneMigrator>> {
    vec![Box::new(MigrateV0ToV1), Box::new(MigrateV1ToV2)]
}

/// Run all applicable migrations on a scene.
///
/// If `scene.version < SCENE_VERSION`, migrations are applied in ascending
/// order until the scene reaches `SCENE_VERSION`. After migration, the scene's
/// version field is set to `SCENE_VERSION`.
///
/// Returns an error if `scene.version > SCENE_VERSION` (forward compatibility
/// is not supported — the scene was saved by a newer engine).
pub fn run_migrations(scene: &mut Scene) -> Result<(), SceneVersionTooNew> {
    use super::SCENE_VERSION;

    if scene.version > SCENE_VERSION {
        return Err(SceneVersionTooNew {
            scene_version: scene.version,
            supported_version: SCENE_VERSION,
        });
    }

    if scene.version >= SCENE_VERSION {
        return Ok(());
    }

    let migrations = built_in_migrations();

    let original_version = scene.version;
    for migration in &migrations {
        if scene.version >= SCENE_VERSION {
            break;
        }
        if migration.source_version() == scene.version {
            info!(
                "Applying scene migration v{} → v{}",
                migration.source_version(),
                migration.target_version()
            );
            migration.migrate(scene);
            scene.version = migration.target_version();
        }
    }

    if original_version < scene.version {
        info!(
            "Scene migrated from v{} to v{} (SCENE_VERSION={})",
            original_version, scene.version, SCENE_VERSION
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SCENE_VERSION;

    #[test]
    fn test_v1_v2_migration_stub() {
        let mut scene = Scene::new("Test");
        scene.version = 1;

        let migration = MigrateV1ToV2;
        assert_eq!(migration.source_version(), 1);
        assert_eq!(migration.target_version(), 2);
        migration.migrate(&mut scene);

        // Stub doesn't change scene data
        assert_eq!(scene.name, "Test");
        assert!(scene.entities.is_empty());
    }

    #[test]
    fn test_migration_runs_on_mismatch() {
        let mut scene = Scene::new("Old Scene");
        scene.version = 1;

        // Scene is at v1, which matches SCENE_VERSION=1, so no migration runs.
        let result = run_migrations(&mut scene);
        assert!(result.is_ok());
        assert_eq!(scene.version, SCENE_VERSION);
    }

    #[test]
    fn test_migration_preserves_data() {
        use crate::scene::descriptors::*;
        use crate::scene::entity_source::EntitySource;

        let mut scene = Scene::new("Data Preservation Test");
        scene.version = 1;

        scene.entities.push(EntityDescriptor {
            name: Some("Cube".to_string()),
            parent: None,
            transform: TransformDescriptor {
                position: [1.0, 2.0, 3.0],
                rotation: [0.0, 0.707, 0.0, 0.707],
                scale: [2.0, 3.0, 4.0],
            },
            source: EntitySource::Cube {
                size: [1.0, 1.0, 1.0],
            },
            drawable: Some(DrawableDescriptor {
                color: Some([0.8, 0.2, 0.1, 1.0]),
                metallic: 0.7,
                roughness: 0.3,
                ao: 0.8,
            }),
            point_light: None,
            particle_emitter: None,
            animation: None,
            velocity: None,
            script: None,
            mass: None,
            drag: None,
            perspective: None,
            directional_light: None,
        });

        scene.entities.push(EntityDescriptor {
            name: Some("Light1".to_string()),
            parent: Some("Cube".to_string()),
            transform: TransformDescriptor::default_transform(),
            source: EntitySource::Light,
            drawable: None,
            point_light: Some(PointLightDescriptor {
                color: [1.0, 0.5, 0.2],
                intensity: 20.0,
                range: 15.0,
            }),
            particle_emitter: None,
            animation: None,
            velocity: None,
            script: None,
            mass: None,
            drag: None,
            perspective: None,
            directional_light: None,
        });

        // Serialize the pre-migration state
        let original_names: Vec<_> = scene.entities.iter().map(|e| e.name.clone()).collect();
        let original_sources: Vec<_> = scene.entities.iter().map(|e| e.source.clone()).collect();
        let original_transforms: Vec<_> =
            scene.entities.iter().map(|e| e.transform.clone()).collect();
        let entity_count = scene.entities.len();

        // Run migration
        let result = run_migrations(&mut scene);
        assert!(result.is_ok());

        // Verify data is preserved
        assert_eq!(scene.entities.len(), entity_count);
        for (i, entity) in scene.entities.iter().enumerate() {
            assert_eq!(
                entity.name, original_names[i],
                "Name mismatch at index {}",
                i
            );
            assert_eq!(
                entity.source, original_sources[i],
                "Source mismatch at index {}",
                i
            );
            assert_eq!(
                entity.transform, original_transforms[i],
                "Transform mismatch at index {}",
                i
            );
        }
    }

    #[test]
    fn test_forward_compatibility_error() {
        let mut scene = Scene::new("Future Scene");
        // Simulate a scene saved by a newer engine (version 99)
        scene.version = 99;

        let result = run_migrations(&mut scene);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.scene_version, 99);
        assert_eq!(err.supported_version, SCENE_VERSION);

        // Verify the error message is descriptive
        let msg = format!("{}", err);
        assert!(
            msg.contains("99"),
            "Error message should mention scene version"
        );
        assert!(
            msg.contains("newer"),
            "Error message should explain the issue"
        );
    }

    #[test]
    fn test_forward_compatibility_no_panic() {
        let mut scene = Scene::new("Future Scene");
        scene.version = SCENE_VERSION + 1;

        // This should return an error, never panic
        let result = run_migrations(&mut scene);
        assert!(result.is_err());

        // Scene should be unmodified
        assert_eq!(scene.name, "Future Scene");
        assert_eq!(scene.version, SCENE_VERSION + 1);
    }

    #[test]
    fn test_migration_same_version_is_noop() {
        let mut scene = Scene::new("Current Version");
        scene.version = SCENE_VERSION;

        let result = run_migrations(&mut scene);
        assert!(result.is_ok());
        assert_eq!(scene.version, SCENE_VERSION);
        assert_eq!(scene.name, "Current Version");
    }

    #[test]
    fn test_migration_bumps_version() {
        // Simulate loading a v0 scene (e.g. a scene file without a version field).
        // The v0→v1 migration should fire, bumping version to SCENE_VERSION (1).
        let mut scene = Scene::new("V0 Scene");
        scene.version = 0;

        let result = run_migrations(&mut scene);
        assert!(result.is_ok());
        assert_eq!(
            scene.version, SCENE_VERSION,
            "Version should be bumped to SCENE_VERSION after migration"
        );
    }
}
