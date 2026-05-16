//! STL (STereoLithography) file parser supporting both ASCII and binary formats.
//!
//! Binary STL: 80-byte header + 4-byte triangle count + N * 50-byte triangle records.
//! ASCII STL: `solid [name] ... facet normal ... outer loop ... vertex ... endloop ... endfacet ... endsolid`

use std::io::{self, BufRead};

use katla_math::Sphere;

/// A single triangle in an STL mesh.
#[derive(Debug, Clone, Copy)]
pub struct StlTriangle {
    /// Face normal vector.
    pub normal: [f32; 3],
    /// Triangle vertices in counter-clockwise order.
    pub vertices: [[f32; 3]; 3],
}

/// Parsed STL mesh data.
#[derive(Debug, Clone)]
pub struct StlMesh {
    pub triangles: Vec<StlTriangle>,
    pub bounds: Sphere,
}

/// Error type for STL parsing.
#[derive(Debug)]
pub enum StlError {
    Io(io::Error),
    InvalidBinaryData { reason: String },
    InvalidAsciiData { line: usize, reason: String },
}

impl std::fmt::Display for StlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::InvalidBinaryData { reason } => write!(f, "invalid binary STL: {}", reason),
            Self::InvalidAsciiData { line, reason } => {
                write!(f, "invalid ASCII STL at line {}: {}", line, reason)
            }
        }
    }
}

impl std::error::Error for StlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for StlError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Detect whether STL data is ASCII or binary.
///
/// ASCII files start with `solid` (case-insensitive) followed by a name or whitespace.
/// We also check that the file doesn't look like binary data masquerading as ASCII
/// by verifying the first line contains only printable ASCII characters.
fn is_ascii(data: &[u8]) -> bool {
    if !data.starts_with(b"solid") && !data.starts_with(b"SOLID") {
        return false;
    }

    // Some binary files start with "solid" in the header. Verify by checking
    // that the first line is valid ASCII text (no control chars except newline).
    let first_line_end = data.iter().position(|&b| b == b'\n').unwrap_or(data.len());
    let first_line = &data[..first_line_end];

    first_line.iter().all(|&b| b >= 0x20 || b == b'\t')
}

/// Maximum number of triangles allowed in a binary STL file.
/// Prevents memory exhaustion from malformed files with bogus triangle counts.
const MAX_TRIANGLE_COUNT: usize = 50_000_000;

/// Check if a float value is finite (not NaN or Infinity).
fn is_finite_f32(v: [f32; 3]) -> bool {
    v[0].is_finite() && v[1].is_finite() && v[2].is_finite()
}

