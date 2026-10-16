//! Desktop offscreen capture helpers (`capture` feature): render a game into an
//! image the tool allocates, with no OS window, for store screenshots and
//! recordings at any resolution.
//!
//! - [`headless`] adjusts a plugin group: synchronous pipeline compilation, no
//!   `WinitPlugin`, no pipelined rendering, so readback and frame advancement
//!   share one app update.
//! - [`target_image`] and [`image_target`] build the camera's image target. The
//!   scale factor stays the game's choice; these helpers change no pixels.
//! - [`parse_size`] and [`parse_non_negative`] parse capture arguments.
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
//! // The game's own scale factor; add `IsDefaultUiCamera` when it draws the UI.
//! app.world_mut()
//!     .spawn((Camera2d, capture::image_target(image, 1.0)));
//! ```
//!
//! An image target is not eligible for Bevy's primary-window UI fallback, so
//! mark the capture camera `IsDefaultUiCamera` when it should draw the UI.

use bevy::{
    app::PluginGroupBuilder,
    camera::{ImageRenderTarget, RenderTarget},
    prelude::*,
    render::{
        RenderPlugin,
        pipelined_rendering::PipelinedRenderingPlugin,
        render_resource::{TextureFormat, TextureUsages},
    },
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

/// Parse `WIDTHxHEIGHT` (for example `1320x2868`) into non-zero dimensions.
pub fn parse_size(value: &str) -> Result<(u32, u32), String> {
    let Some((width, height)) = value.split_once('x') else {
        return Err("must look like 1320x2868".to_string());
    };
    let width = width
        .parse::<u32>()
        .map_err(|error| format!("invalid width: {error}"))?;
    let height = height
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
mod tests {
    use super::*;

    #[derive(Default)]
    struct Marker;

    impl Plugin for Marker {
        fn build(&self, _: &mut App) {}
    }

    struct Group;

    impl PluginGroup for Group {
        fn build(self) -> PluginGroupBuilder {
            PluginGroupBuilder::start::<Self>()
        }
    }

    #[test]
    fn headless_removes_the_window_backend_and_pipelined_rendering() {
        let group = PluginGroupBuilder::start::<Group>()
            .add(Marker)
            .add(RenderPlugin::default())
            .add(WinitPlugin::default())
            .add(PipelinedRenderingPlugin);
        let group = headless(group);
        assert!(group.enabled::<RenderPlugin>());
        assert!(group.enabled::<Marker>());
        assert!(!group.enabled::<WinitPlugin>());
        assert!(!group.enabled::<PipelinedRenderingPlugin>());
    }

    #[test]
    fn headless_tolerates_groups_without_optional_plugins() {
        let group = PluginGroupBuilder::start::<Group>().add(RenderPlugin::default());
        let group = headless(group);
        assert!(group.enabled::<RenderPlugin>());
        let group = headless(DefaultPlugins.build());
        assert!(!group.enabled::<WinitPlugin>());
    }

    #[test]
    fn image_targets_keep_native_pixels_and_the_given_scale() {
        let mut images = Assets::<Image>::default();
        let image = target_image(1320, 2868);
        assert_eq!(image.size(), UVec2::new(1320, 2868));
        assert_eq!(
            image.texture_descriptor.format,
            TextureFormat::Rgba8UnormSrgb
        );
        assert!(
            image
                .texture_descriptor
                .usage
                .contains(TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC)
        );
        let handle = images.add(image);
        for scale in [1.0, 3.0] {
            let RenderTarget::Image(target) = image_target(handle.clone(), scale) else {
                panic!("an image target");
            };
            assert_eq!(target.handle, handle);
            assert_eq!(target.scale_factor, scale);
        }
    }

    #[test]
    fn sizes_parse_width_by_height() {
        assert_eq!(parse_size("1320x2868"), Ok((1320, 2868)));
        assert_eq!(parse_size("390x844"), Ok((390, 844)));
        assert_eq!(
            parse_size("1320"),
            Err("must look like 1320x2868".to_string())
        );
        assert!(parse_size("0x844").is_err());
        assert!(parse_size("390x0").is_err());
        assert!(parse_size("-1x844").is_err());
        assert!(parse_size("390x844x2").is_err());
        assert!(parse_size(" 390x844").is_err());
    }

    #[test]
    fn non_negative_numbers_reject_negatives_and_non_finite_values() {
        assert_eq!(parse_non_negative("0"), Ok(0.0));
        assert_eq!(parse_non_negative("90.5"), Ok(90.5));
        for bad in ["-1", "NaN", "inf", "", "ten"] {
            assert!(parse_non_negative(bad).is_err(), "{bad}");
        }
    }
}
