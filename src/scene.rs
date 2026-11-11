use {
    crate::{
        camera::{Camera, Ray},
        text::TextSpec,
    },
    glam::{Mat4, Quat, Vec2, Vec3},
};

pub(crate) struct Scene {
    pub(crate) camera: Camera,
    pub(crate) items: Vec<Item>,
}

pub(crate) struct SceneConfig {
    pub(crate) camera: Camera,
    pub(crate) items: Vec<Item>,
}

pub(crate) enum Item {
    Plane(Plane),
    Text(Text),
}

#[derive(Clone, Copy)]
pub(crate) struct Transform {
    pub(crate) translation: Vec3,
    pub(crate) rotation: Quat,
    pub(crate) scale: Vec3,
}

pub(crate) struct Plane {
    pub(crate) transform: Transform,
    pub(crate) size: Vec2,
    pub(crate) color: [f32; 4],
}

pub(crate) struct Text {
    pub(crate) transform: Transform,
    pub(crate) spec: TextSpec,
    pub(crate) color: [f32; 4],
}

impl Scene {
    pub(crate) fn new(config: SceneConfig) -> Self {
        Self {
            camera: config.camera,
            items: config.items,
        }
    }

    pub(crate) fn pick(&self, point: Vec2, aspect: f32) -> Vec3 {
        let Some(ray) = self.camera.ray(point, aspect) else {
            return Vec3::ZERO;
        };

        self.items
            .iter()
            .filter_map(|item| match item {
                Item::Plane(plane) => plane.hit(&ray),
                Item::Text(_) => None,
            })
            .min_by(f32::total_cmp)
            .map_or(Vec3::ZERO, |distance| ray.origin + ray.direction * distance)
    }
}

impl Plane {
    fn hit(&self, ray: &Ray) -> Option<f32> {
        // Preserve the ray parameter while testing the plane in its local bounds.
        let local_from_world = self.transform.matrix().inverse();
        let origin = local_from_world.transform_point3(ray.origin);
        let direction = local_from_world.transform_vector3(ray.direction);
        if direction.z.abs() <= f32::EPSILON {
            return None;
        }

        let distance = -origin.z / direction.z;
        if !distance.is_finite() || distance <= 0.0 {
            return None;
        }

        let point = origin + direction * distance;
        point
            .truncate()
            .abs()
            .cmple(self.size * 0.5)
            .all()
            .then_some(distance)
    }
}

impl Transform {
    pub(crate) fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::camera::CameraConfig};

    #[test]
    fn picks_nearest_plane() {
        for depths in [[0.0, 2.0], [2.0, 0.0]] {
            let items = depths
                .into_iter()
                .map(|z| {
                    plane(
                        Vec3::new(0.0, 0.0, z),
                        Quat::IDENTITY,
                        Vec3::new(1.0, 1.0, if z == 0.0 { 100.0 } else { 0.1 }),
                    )
                })
                .collect();
            let scene = scene(items);
            let hit = scene.pick(Vec2::new(0.1, 0.1), 1.6);
            assert!(hit.abs_diff_eq(Vec3::new(0.64, 0.4, 2.0), TOLERANCE));
        }
    }

    #[test]
    fn picks_transformed_bounds() {
        let item = plane(
            Vec3::new(1.0, 0.4, 1.0),
            Quat::from_rotation_y(0.45) * Quat::from_rotation_x(0.25),
            Vec3::new(2.0, 0.75, 3.0),
        );
        let Item::Plane(ref plane) = item else {
            unreachable!()
        };
        let transform = plane.transform.matrix();
        let scene = scene(vec![item]);
        let projection = scene.camera.clip_from_world(1.6).unwrap();
        let target = transform.transform_point3(Vec3::new(0.4, -0.3, 0.0));
        let point = projection.project_point3(target).truncate();
        assert!(scene.pick(point, 1.6).abs_diff_eq(target, TOLERANCE));

        let outside = transform.transform_point3(Vec3::new(1.2, 0.0, 0.0));
        let point = projection.project_point3(outside).truncate();
        assert_eq!(scene.pick(point, 1.6), Vec3::ZERO);
    }

    #[test]
    fn misses_use_origin() {
        let scenes = [
            scene(vec![]),
            scene(vec![plane(
                Vec3::new(0.0, 0.0, 9.0),
                Quat::IDENTITY,
                Vec3::ONE,
            )]),
            scene(vec![plane(
                Vec3::ZERO,
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                Vec3::ONE,
            )]),
        ];
        for scene in scenes {
            assert_eq!(scene.pick(Vec2::ZERO, 1.0), Vec3::ZERO);
        }
        let scene = scene(vec![plane(
            Vec3::new(0.0, 0.0, 2.0),
            Quat::IDENTITY,
            Vec3::ONE,
        )]);
        assert_eq!(scene.pick(Vec2::splat(0.9), 1.0), Vec3::ZERO);
    }

    const TOLERANCE: f32 = 0.0001;

    fn scene(items: Vec<Item>) -> Scene {
        Scene::new(SceneConfig {
            camera: Camera::new(CameraConfig {
                position: Vec3::new(0.0, 0.0, 6.0),
                focus: Vec3::ZERO,
                vertical_fov: 90.0_f32.to_radians(),
                near: 0.1,
                far: 100.0,
            }),
            items,
        })
    }

    fn plane(position: Vec3, rotation: Quat, scale: Vec3) -> Item {
        Item::Plane(Plane {
            transform: Transform {
                translation: position,
                rotation,
                scale,
            },
            size: Vec2::splat(2.0),
            color: [1.0; 4],
        })
    }
}