impl StlMesh {
    /// Parse an STL file from raw bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, StlError> {
        if is_ascii(data) {
            Self::parse_ascii(data)
        } else {
            Self::parse_binary(data)
        }
    }

    /// Parse binary STL format.
    fn parse_binary(data: &[u8]) -> Result<Self, StlError> {
        if data.len() < 84 {
            return Err(StlError::InvalidBinaryData {
                reason: format!("file too short ({} bytes, need at least 84)", data.len()),
            });
        }

        let triangle_count = u32::from_le_bytes([data[80], data[81], data[82], data[83]]) as usize;

        if triangle_count > MAX_TRIANGLE_COUNT {
            return Err(StlError::InvalidBinaryData {
                reason: format!(
                    "triangle count {} exceeds maximum {}",
                    triangle_count, MAX_TRIANGLE_COUNT
                ),
            });
        }

        let expected_len = 84 + triangle_count * 50;

        if data.len() < expected_len {
            return Err(StlError::InvalidBinaryData {
                reason: format!(
                    "expected {} bytes for {} triangles, got {}",
                    expected_len,
                    triangle_count,
                    data.len()
                ),
            });
        }

        let mut triangles = Vec::with_capacity(triangle_count);

        for i in 0..triangle_count {
            let offset = 84 + i * 50;
            let chunk = &data[offset..offset + 50];

            let normal = [
                f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
                f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
                f32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]),
            ];

            let v0 = [
                f32::from_le_bytes([chunk[12], chunk[13], chunk[14], chunk[15]]),
                f32::from_le_bytes([chunk[16], chunk[17], chunk[18], chunk[19]]),
                f32::from_le_bytes([chunk[20], chunk[21], chunk[22], chunk[23]]),
            ];

            let v1 = [
                f32::from_le_bytes([chunk[24], chunk[25], chunk[26], chunk[27]]),
                f32::from_le_bytes([chunk[28], chunk[29], chunk[30], chunk[31]]),
                f32::from_le_bytes([chunk[32], chunk[33], chunk[34], chunk[35]]),
            ];

            let v2 = [
                f32::from_le_bytes([chunk[36], chunk[37], chunk[38], chunk[39]]),
                f32::from_le_bytes([chunk[40], chunk[41], chunk[42], chunk[43]]),
                f32::from_le_bytes([chunk[44], chunk[45], chunk[46], chunk[47]]),
            ];

            triangles.push(StlTriangle {
                normal,
                vertices: [v0, v1, v2],
            });
        }

        validate_triangles(&mut triangles);
        let bounds = compute_bounds(&triangles);
        Ok(Self { triangles, bounds })
    }

    /// Parse ASCII STL format.
    fn parse_ascii(data: &[u8]) -> Result<Self, StlError> {
        let reader = io::BufReader::new(data);
        let mut triangles = Vec::new();
        let mut normal = [0.0f32; 3];
        let mut vertices = [[0.0f32; 3]; 3];

        let mut line_num = 0usize;
        let mut vertex_idx = 0usize;
        let mut in_loop = false;

        for line_result in reader.lines() {
            let line = line_result?;
            line_num += 1;
            let trimmed = line.trim();

            // Skip empty lines and the solid/endsolid keywords
            if trimmed.is_empty() {
                continue;
            }

            let lower = trimmed.to_ascii_lowercase();

            if lower.starts_with("solid") || lower.starts_with("endsolid") {
                continue;
            }

            if lower.starts_with("facet normal") {
                in_loop = false;
                vertex_idx = 0;
                normal = parse_vec3_from_parts(&lower["facet normal".len()..].trim(), line_num)?;
                continue;
            }

            if lower == "endfacet" {
                if vertex_idx != 3 {
                    return Err(StlError::InvalidAsciiData {
                        line: line_num,
                        reason: format!("expected 3 vertices, got {}", vertex_idx),
                    });
                }
                triangles.push(StlTriangle { normal, vertices });
                in_loop = false;
                continue;
            }

            if lower == "outer loop" {
                in_loop = true;
                vertex_idx = 0;
                continue;
            }

            if lower == "endloop" {
                in_loop = false;
                continue;
            }

            if lower.starts_with("vertex") {
                if !in_loop {
                    return Err(StlError::InvalidAsciiData {
                        line: line_num,
                        reason: "vertex outside of outer loop".into(),
                    });
                }
                if vertex_idx >= 3 {
                    return Err(StlError::InvalidAsciiData {
                        line: line_num,
                        reason: "more than 3 vertices in facet".into(),
                    });
                }
                let v = parse_vec3_from_parts(&lower["vertex".len()..].trim(), line_num)?;
                vertices[vertex_idx] = v;
                vertex_idx += 1;
                continue;
            }

            // Unknown keyword inside facet - skip (some STL files have extra attributes)
        }

        validate_triangles(&mut triangles);
        let bounds = compute_bounds(&triangles);
        Ok(Self { triangles, bounds })
    }

    /// Build indexed vertex + index data suitable for GPU upload.
    ///
    /// Returns (positions, normals, indices) where each position/normal is `[f32; 3]`.
    /// Vertices are deduplicated by (position, normal) pair.
    ///
    /// Note: STL has no tangent or UV data. The caller is responsible for filling
    /// in default tangents and tex coords when constructing GPU vertex buffers.
    pub fn to_indexed_mesh(&self) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
        use std::collections::HashMap;

        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        let mut vertex_map: HashMap<[u32; 6], u32> = HashMap::new();

        for tri in &self.triangles {
            for v in &tri.vertices {
                let key = f32_pair_to_key(*v, tri.normal);
                let idx = match vertex_map.get(&key) {
                    Some(&i) => i,
                    None => {
                        let i = positions.len() as u32;
                        positions.push(*v);
                        normals.push(tri.normal);
                        vertex_map.insert(key, i);
                        i
                    }
                };
                indices.push(idx);
            }
        }

        (positions, normals, indices)
    }
}

fn f32_pair_to_key(a: [f32; 3], b: [f32; 3]) -> [u32; 6] {
    [
        a[0].to_bits(),
        a[1].to_bits(),
        a[2].to_bits(),
        b[0].to_bits(),
        b[1].to_bits(),
        b[2].to_bits(),
    ]
}

