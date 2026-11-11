mod fonts;

use {
    crate::{
        render::{RenderConfig, RenderError, Renderer, Target, Viewport},
        scene::{Camera, Item, Plane, Scene, SceneConfig, Text, Transform},
        text::{FontConfig, FontError, FontId, FontSystem, TextSpec},
    },
    glam::{Quat, Vec2, Vec3},
    std::{cell::RefCell, rc::Rc},
    thiserror::Error,
    wasm_bindgen::{JsCast, JsValue, closure::Closure},
    web_sys::{HtmlCanvasElement, Window},
};

pub(super) async fn run(canvas_id: &str, font_url: &str) -> Result<(), AppError> {
    let window = web_sys::window().ok_or_else(|| AppError::Browser("No browser window".into()))?;
    let canvas = window
        .document()
        .and_then(|document| document.get_element_by_id(canvas_id))
        .ok_or_else(|| AppError::Browser(format!("Canvas {canvas_id:?} not found")))?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| AppError::Browser("Expected a canvas element".into()))?;
    let bytes = fonts::fetch_font(font_url)
        .await
        .map_err(|error| AppError::FontLoad(error.to_string()))?;
    let fonts = FontSystem::new(FontConfig {
        fonts: vec![(FONT, bytes)],
    })?;
    let viewport = measure(&window, &canvas);
    canvas.set_width(viewport.width);
    canvas.set_height(viewport.height);

    // Text shares the plane's orientation, with a small offset to avoid z-fighting.
    let rotation = Quat::from_rotation_y(-0.18) * Quat::from_rotation_x(-0.08);
    let scene = Scene::new(SceneConfig {
        camera: Camera {
            transform: Transform {
                translation: Vec3::new(0.0, 0.0, 6.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            vertical_fov: 45.0_f32.to_radians(),
            near: 0.1,
            far: 100.0,
        },
        items: vec![
            Item::Plane(Plane {
                transform: Transform {
                    translation: Vec3::ZERO,
                    rotation,
                    scale: Vec3::ONE,
                },
                size: Vec2::new(4.5, 2.4),
                color: [0.035, 0.10, 0.13, 1.0],
            }),
            Item::Text(Text {
                transform: Transform {
                    translation: rotation * Vec3::new(-1.8, 0.65, 0.01),
                    rotation,
                    scale: Vec3::splat(0.009),
                },
                spec: TextSpec {
                    content: "Strandscape\nPeople, ideas, time and place.".into(),
                    font: FONT,
                    size: 32.0,
                    max_width: Some(400.0),
                },
                color: [0.91, 0.84, 0.66, 1.0],
            }),
        ],
    });
    let mut renderer = Renderer::new(
        Target::from(canvas.clone()),
        fonts,
        RenderConfig {
            viewport,
            clear_color: [0.012, 0.023, 0.033, 1.0],
        },
    )
    .await?;
    renderer.draw(&scene)?;

    let app = Rc::new(RefCell::new(App {
        scene,
        renderer,
        window: window.clone(),
        canvas: canvas.clone(),
        viewport,
        scheduled: false,
        _scale_query: None,
    }));
    let redraw: Redraw = Rc::new(Closure::new({
        let app = app.clone();
        move |_: f64| {
            let mut app = app.borrow_mut();
            app.scheduled = false;
            let viewport = measure(&app.window, &app.canvas);
            if viewport != app.viewport {
                app.canvas.set_width(viewport.width);
                app.canvas.set_height(viewport.height);
                app.renderer.resize(viewport);
                app.viewport = viewport;
            }
            let App {
                renderer, scene, ..
            } = &mut *app;
            if let Err(error) = renderer.draw(scene) {
                report_error(&error.to_string());
            }
        }
    }));

    // These callbacks keep the application alive for the browser page's lifetime.
    let on_resize = Closure::<dyn FnMut()>::new({
        let app = app.clone();
        let redraw = redraw.clone();
        move || request_redraw(&app, &redraw)
    });
    window.add_event_listener_with_callback("resize", on_resize.as_ref().unchecked_ref())?;
    on_resize.forget();

    let on_observe = Closure::<dyn FnMut()>::new({
        let app = app.clone();
        let redraw = redraw.clone();
        move || request_redraw(&app, &redraw)
    });
    let observer = web_sys::ResizeObserver::new(on_observe.as_ref().unchecked_ref())?;
    observer.observe(&canvas);
    on_observe.forget();
    std::mem::forget(observer);
    watch_scale(&app, &redraw)?;
    Ok(())
}

#[derive(Debug, Error)]
pub(super) enum AppError {
    #[error("{0}")]
    FontLoad(String),
    #[error(transparent)]
    Font(#[from] FontError),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error("{0}")]
    Browser(String),
}

impl From<JsValue> for AppError {
    fn from(value: JsValue) -> Self {
        Self::Browser(format!("{value:?}"))
    }
}

const FONT: FontId = FontId(0);

struct App {
    scene: Scene,
    renderer: Renderer,
    window: Window,
    canvas: HtmlCanvasElement,
    viewport: Viewport,
    scheduled: bool,
    _scale_query: Option<web_sys::MediaQueryList>,
}

type Redraw = Rc<Closure<dyn FnMut(f64)>>;

fn measure(window: &Window, canvas: &HtmlCanvasElement) -> Viewport {
    let rect = canvas.get_bounding_client_rect();
    let scale_factor = window.device_pixel_ratio();
    Viewport {
        width: (rect.width() * scale_factor).round() as u32,
        height: (rect.height() * scale_factor).round() as u32,
        scale_factor,
    }
}

fn request_redraw(app: &Rc<RefCell<App>>, redraw: &Redraw) {
    let mut app = app.borrow_mut();
    if app.scheduled {
        return;
    }
    match app
        .window
        .request_animation_frame(redraw.as_ref().as_ref().unchecked_ref())
    {
        Ok(_) => app.scheduled = true,
        Err(error) => report_error(&format!("Could not request redraw: {error:?}")),
    }
}

fn watch_scale(app: &Rc<RefCell<App>>, redraw: &Redraw) -> Result<(), AppError> {
    let window = app.borrow().window.clone();
    let query = window
        .match_media(&format!(
            "(resolution: {}dppx)",
            window.device_pixel_ratio()
        ))?
        .ok_or_else(|| AppError::Browser("Could not observe display scale".into()))?;
    let callback = Closure::once_into_js({
        let app = app.clone();
        let redraw = redraw.clone();
        move || {
            request_redraw(&app, &redraw);
            if let Err(error) = watch_scale(&app, &redraw) {
                report_error(&error.to_string());
            }
        }
    });
    let options = web_sys::AddEventListenerOptions::new();
    options.set_once(true);
    query.add_event_listener_with_callback_and_add_event_listener_options(
        "change",
        callback.unchecked_ref(),
        &options,
    )?;
    app.borrow_mut()._scale_query = Some(query);
    Ok(())
}

fn report_error(message: &str) {
    web_sys::console::error_1(&JsValue::from_str(message));
    if let Some(status) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("status"))
    {
        status.set_text_content(Some(message));
        let _ = status.remove_attribute("hidden");
    }
}
