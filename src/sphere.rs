//! Generates a UV sphere mesh.

/// Returns the vertex positions and triangle indices of a unit sphere centered on the origin.
///
/// Because the sphere has radius 1, each position is also that vertex's normal.
/// Triangles are wound counter-clockwise when seen from outside.
pub fn unit_sphere(segments: u16, rings: u16) -> (Vec<[f32; 3]>, Vec<u16>) {
    let mut positions = Vec::with_capacity((segments as usize + 1) * (rings as usize + 1));
    for ring in 0..=rings {
        // 0 at the north pole (+Y), PI at the south pole.
        let theta = ring as f32 / rings as f32 * std::f32::consts::PI;
        for segment in 0..=segments {
            let phi = segment as f32 / segments as f32 * std::f32::consts::TAU;
            positions.push([theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin()]);
        }
    }

    let mut indices = Vec::with_capacity(segments as usize * rings as usize * 6);
    let stride = segments + 1;
    for ring in 0..rings {
        for segment in 0..segments {
            let a = ring * stride + segment;
            let b = a + stride;
            indices.extend_from_slice(&[a, a + 1, b, b, a + 1, b + 1]);
        }
    }

    (positions, indices)
}
