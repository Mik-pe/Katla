use super::{LocalAction, LocalResponse};

/// Process a local (non-LLM) pattern-matching request.
///
/// Returns a response with the text to display and actions for the app to execute.
pub fn process_local_request(text: &str) -> LocalResponse {
    let lower = text.to_lowercase();

    if lower.contains("cube") || lower.contains("spawn") || lower.contains("create") {
        let count = extract_count(&lower);
        if count <= 1 {
            LocalResponse {
                text: "Spawned a cube at the origin.".to_string(),
                actions: vec![LocalAction::SpawnCube {
                    position: [0.0, 0.5, 0.0],
                    size: [1.0, 1.0, 1.0],
                }],
            }
        } else {
            let n = count.min(10);
            LocalResponse {
                text: format!("Spawned {} cubes in a ring formation.", n),
                actions: vec![LocalAction::SpawnCubeRing { count: n }],
            }
        }
    } else if lower.contains("sphere") {
        LocalResponse {
            text: "Spawned a sphere at the origin.".to_string(),
            actions: vec![LocalAction::SpawnSphere {
                position: [0.0, 0.7, 0.0],
                radius: 0.7,
            }],
        }
    } else if lower.contains("light") {
        LocalResponse {
            text: "Spawned a point light at (0, 3, 0).".to_string(),
            actions: vec![LocalAction::SpawnLight {
                position: [0.0, 3.0, 0.0],
            }],
        }
    } else if lower.contains("help") {
        LocalResponse {
            text: "I can help you build your scene! Try: 'spawn a cube', 'create 5 cubes', 'add a sphere', 'add a light'.\n\
                 \n\
                 To connect me to an AI, configure an LLM provider in Edit > Preferences > AI tab."
                .to_string(),
            actions: Vec::new(),
        }
    } else {
        LocalResponse {
            text: format!(
                "I understood: \"{}\". Try 'help' for available commands.\n\
             \n\
             For smarter responses, configure an LLM provider in Edit > Preferences > AI tab.",
                text
            ),
            actions: Vec::new(),
        }
    }
}

/// Extract a count from text like "spawn 5 cubes" or "create 3 entities".
fn extract_count(text: &str) -> usize {
    for word in text.split_whitespace() {
        if let Ok(n) = word.parse::<usize>() {
            return n;
        }
        match word {
            "one" => return 1,
            "two" => return 2,
            "three" => return 3,
            "four" => return 4,
            "five" => return 5,
            "six" => return 6,
            "seven" => return 7,
            "eight" => return 8,
            "nine" => return 9,
            "ten" => return 10,
            _ => {}
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_cube() {
        let resp = process_local_request("spawn a cube");
        assert!(resp.text.contains("Spawned"));
        assert_eq!(resp.actions.len(), 1);
        assert!(matches!(resp.actions[0], LocalAction::SpawnCube { .. }));
    }

    #[test]
    fn test_spawn_multiple_cubes() {
        let resp = process_local_request("create 5 cubes");
        assert!(resp.text.contains("5 cubes"));
        assert_eq!(resp.actions.len(), 1);
        assert!(matches!(
            resp.actions[0],
            LocalAction::SpawnCubeRing { count: 5 }
        ));
    }

    #[test]
    fn test_spawn_sphere() {
        let resp = process_local_request("add a sphere");
        assert!(resp.text.contains("sphere"));
        assert!(matches!(resp.actions[0], LocalAction::SpawnSphere { .. }));
    }

    #[test]
    fn test_spawn_light() {
        let resp = process_local_request("add a light");
        assert!(resp.text.contains("light"));
        assert!(matches!(resp.actions[0], LocalAction::SpawnLight { .. }));
    }

    #[test]
    fn test_help() {
        let resp = process_local_request("help");
        assert!(resp.text.contains("spawn a cube"));
        assert!(resp.actions.is_empty());
    }

    #[test]
    fn test_unknown() {
        let resp = process_local_request("foobar");
        assert!(resp.text.contains("foobar"));
        assert!(resp.actions.is_empty());
    }

    #[test]
    fn test_extract_count_numeric() {
        assert_eq!(extract_count("spawn 3 cubes"), 3);
        assert_eq!(extract_count("create 10 things"), 10);
        assert_eq!(extract_count("no number here"), 0);
    }

    #[test]
    fn test_extract_count_words() {
        assert_eq!(extract_count("spawn five cubes"), 5);
        assert_eq!(extract_count("create two spheres"), 2);
    }
}
