//! Desktop offscreen capture helpers (`capture` feature): render a game into an
//! image the tool allocates, with no OS window, for store screenshots and
//! recordings at any resolution.
//!
//! [`headless`] adjusts the plugin group, [`attach_image_target`] points a
//! camera at a new image, and [`fixed_clock`] makes frame timing repeatable.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_ios_toolkit::capture;
//!
//! let (width, height) = capture::parse_size("1320x2868").unwrap();
//! let mut app = App::new();
//! app.add_plugins(capture::headless(DefaultPlugins.build()));
//! let image = app
//!     .world_mut()
//!     .resource_mut::<Assets<Image>>()
//!     .add(capture::target_image(width, height));
//! // The game's own scale factor.
//! app.world_mut()
//!     .spawn((Camera2d, capture::image_target(image, 1.0)));
//! ```
//!
//! An image target is not eligible for Bevy's primary-window UI fallback, so
//! mark the capture camera `IsDefaultUiCamera` when it should draw the UI.

use std::time::Duration;

use bevy::{
    app::{PluginGroupBuilder, ScheduleRunnerPlugin},
    camera::{ImageRenderTarget, RenderTarget},
    ecs::system::EntityCommands,
    prelude::*,
    render::{
        RenderPlugin,
        pipelined_rendering::PipelinedRenderingPlugin,
        render_resource::{TextureFormat, TextureUsages},
    },
    time::TimeUpdateStrategy,
    window::WindowResolution,
    winit::WinitPlugin,
};

/// Configure `plugins` for windowless rendering: compile pipelines before the
/// frame that needs them, and remove the window backend and pipelined
/// rendering. Plugins the group lacks are skipped; `RenderPlugin` must be
/// present and is replaced with its defaults plus synchronous compilation.
pub fn headless(plugins: PluginGroupBuilder) -> PluginGroupBuilder {
    let mut plugins = plugins.set(RenderPlugin {
        synchronous_pipeline_compilation: true,
        ..default()
    });
    if plugins.enabled::<WinitPlugin>() {
        plugins = plugins.disable::<WinitPlugin>();
    }
    if plugins.enabled::<PipelinedRenderingPlugin>() {
        plugins = plugins.disable::<PipelinedRenderingPlugin>();
    }
    plugins
}

/// A `width` x `height` sRGB RGBA8 render target, in physical pixels, that can
/// be copied back to the CPU.
pub fn target_image(width: u32, height: u32) -> Image {
    let mut image = Image::new_target_texture(width, height, TextureFormat::Rgba8UnormSrgb, None);
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    image
}

/// A camera target rendering into `handle` at `scale_factor`, the image's
/// counterpart to a window's scale factor.
pub fn image_target(handle: Handle<Image>, scale_factor: f32) -> RenderTarget {
    RenderTarget::Image(ImageRenderTarget {
        handle,
        scale_factor,
    })
}

/// Allocate a `width` x `height` [`target_image`] in `images` and point
/// `camera` at it with [`image_target`] at `scale_factor`. Returns the image,
/// for reading the capture back.
///
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_ios_toolkit::capture;
///
/// #[derive(Component)]
/// struct MainCamera;
///
/// fn retarget(
///     mut commands: Commands,
///     mut images: ResMut<Assets<Image>>,
///     camera: Single<Entity, With<MainCamera>>,
/// ) {
///     let mut camera = commands.entity(*camera);
///     capture::attach_image_target(&mut camera, &mut images, 1320, 2868, 3.0);
/// }
/// ```
pub fn attach_image_target(
    camera: &mut EntityCommands,
    images: &mut Assets<Image>,
    width: u32,
    height: u32,
    scale_factor: f32,
) -> Handle<Image> {
    let handle = images.add(target_image(width, height));
    camera.insert(image_target(handle.clone(), scale_factor));
    handle
}

/// A window of `width` x `height` physical pixels at `scale_factor` pixels per
/// point, so the layout sees `width / scale_factor` by `height /
/// scale_factor` points: a store-size capture laid out like the device. A
/// scale that is not finite and positive is treated as 1. The scale always
/// overrides the backend's, which a windowless capture does not have.
///
/// ```
/// use bevy_ios_toolkit::capture;
///
/// let phone = capture::window_resolution(1320, 2868, 3.0);
/// assert_eq!((phone.width(), phone.height()), (440.0, 956.0));
/// assert_eq!(phone.physical_width(), 1320);
/// ```
pub fn window_resolution(width: u32, height: u32, scale_factor: f32) -> WindowResolution {
    let scale_factor = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    WindowResolution::new(width, height).with_scale_factor_override(scale_factor)
}

/// Advance time by exactly `1 / fps` seconds per app update, whatever the
/// wall clock does, and return that frame length. An `fps` of zero is treated
/// as one.
///
/// Raises `Time<Virtual>`'s maximum delta so low rates are not clamped, and
/// adds `ScheduleRunnerPlugin::run_loop` at the same period unless a runner is
/// already present (nothing drives the app without `WinitPlugin`).
pub fn fixed_clock(app: &mut App, fps: u32) -> Duration {
    let frame = Duration::from_secs_f64(1.0 / f64::from(fps.max(1)));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));
    let mut virtual_time = app.world_mut().get_resource_or_init::<Time<Virtual>>();
    let max_delta = virtual_time.max_delta().max(frame);
    virtual_time.set_max_delta(max_delta);
    if !app.is_plugin_added::<ScheduleRunnerPlugin>() {
        app.add_plugins(ScheduleRunnerPlugin::run_loop(frame));
    }
    frame
}

/// Parse `WIDTHxHEIGHT` (for example `1320x2868`) into non-zero dimensions.
/// The separator may be `x` or `X`, and whitespace around either number is
/// ignored.
pub fn parse_size(value: &str) -> Result<(u32, u32), String> {
    let Some((width, height)) = value.split_once(['x', 'X']) else {
        return Err("must look like 1320x2868".to_string());
    };
    let width = width
        .trim()
        .parse::<u32>()
        .map_err(|error| format!("invalid width: {error}"))?;
    let height = height
        .trim()
        .parse::<u32>()
        .map_err(|error| format!("invalid height: {error}"))?;
    if width == 0 || height == 0 {
        return Err("dimensions must be non-zero".to_string());
    }
    Ok((width, height))
}

/// Parse a finite number of at least zero.
pub fn parse_non_negative(value: &str) -> Result<f32, String> {
    let value = value
        .parse::<f32>()
        .map_err(|error| format!("must be a number: {error}"))?;
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err("must be a finite non-negative number".to_string())
    }
}

#[cfg(test)]
mod tests;
