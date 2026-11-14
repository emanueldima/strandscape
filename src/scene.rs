use {
    crate::{
        camera::{Camera, Ray},
        text::{TextLayout, TextSpec},
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

pub(crate) struct Hit {
    pub(crate) item: usize,
    pub(crate) point: Vec3,
}

pub(crate) enum Item {
    Plane(Plane),
    Dot(Dot),
    Sphere(Sphere),
    Tube(Tube),
    Text(Text),
}

#[derive(Clone, Copy)]
pub(crate) struct Transform {
    pub(crate) space: Space,
    pub(crate) translation: Vec3,
    pub(crate) rotation: Quat,
    pub(crate) scale: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Space {
    World,
    Hud,
}

pub(crate) struct Plane {
    pub(crate) transform: Transform,
    pub(crate) size: Vec2,
    pub(crate) color: [f32; 4],
}

pub(crate) struct Dot {
    pub(crate) transform: Transform,
    pub(crate) radius: f32,
    pub(crate) color: [f32; 4],
}

pub(crate) struct Sphere {
    pub(crate) center: Vec3,
    pub(crate) radius: f32,
    pub(crate) color: [f32; 4],
}

pub(crate) struct Tube {
    pub(crate) start: Vec3,
    pub(crate) end: Vec3,
    pub(crate) radius: f32,
    pub(crate) color: [f32; 4],
}

pub(crate) struct Text {
    pub(crate) transform: Transform,
    pub(crate) spec: TextSpec,
    pub(crate) color: [f32; 4],
    pub(crate) background: Option<[f32; 4]>,
    pub(crate) layout: Option<TextLayout>,
}

impl Scene {
    pub(crate) fn new(config: SceneConfig) -> Self {
        Self {
            camera: config.camera,
            items: config.items,
        }
    }

    pub(crate) fn pick(
        &self,
        point: Vec2,
        viewport: Vec2,
        eligible: impl Fn(usize) -> bool,
    ) -> Option<Hit> {
        if !point.is_finite() || !viewport.is_finite() || viewport.min_element() <= 0.0 {
            return None;
        }

        // HUD coordinates use CSS pixels from the top-left, with Y pointing up.
        let hud_ray = Ray {
            origin: Vec3::new(
                (point.x + 1.0) * viewport.x * 0.5,
                (point.y - 1.0) * viewport.y * 0.5,
                1.0,
            ),
            direction: -Vec3::Z,
        };
        let hud_hit = self
            .items
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, item)| {
                item.hit(&hud_ray, Space::Hud).map(|distance| Hit {
                    item: index,
                    point: hud_ray.origin + hud_ray.direction * distance,
                })
            });
        if hud_hit.is_some() {
            return hud_hit;
        }

        let ray = self.camera.ray(point, viewport.x / viewport.y)?;

        self.items
            .iter()
            .enumerate()
            // Ignore decorative world items before choosing the nearest surface.
            .filter(|(index, _)| eligible(*index))
            .filter_map(|(index, item)| {
                item.hit(&ray, Space::World)
                    .map(|distance| (index, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(item, distance)| Hit {
                item,
                point: ray.origin + ray.direction * distance,
            })
            .filter(|hit| hit.point.is_finite())
    }
}

impl Item {
    fn hit(&self, ray: &Ray, space: Space) -> Option<f32> {
        let (transform, min, max) = match self {
            Self::Tube(_) => return None,
            Self::Sphere(sphere) => {
                if space != Space::World || !sphere.radius.is_finite() || sphere.radius <= 0.0 {
                    return None;
                }

                // Find the nearest forward surface, including rays starting inside.
                let offset = ray.origin - sphere.center;
                let speed = ray.direction.length_squared();
                let along = -offset.dot(ray.direction) / speed;
                let closest = offset + ray.direction * along;
                let chord = (sphere.radius * sphere.radius - closest.length_squared()) / speed;
                if chord < 0.0 {
                    return None;
                }
                let near = along - chord.sqrt();
                let far = along + chord.sqrt();
                let distance = if near > 0.0 { near } else { far };
                return (distance.is_finite() && distance > 0.0).then_some(distance);
            }
            Self::Plane(plane) => (plane.transform, -plane.size * 0.5, plane.size * 0.5),
            Self::Dot(dot) => (
                dot.transform,
                Vec2::splat(-dot.radius),
                Vec2::splat(dot.radius),
            ),
            Self::Text(text) => {
                text.background?;
                let layout = text.layout.as_ref()?;
                if layout.bounds.contains(&0.0) {
                    return None;
                }
                let [min, max] = layout.pane_bounds().map(Vec2::from_array);
                (text.transform, min, max)
            }
        };
        if transform.space != space {
            return None;
        }

        // Preserve the ray parameter while testing the plane in its local bounds.
        let local_from_world = transform.matrix().inverse();
        let origin = local_from_world.transform_point3(ray.origin);
        let direction = local_from_world.transform_vector3(ray.direction);
        if direction.z.abs() <= f32::EPSILON {
            return None;
        }

        let distance = -origin.z / direction.z;
        if !distance.is_finite() || distance <= 0.0 {
            return None;
        }

        let point = (origin + direction * distance).truncate();
        let inside = match self {
            Self::Dot(dot) => point.length_squared() <= dot.radius * dot.radius,
            _ => point.cmpge(min).all() && point.cmple(max).all(),
        };
        inside.then_some(distance)
    }
}

impl Transform {
    pub(crate) fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    pub(crate) fn clip_from_local(self, clip_from_world: Mat4, viewport: Vec2) -> Mat4 {
        let projection = match self.space {
            Space::World => clip_from_world,
            // Depth zero keeps HUD text in front of world geometry.
            Space::Hud => Mat4::from_scale_rotation_translation(
                Vec3::new(2.0 / viewport.x, 2.0 / viewport.y, 0.0),
                Quat::IDENTITY,
                Vec3::new(-1.0, 1.0, 0.0),
            ),
        };
        projection * self.matrix()
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{camera::CameraConfig, text::FontId},
    };

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
            let hit = scene
                .pick(Vec2::new(0.1, 0.1), Vec2::new(1600.0, 1000.0), |_| true)
                .unwrap();
            assert!(hit.point.abs_diff_eq(Vec3::new(0.64, 0.4, 2.0), TOLERANCE));
            assert_eq!(depths[hit.item], 2.0);
        }
    }

    #[test]
    fn filters_world_candidates() {
        let scene = scene(vec![
            plane(Vec3::Z * 2.0, Quat::IDENTITY, Vec3::ONE),
            Item::Sphere(Sphere {
                center: Vec3::ZERO,
                radius: 1.0,
                color: [1.0; 4],
            }),
        ]);
        let viewport = Vec2::new(800.0, 600.0);
        assert_eq!(
            scene
                .pick(Vec2::ZERO, viewport, |index| index == 0)
                .unwrap()
                .item,
            0
        );
        assert_eq!(
            scene
                .pick(Vec2::ZERO, viewport, |index| index == 1)
                .unwrap()
                .item,
            1
        );
        assert!(scene.pick(Vec2::ZERO, viewport, |_| false).is_none());
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
        assert!(
            scene
                .pick(point, Vec2::new(1600.0, 1000.0), |_| true)
                .unwrap()
                .point
                .abs_diff_eq(target, TOLERANCE)
        );

        let outside = transform.transform_point3(Vec3::new(1.2, 0.0, 0.0));
        let point = projection.project_point3(outside).truncate();
        assert!(
            scene
                .pick(point, Vec2::new(1600.0, 1000.0), |_| true)
                .is_none()
        );
    }

    #[test]
    fn picks_spheres_from_any_angle() {
        let viewport = Vec2::new(800.0, 600.0);
        let mut scene = scene(vec![Item::Sphere(Sphere {
            center: Vec3::ZERO,
            radius: 1.0,
            color: [1.0; 4],
        })]);
        for orbit in [Vec2::ZERO, Vec2::new(0.5, 0.0), Vec2::new(0.25, 0.2)] {
            scene.camera.orbit(orbit);
            let ray = scene
                .camera
                .ray(Vec2::ZERO, viewport.x / viewport.y)
                .unwrap();
            let hit = scene.pick(Vec2::ZERO, viewport, |_| true).unwrap();
            assert_eq!(hit.item, 0);
            assert!((hit.point.length() - 1.0).abs() < TOLERANCE);
            assert!(
                (ray.origin.distance(hit.point) - (ray.origin.length() - 1.0)).abs() < TOLERANCE
            );
            assert!(scene.pick(Vec2::splat(0.9), viewport, |_| true).is_none());
        }
    }

    #[test]
    fn sphere_roots_and_occlusion() {
        let sphere = Item::Sphere(Sphere {
            center: Vec3::ZERO,
            radius: 1.0,
            color: [1.0; 4],
        });
        for (origin, distance) in [
            (Vec3::ZERO, Some(1.0)),
            (Vec3::new(-2.0, 1.0, 0.0), Some(2.0)),
            (Vec3::new(-2.0, 1.01, 0.0), None),
            (Vec3::new(2.0, 0.0, 0.0), None),
        ] {
            let ray = Ray {
                origin,
                direction: Vec3::X,
            };
            assert_eq!(sphere.hit(&ray, Space::World), distance);
            assert!(sphere.hit(&ray, Space::Hud).is_none());
        }

        // Pick the nearest surface, rather than the nearest center.
        let mut scene = scene(vec![
            sphere,
            Item::Sphere(Sphere {
                center: Vec3::Z * 0.5,
                radius: 0.2,
                color: [1.0; 4],
            }),
        ]);
        let viewport = Vec2::new(800.0, 600.0);
        assert_eq!(scene.pick(Vec2::ZERO, viewport, |_| true).unwrap().item, 0);
        scene
            .items
            .push(plane(Vec3::Z * 2.0, Quat::IDENTITY, Vec3::ONE));
        assert_eq!(scene.pick(Vec2::ZERO, viewport, |_| true).unwrap().item, 2);
    }

    #[test]
    fn picks_round_dot_bounds() {
        let viewport = Vec2::new(800.0, 600.0);
        for space in [Space::World, Space::Hud] {
            let transform = Transform {
                space,
                translation: if space == Space::Hud {
                    Vec3::new(120.0, -126.0, 0.0)
                } else {
                    Vec3::new(1.0, 0.4, 1.0)
                },
                rotation: Quat::from_rotation_z(0.3),
                scale: if space == Space::Hud {
                    Vec3::splat(20.0)
                } else {
                    Vec3::ONE
                },
            };
            let scene = scene(vec![Item::Dot(Dot {
                transform,
                radius: 1.0,
                color: [1.0; 4],
            })]);
            let clip = transform.clip_from_local(
                scene
                    .camera
                    .clip_from_world(viewport.x / viewport.y)
                    .unwrap(),
                viewport,
            );
            let inside = clip.project_point3(Vec3::new(0.8, 0.0, 0.0)).truncate();
            let corner = clip.project_point3(Vec3::new(0.8, 0.8, 0.0)).truncate();
            assert_eq!(scene.pick(inside, viewport, |_| true).unwrap().item, 0);
            assert!(scene.pick(corner, viewport, |_| true).is_none());
        }
    }

    #[test]
    fn picks_text_pane_bounds() {
        let viewport = Vec2::new(800.0, 600.0);
        for space in [Space::World, Space::Hud] {
            for background in [None, Some([0.0; 4]), Some([1.0; 4])] {
                let transform = Transform {
                    space,
                    translation: if space == Space::Hud {
                        Vec3::new(120.0, -126.0, 0.0)
                    } else {
                        Vec3::new(1.0, 0.4, 1.0)
                    },
                    rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
                    scale: if space == Space::Hud {
                        Vec3::new(1.3, 0.8, 1.0)
                    } else {
                        Vec3::new(0.02, 0.01, 1.0)
                    },
                };
                let mut scene = scene(vec![Item::Text(Text {
                    transform,
                    spec: TextSpec {
                        content: "An event".into(),
                        font: FontId(0),
                        size: 20.0,
                        max_width: None,
                    },
                    color: [1.0; 4],
                    background,
                    layout: Some(TextLayout {
                        glyphs: Vec::new().into(),
                        bounds: [100.0, 27.0],
                    }),
                })]);
                let clip = transform.clip_from_local(
                    scene
                        .camera
                        .clip_from_world(viewport.x / viewport.y)
                        .unwrap(),
                    viewport,
                );

                // Include pane padding while rejecting points beyond each edge.
                for (local, inside) in [
                    (Vec3::new(50.0, -13.5, 0.0), true),
                    (Vec3::new(-3.5, 3.5, 0.0), true),
                    (Vec3::new(103.5, -30.5, 0.0), true),
                    (Vec3::new(-4.5, -13.5, 0.0), false),
                    (Vec3::new(104.5, -13.5, 0.0), false),
                    (Vec3::new(50.0, 4.5, 0.0), false),
                    (Vec3::new(50.0, -31.5, 0.0), false),
                ] {
                    let point = clip.project_point3(local).truncate();
                    assert_eq!(
                        scene.pick(point, viewport, |_| true).is_some(),
                        inside && background.is_some()
                    );
                }

                let Item::Text(text) = &mut scene.items[0] else {
                    unreachable!()
                };
                text.layout = None;
                let point = clip.project_point3(Vec3::new(50.0, -13.5, 0.0)).truncate();
                assert!(scene.pick(point, viewport, |_| true).is_none());
            }
        }
    }

    #[test]
    fn misses_return_none() {
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
            assert!(
                scene
                    .pick(Vec2::ZERO, Vec2::splat(1000.0), |_| true)
                    .is_none()
            );
        }
        let scene = scene(vec![plane(
            Vec3::new(0.0, 0.0, 2.0),
            Quat::IDENTITY,
            Vec3::ONE,
        )]);
        assert!(
            scene
                .pick(Vec2::splat(0.9), Vec2::splat(1000.0), |_| true)
                .is_none()
        );
    }

    #[test]
    fn hud_keeps_screen_coordinates() {
        let transform = Transform {
            space: Space::Hud,
            translation: Vec3::new(24.0, -24.0, 0.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        };
        let mut scene = scene(vec![]);
        for viewport in [
            Vec2::new(800.0, 600.0),
            Vec2::new(1600.0, 900.0),
            Vec2::new(600.0, 800.0),
        ] {
            let aspect = viewport.x / viewport.y;
            let before =
                transform.clip_from_local(scene.camera.clip_from_world(aspect).unwrap(), viewport);
            scene.camera.orbit(Vec2::new(0.2, 0.1));
            scene.camera.pan(Vec2::new(0.1, -0.2));
            scene.camera.zoom(-250.0);
            scene.camera.focus(Vec3::new(3.0, 2.0, 1.0));
            let after =
                transform.clip_from_local(scene.camera.clip_from_world(aspect).unwrap(), viewport);
            assert_eq!(before, after);

            let point = after.project_point3(Vec3::new(12.0, -8.0, 0.0));
            let pixels = Vec2::new(
                (point.x + 1.0) * viewport.x * 0.5,
                (1.0 - point.y) * viewport.y * 0.5,
            );
            assert!(pixels.abs_diff_eq(Vec2::new(36.0, 32.0), TOLERANCE));
            assert_eq!(point.z, 0.0);
        }
    }

    #[test]
    fn picks_hud_before_world() {
        let mut hud = plane(Vec3::new(120.0, -126.0, 0.0), Quat::IDENTITY, Vec3::ONE);
        let Item::Plane(ref mut label) = hud else {
            unreachable!()
        };
        label.transform.space = Space::Hud;
        label.size = Vec2::new(200.0, 28.0);
        let mut scene = scene(vec![
            hud,
            plane(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(100.0)),
        ]);
        let viewport = Vec2::new(800.0, 600.0);
        let inside = Vec2::new(
            120.0 / viewport.x * 2.0 - 1.0,
            1.0 - 126.0 / viewport.y * 2.0,
        );
        let outside = Vec2::new(230.0 / viewport.x * 2.0 - 1.0, inside.y);
        assert_eq!(scene.pick(inside, viewport, |_| false).unwrap().item, 0);
        assert_eq!(scene.pick(outside, viewport, |_| true).unwrap().item, 1);

        scene.camera.orbit(Vec2::new(0.2, 0.1));
        scene.camera.pan(Vec2::new(0.1, -0.2));
        scene.camera.zoom(-250.0);
        scene.camera.focus(Vec3::new(3.0, 2.0, 1.0));
        let hit = scene.pick(inside, viewport, |_| true).unwrap();
        assert_eq!(hit.item, 0);
        assert!(
            hit.point
                .abs_diff_eq(Vec3::new(120.0, -126.0, 0.0), TOLERANCE)
        );
        assert!(scene.pick(inside, Vec2::ZERO, |_| true).is_none());
        assert!(scene.pick(Vec2::NAN, viewport, |_| true).is_none());
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
                space: Space::World,
                translation: position,
                rotation,
                scale,
            },
            size: Vec2::splat(2.0),
            color: [1.0; 4],
        })
    }
}