fn parse_vec3_from_parts(s: &str, line_num: usize) -> Result<[f32; 3], StlError> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(StlError::InvalidAsciiData {
            line: line_num,
            reason: format!("expected 3 float values, got {}", parts.len()),
        });
    }

    let x = parts[0]
        .parse::<f32>()
        .map_err(|e| StlError::InvalidAsciiData {
            line: line_num,
            reason: format!("invalid float '{}': {}", parts[0], e),
        })?;
    let y = parts[1]
        .parse::<f32>()
        .map_err(|e| StlError::InvalidAsciiData {
            line: line_num,
            reason: format!("invalid float '{}': {}", parts[1], e),
        })?;
    let z = parts[2]
        .parse::<f32>()
        .map_err(|e| StlError::InvalidAsciiData {
            line: line_num,
            reason: format!("invalid float '{}': {}", parts[2], e),
        })?;

    Ok([x, y, z])
}

fn validate_triangles(triangles: &mut Vec<StlTriangle>) {
    use log::warn;
    let original_len = triangles.len();
    triangles.retain(|tri| {
        if !is_finite_f32(tri.normal) {
            warn!("Dropping STL triangle with non-finite normal");
            return false;
        }
        for v in &tri.vertices {
            if !is_finite_f32(*v) {
                warn!("Dropping STL triangle with non-finite vertex");
                return false;
            }
        }
        true
    });
    let dropped = original_len - triangles.len();
    if dropped > 0 {
        warn!(
            "Dropped {}/{} STL triangles with non-finite values",
            dropped, original_len
        );
    }
}

