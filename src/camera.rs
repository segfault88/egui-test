use eframe::egui;
use glam::{
    Mat4, Vec3,
    camera::rh::{proj::directx as proj, view},
};

/// A camera that orbits around the origin, always looking at it.
pub struct OrbitCamera {
    /// Rotation around the Y axis, in radians.
    pub yaw: f32,
    /// Angle above the horizon, in radians.
    pub pitch: f32,
    /// Distance from the origin.
    pub distance: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: 0.6,
            pitch: 0.35,
            distance: 9.0,
        }
    }
}

impl OrbitCamera {
    const MAX_PITCH: f32 = 1.5;
    const DISTANCE_RANGE: std::ops::RangeInclusive<f32> = 2.5..=30.0;

    pub fn position(&self) -> Vec3 {
        self.distance
            * Vec3::new(
                self.pitch.cos() * self.yaw.sin(),
                self.pitch.sin(),
                self.pitch.cos() * self.yaw.cos(),
            )
    }

    /// World space -> clip space. wgpu's clip space is the same as DirectX's: Y up, depth 0..1.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = view::look_at_mat4(self.position(), Vec3::ZERO, Vec3::Y);
        let proj = proj::perspective(45f32.to_radians(), aspect, 0.1, 100.0);
        proj * view
    }

    /// Drag to rotate, scroll or pinch to zoom.
    pub fn handle_input(&mut self, ui: &egui::Ui, response: &egui::Response) {
        let drag = response.drag_delta();
        self.yaw -= drag.x * 0.01;
        self.pitch = (self.pitch + drag.y * 0.01).clamp(-Self::MAX_PITCH, Self::MAX_PITCH);

        if response.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let zoom = (-scroll * 0.002).exp() / pinch;
            self.distance = (self.distance * zoom).clamp(*Self::DISTANCE_RANGE.start(), *Self::DISTANCE_RANGE.end());
        }
    }
}
