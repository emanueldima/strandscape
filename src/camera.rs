use glam::{Mat4, Quat, Vec2, Vec3};

pub(crate) struct Camera {
    position: Vec3,
    focus: Vec3,
    rotation: Quat,
    vertical_fov: f32,
    near: f32,
    far: f32,
}

pub(crate) struct CameraConfig {
    pub(crate) position: Vec3,
    pub(crate) focus: Vec3,
    pub(crate) vertical_fov: f32,
    pub(crate) near: f32,
    pub(crate) far: f32,
}

pub(crate) struct Ray {
    pub(crate) origin: Vec3,
    pub(crate) direction: Vec3,
}

impl Camera {
    pub(crate) fn new(config: CameraConfig) -> Self {
        let mut camera = Self {
            position: config.position,
            focus: config.focus,
            rotation: Quat::IDENTITY,
            vertical_fov: config.vertical_fov,
            near: config.near,
            far: config.far,
        };
        camera.aim();
        camera
    }

    pub(crate) fn orbit(&mut self, delta: Vec2) {
        if !delta.is_finite() || delta == Vec2::ZERO {
            return;
        }

        let offset = self.position - self.focus;
        let distance = offset.length();
        let distance = if distance > f32::EPSILON {
            distance
        } else {
            MIN_DISTANCE
        };
        // Preserve heading at the pole, where tiny position errors make azimuth unstable.
        let right = self.rotation * Vec3::X;
        let yaw = (-right.z).atan2(right.x) + delta.x * ORBIT_SPEED;
        let pitch = ((offset.y / distance).clamp(-1.0, 1.0).asin() - delta.y * ORBIT_SPEED)
            .clamp(-MAX_PITCH, MAX_PITCH);

        // Orbit about a fixed focus; stop short of the poles to prevent flipping.
        self.position = self.focus
            + distance
                * Vec3::new(
                    yaw.sin() * pitch.cos(),
                    pitch.sin(),
                    yaw.cos() * pitch.cos(),
                );
        self.aim();
    }

    pub(crate) fn pan(&mut self, delta: Vec2) {
        if !delta.is_finite() || delta == Vec2::ZERO {
            return;
        }

        // Scale viewport-height deltas at the focus depth for consistent scrolling.
        let span = 2.0 * self.position.distance(self.focus) * (self.vertical_fov * 0.5).tan();
        let movement = self.rotation * Vec3::new(delta.x, -delta.y, 0.0) * span;
        self.position += movement;
        self.focus += movement;
    }

    pub(crate) fn focus(&mut self, point: Vec3) {
        // Recenter without changing the viewing angle or distance.
        self.position += point - self.focus;
        self.focus = point;
    }

    pub(crate) fn zoom(&mut self, scroll: f32) {
        if !scroll.is_finite() || scroll == 0.0 {
            return;
        }

        let offset = self.position - self.focus;
        let distance = offset.length();
        let direction = if distance > f32::EPSILON {
            offset / distance
        } else {
            self.rotation * Vec3::Z
        };
        let distance = (distance.max(MIN_DISTANCE) * (scroll * ZOOM_SPEED).exp())
            .clamp(MIN_DISTANCE, MAX_DISTANCE);
        self.position = self.focus + direction * distance;
    }

    pub(crate) fn ray(&self, point: Vec2, aspect: f32) -> Option<Ray> {
        if !point.is_finite() {
            return None;
        }

        let world_from_clip = self.clip_from_world(aspect)?.inverse();
        let near = world_from_clip.project_point3(point.extend(0.0));
        Some(Ray {
            origin: self.position,
            direction: (near - self.position).normalize(),
        })
    }

    pub(crate) fn clip_from_world(&self, aspect: f32) -> Option<Mat4> {
        if !aspect.is_finite()
            || aspect <= 0.0
            || !self.vertical_fov.is_finite()
            || self.vertical_fov <= 0.0
            || self.vertical_fov >= std::f32::consts::PI
            || !self.near.is_finite()
            || !self.far.is_finite()
            || self.near <= 0.0
            || self.far <= self.near
        {
            return None;
        }

        let projection = Mat4::perspective_rh(self.vertical_fov, aspect, self.near, self.far);
        let world_from_view = Mat4::from_rotation_translation(self.rotation, self.position);
        let matrix = projection * world_from_view.inverse();
        matrix.is_finite().then_some(matrix)
    }

