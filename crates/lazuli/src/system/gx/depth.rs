use glam::{Vec3, Vec4};

use super::pix::Scissor;
use super::{Topology, Vertex, xform};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DepthPlane {
    pub coefficients: Vec3,
}

impl DepthPlane {
    fn from_triangle(
        clip: [Vec4; 3],
        viewport: &xform::Viewport,
        scissor: Scissor,
    ) -> Option<Self> {
        let distances = clip.map(|v| [v.x + v.w, v.w - v.x, v.y + v.w, v.w - v.y, v.z + v.w, -v.z]);
        if (0..6).any(|axis| distances.iter().all(|d| d[axis] < 0.0)) {
            return None;
        }

        let (offset_x, offset_y) = scissor.offset();
        let [a, b, c] = clip.map(|v| {
            let ndc = v.truncate() / v.w;
            Vec3::new(
                ndc.x * viewport.width * 0.5 + viewport.center_x - offset_x as f32,
                ndc.y * viewport.height * -0.5 + viewport.center_y - offset_y as f32,
                ndc.z * viewport.far_minus_near + viewport.far,
            )
        });

        let normal = (b - a).cross(c - a);
        if normal.z == 0.0 {
            return None;
        }
        
        let dx = -normal.x / normal.z;
        let dy = -normal.y / normal.z;
        
        let coefficients = Vec3::new(dx, dy, a.z - dx * a.x - dy * a.y);
        coefficients.is_finite().then_some(Self { coefficients })
    }

    pub(super) fn update(
        &mut self,
        topology: Topology,
        vertices: &[Vertex],
        xf: &xform::Interface,
        scissor: Scissor,
    ) {
        let count = match topology {
            Topology::QuadList => vertices.len() / 4 * 2,
            Topology::TriangleList => vertices.len() / 3,
            Topology::TriangleStrip | Topology::TriangleFan => vertices.len().saturating_sub(2),
            Topology::LineList | Topology::LineStrip | Topology::PointList => return,
        };

        let projection = xf.projection_matrix();
        for triangle in (0..count).rev() {
            let indices = match topology {
                Topology::QuadList => {
                    let base = triangle / 2 * 4;
                    if triangle % 2 == 0 {
                        [base, base + 1, base + 2]
                    } else {
                        [base, base + 2, base + 3]
                    }
                }
                Topology::TriangleList => [triangle * 3, triangle * 3 + 1, triangle * 3 + 2],
                Topology::TriangleStrip => [triangle, triangle + 1, triangle + 2],
                Topology::TriangleFan => [0, triangle + 1, triangle + 2],
                _ => unreachable!(),
            };
        
            let clip = indices.map(|i| {
                let vertex = &vertices[i];
                projection
                    * (xf.matrix(vertex.pos_norm_matrix.index()) * vertex.position.extend(1.0))
            });
        
            if let Some(plane) = Self::from_triangle(clip, &xf.internal.viewport, scissor) {
                *self = plane;
                break;
            }
        }
    }
}