fn compute_bounds(triangles: &[StlTriangle]) -> Sphere {
    if triangles.is_empty() {
        return Sphere::new(katla_math::Vec3::new(0.0, 0.0, 0.0), 0.0);
    }

    use katla_math::Vec3;

    let mut min_pos = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max_pos = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);

    for tri in triangles {
        for v in &tri.vertices {
            min_pos = Vec3::new(
                min_pos.x().min(v[0]),
                min_pos.y().min(v[1]),
                min_pos.z().min(v[2]),
            );
            max_pos = Vec3::new(
                max_pos.x().max(v[0]),
                max_pos.y().max(v[1]),
                max_pos.z().max(v[2]),
            );
        }
    }

    let center = Vec3::new(
        (min_pos.x() + max_pos.x()) * 0.5,
        (min_pos.y() + max_pos.y()) * 0.5,
        (min_pos.z() + max_pos.z()) * 0.5,
    );

    let radius = ((max_pos.x() - min_pos.x()).powi(2)
        + (max_pos.y() - min_pos.y()).powi(2)
        + (max_pos.z() - min_pos.z()).powi(2))
    .sqrt()
        * 0.5;

    Sphere::new(center, radius)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_binary_stl() {
        // Minimal binary STL: 80-byte header + 4-byte count + 1 triangle
        let mut data = vec![0u8; 84];
        // Triangle count = 1
        data[80..84].copy_from_slice(&1u32.to_le_bytes());

        // Normal (0, 0, 1)
        data.extend_from_slice(
            &[
                0.0f32.to_le_bytes(),
                0.0f32.to_le_bytes(),
                1.0f32.to_le_bytes(),
            ]
            .concat(),
        );
        // Vertex 0 (0, 0, 0)
        data.extend_from_slice(&[0.0f32.to_le_bytes(); 3].concat());
        // Vertex 1 (1, 0, 0)
        data.extend_from_slice(
            &[
                1.0f32.to_le_bytes(),
                0.0f32.to_le_bytes(),
                0.0f32.to_le_bytes(),
            ]
            .concat(),
        );
        // Vertex 2 (0, 1, 0)
        data.extend_from_slice(
            &[
                0.0f32.to_le_bytes(),
                1.0f32.to_le_bytes(),
                0.0f32.to_le_bytes(),
            ]
            .concat(),
        );
        // Attribute byte count
        data.extend_from_slice(&0u16.to_le_bytes());

        let mesh = StlMesh::from_bytes(&data).unwrap();
        assert_eq!(mesh.triangles.len(), 1);
        assert_eq!(mesh.triangles[0].normal, [0.0, 0.0, 1.0]);
        assert_eq!(mesh.triangles[0].vertices[0], [0.0, 0.0, 0.0]);
        assert_eq!(mesh.triangles[0].vertices[1], [1.0, 0.0, 0.0]);
        assert_eq!(mesh.triangles[0].vertices[2], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn test_parse_ascii_stl() {
        let ascii = b"solid test
  facet normal 0 0 1
    outer loop
      vertex 0 0 0
      vertex 1 0 0
      vertex 0 1 0
    endloop
  endfacet
endsolid test
";
        let mesh = StlMesh::from_bytes(ascii).unwrap();
        assert_eq!(mesh.triangles.len(), 1);
        assert_eq!(mesh.triangles[0].normal, [0.0, 0.0, 1.0]);
        assert_eq!(mesh.triangles[0].vertices[0], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_ascii_multiple_triangles() {
        let ascii = b"solid cube
  facet normal 0 0 -1
    outer loop
      vertex -1 -1 -1
      vertex 1 -1 -1
      vertex 1 1 -1
    endloop
  endfacet
  facet normal 0 0 -1
    outer loop
      vertex -1 -1 -1
      vertex 1 1 -1
      vertex -1 1 -1
    endloop
  endfacet
endsolid cube
";
        let mesh = StlMesh::from_bytes(ascii).unwrap();
        assert_eq!(mesh.triangles.len(), 2);
    }

    #[test]
    fn test_to_indexed_mesh_deduplicates() {
        let ascii = b"solid test
  facet normal 0 0 1
    outer loop
      vertex 0 0 0
      vertex 1 0 0
      vertex 0 1 0
    endloop
  endfacet
  facet normal 0 0 1
    outer loop
      vertex 0 0 0
      vertex 0 1 0
      vertex 1 1 0
    endloop
  endfacet
endsolid test
";
        let mesh = StlMesh::from_bytes(ascii).unwrap();
        let (positions, normals, indices) = mesh.to_indexed_mesh();

        // 4 unique (position, normal) pairs, 6 indices
        assert_eq!(positions.len(), 4);
        assert_eq!(normals.len(), 4);
        assert_eq!(indices.len(), 6);
    }

    #[test]
    fn test_binary_too_short() {
        let data = vec![0u8; 50];
        let result = StlMesh::from_bytes(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_ascii_invalid_vertex_count() {
        let ascii = b"solid test
  facet normal 0 0 1
    outer loop
      vertex 0 0 0
      vertex 1 0 0
    endloop
  endfacet
endsolid test
";
        let result = StlMesh::from_bytes(ascii);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_ascii_stl() {
        let ascii = b"solid empty\nendsolid empty\n";
        let mesh = StlMesh::from_bytes(ascii).unwrap();
        assert!(mesh.triangles.is_empty());
    }

    #[test]
    fn test_bounds() {
        let ascii = b"solid test
  facet normal 0 0 1
    outer loop
      vertex -1 -2 -3
      vertex 1 2 3
      vertex 0 0 0
    endloop
  endfacet
endsolid test
";
        let mesh = StlMesh::from_bytes(ascii).unwrap();
        let center = mesh.bounds.center;
        let radius = mesh.bounds.radius;

        assert!((center.x() - 0.0).abs() < 0.01);
        assert!((center.y() - 0.0).abs() < 0.01);
        assert!((center.z() - 0.0).abs() < 0.01);
        assert!(radius > 0.0);
    }

    #[test]
    fn test_case_insensitive_ascii_detection() {
        let ascii = b"SOLID Test\n  facet normal 0 0 1\n    outer loop\n      vertex 0 0 0\n      vertex 1 0 0\n      vertex 0 1 0\n    endloop\n  endfacet\nENDSOLID Test\n";
        let mesh = StlMesh::from_bytes(ascii).unwrap();
        assert_eq!(mesh.triangles.len(), 1);
    }

    #[test]
    fn test_binary_triangle_count_exceeds_limit() {
        let mut data = vec![0u8; 84];
        let bogus_count = (MAX_TRIANGLE_COUNT + 1) as u32;
        data[80..84].copy_from_slice(&bogus_count.to_le_bytes());
        let result = StlMesh::from_bytes(&data);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("exceeds maximum"));
    }

    #[test]
    fn test_binary_nan_vertices_dropped() {
        let mut data = vec![0u8; 84];
        data[80..84].copy_from_slice(&1u32.to_le_bytes());

        // Normal (0, 0, 1)
        data.extend_from_slice(
            &[
                0.0f32.to_le_bytes(),
                0.0f32.to_le_bytes(),
                1.0f32.to_le_bytes(),
            ]
            .concat(),
        );
        // Vertex 0 with NaN
        data.extend_from_slice(&[f32::NAN.to_le_bytes(); 3].concat());
        // Vertex 1 (1, 0, 0)
        data.extend_from_slice(
            &[
                1.0f32.to_le_bytes(),
                0.0f32.to_le_bytes(),
                0.0f32.to_le_bytes(),
            ]
            .concat(),
        );
        // Vertex 2 (0, 1, 0)
        data.extend_from_slice(
            &[
                0.0f32.to_le_bytes(),
                1.0f32.to_le_bytes(),
                0.0f32.to_le_bytes(),
            ]
            .concat(),
        );
        data.extend_from_slice(&0u16.to_le_bytes());

        let mesh = StlMesh::from_bytes(&data).unwrap();
        assert!(mesh.triangles.is_empty(), "NaN triangles should be dropped");
    }
}