    fn aim(&mut self) {
        let forward = self.focus - self.position;
        if forward.length_squared() <= f32::EPSILON {
            return;
        }

        // A vertical view uses map north as up to stay upright and invertible.
        let up = if forward.normalize().cross(Vec3::Y).length_squared() <= f32::EPSILON {
            -Vec3::Z
        } else {
            Vec3::Y
        };
        self.rotation =
            Quat::from_mat4(&Mat4::look_at_rh(self.position, self.focus, up).inverse()).normalize();
    }
}

const MIN_DISTANCE: f32 = 0.25;
const MAX_DISTANCE: f32 = 50.0;
const MAX_PITCH: f32 = 89.0_f32.to_radians();
const ORBIT_SPEED: f32 = std::f32::consts::PI;
const ZOOM_SPEED: f32 = 0.001;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_from_top_keeps_north() {
        for drift in [
            Vec2::ZERO,
            Vec2::new(0.0, -0.000001),
            Vec2::new(0.000001, 0.0),
        ] {
            for delta in [
                Vec2::new(0.001, 0.0),
                Vec2::new(-0.001, 0.0),
                Vec2::new(0.0, 0.001),
                Vec2::new(0.0, -0.001),
            ] {
                let mut camera = Camera::new(CameraConfig {
                    position: Vec3::new(drift.x, 6.0, drift.y),
                    focus: Vec3::ZERO,
                    vertical_fov: 45.0_f32.to_radians(),
                    near: 0.1,
                    far: 100.0,
                });
                let north = camera.rotation * Vec3::Y;

                // Tiny residual offsets near the pole must not change the initial heading.
                camera.orbit(delta);
                assert!(
                    north.dot(camera.rotation * Vec3::Y) > 0.99,
                    "north flipped for drift {drift:?} and drag {delta:?}"
                );
            }
        }
    }

    #[test]
    fn orbit_keeps_focus_and_distance() {
        let mut camera = camera();
        camera.focus(Vec3::new(1.0, 0.5, 0.0));
        let focus = camera.focus;
        let distance = camera.position.distance(focus);
        camera.orbit(Vec2::new(0.1, 0.05));

        assert_eq!(camera.focus, focus);
        assert!((camera.position.distance(focus) - distance).abs() < TOLERANCE);
        assert!(camera.position.x > 0.0);
        assert!(camera.position.y < 0.0);
        let ray = camera.ray(Vec2::ZERO, 1.0).unwrap();
        assert!(
            ray.direction
                .abs_diff_eq((focus - camera.position).normalize(), TOLERANCE)
        );
    }

    #[test]
    fn orbit_limits_elevation() {
        let mut camera = camera();
        for delta in [100.0, -100.0] {
            camera.orbit(Vec2::new(0.0, delta));
            assert!(camera.position.z > 0.0);
            assert!((camera.position.length() - 6.0).abs() < TOLERANCE);
            assert!(camera.clip_from_world(1.0).unwrap().is_finite());
        }
    }

    #[test]
    fn pan_moves_in_view_plane() {
        for orbit in [Vec2::ZERO, Vec2::new(0.2, 0.1)] {
            for scroll in [0.0, -500.0] {
                let mut camera = camera();
                camera.orbit(orbit);
                camera.zoom(scroll);
                let position = camera.position;
                let focus = camera.focus;
                let rotation = camera.rotation;
                let delta = Vec2::new(0.1, 0.2);
                camera.pan(delta);

                let movement = camera.position - position;
                let span = 2.0 * position.distance(focus) * (camera.vertical_fov * 0.5).tan();
                assert!(
                    movement.abs_diff_eq(rotation * Vec3::new(0.1, -0.2, 0.0) * span, TOLERANCE)
                );
                assert!(camera.focus.abs_diff_eq(focus + movement, TOLERANCE));
                assert_eq!(camera.rotation, rotation);

                for aspect in [0.5, 1.0, 2.0] {
                    let point = camera
                        .clip_from_world(aspect)
                        .unwrap()
                        .project_point3(focus);
                    assert!(
                        point
                            .truncate()
                            .abs_diff_eq(Vec2::new(-0.2 / aspect, 0.4), TOLERANCE)
                    );
                }
            }
        }
    }

    #[test]
    fn pan_ignores_invalid_input() {
        let mut camera = camera();
        let position = camera.position;
        let focus = camera.focus;
        for delta in [
            Vec2::ZERO,
            Vec2::new(f32::NAN, 0.0),
            Vec2::new(0.0, f32::INFINITY),
        ] {
            camera.pan(delta);
            assert_eq!(camera.position, position);
            assert_eq!(camera.focus, focus);
        }
    }

    #[test]
    fn focus_keeps_angle_and_distance() {
        let mut camera = camera();
        camera.orbit(Vec2::new(0.1, 0.05));
        camera.zoom(-100.0);
        let position = camera.position;
        let offset = position - camera.focus;
        let rotation = camera.rotation;
        for focus in [Vec3::X, position + Vec3::Y, position, Vec3::ZERO] {
            camera.focus(focus);
            assert_eq!(camera.focus, focus);
            assert_eq!(camera.rotation, rotation);
            assert!((camera.position - focus).abs_diff_eq(offset, TOLERANCE));
            let projected = camera.clip_from_world(1.0).unwrap().project_point3(focus);
            assert!(projected.truncate().abs_diff_eq(Vec2::ZERO, TOLERANCE));
        }
    }

    #[test]
    fn zoom_scales_distance_and_clamps() {
        let mut camera = camera();
        camera.orbit(Vec2::new(0.1, 0.05));
        let direction = (camera.position - camera.focus).normalize();
        camera.zoom(-100.0);
        assert!((camera.position.length() - 6.0 * (-0.1_f32).exp()).abs() < TOLERANCE);

        for (scroll, expected) in [(-100_000.0, MIN_DISTANCE), (100_000.0, MAX_DISTANCE)] {
            camera.zoom(scroll);
            assert!((camera.position.distance(camera.focus) - expected).abs() < TOLERANCE);
            assert!(
                (camera.position - camera.focus)
                    .normalize()
                    .abs_diff_eq(direction, TOLERANCE)
            );
            assert_eq!(camera.focus, Vec3::ZERO);
        }
    }

    #[test]
    fn zoom_handles_coincident_focus() {
        let mut camera = camera();
        camera.position = camera.focus;
        camera.zoom(100_000.0);
        assert!((camera.position.distance(camera.focus) - MAX_DISTANCE).abs() < TOLERANCE);
        assert!(camera.ray(Vec2::ZERO, 1.0).unwrap().direction.is_finite());
    }

    #[test]
    fn rays_match_projection() {
        let mut camera = camera();
        camera.orbit(Vec2::new(0.1, 0.05));
        camera.focus(Vec3::X);
        for aspect in [0.5, 1.0, 2.0] {
            let point = Vec2::new(0.4, -0.3);
            let ray = camera.ray(point, aspect).unwrap();
            let projected = camera
                .clip_from_world(aspect)
                .unwrap()
                .project_point3(ray.origin + ray.direction * 4.0);
            assert!(projected.truncate().abs_diff_eq(point, TOLERANCE));
        }
        assert!(camera.ray(Vec2::ZERO, 0.0).is_none());
    }

    const TOLERANCE: f32 = 0.0001;

    fn camera() -> Camera {
        Camera::new(CameraConfig {
            position: Vec3::new(0.0, 0.0, 6.0),
            focus: Vec3::ZERO,
            vertical_fov: 45.0_f32.to_radians(),
            near: 0.1,
            far: 100.0,
        })
    }
}
