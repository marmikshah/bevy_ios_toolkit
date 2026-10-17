//! Render-world boot shield coordination; see the parent module docs.

use bevy::prelude::*;

use super::BootShieldReady;

/// Launch frames painted before the pipeline cache can report what it is
/// still waiting for.
pub const LAUNCH_PAINT_FRAMES: u32 = 8;
/// Window presents required, with every pipeline ready, before dismissal.
pub const WARM_PRESENTS: u32 = 2;
/// Window presents after which the shield drops even if pipelines still compile.
pub const MAX_PRESENTS: u32 = 12;

/// Keeps the native shield up until the renderer has presented real frames.
/// Add it with `DefaultPlugins`, in any order. Off iOS it only inserts a ready
/// [`BootShieldReady`].
pub struct BootShieldPlugin;

impl Plugin for BootShieldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BootShieldReady>();
    }

    // `finish` runs after every plugin has built, so `RenderApp` exists however
    // the game ordered its plugins.
    fn finish(&self, app: &mut App) {
        #[cfg(target_os = "ios")]
        {
            use bevy::render::{Render, RenderApp, RenderSystems, renderer::render_system};

            let ready = app.world().resource::<BootShieldReady>().clone();
            let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
                // Nothing would ever lower the shield, so lower it now.
                tracing::error!("BootShieldPlugin requires RenderPlugin; dismissing the shield");
                ready.mark_ready();
                super::first_frame_presented();
                return;
            };
            render_app.insert_resource(ready).add_systems(
                Render,
                (
                    paint_unwritten_drawables
                        .in_set(RenderSystems::Render)
                        .before(render_system),
                    dismiss_after_presents.in_set(RenderSystems::Cleanup),
                ),
            );
        }
        let _ = app;
    }
}

fn needs_paint(launch_frames: u32, pipelines_waiting: bool) -> bool {
    launch_frames < LAUNCH_PAINT_FRAMES || pipelines_waiting
}

#[derive(Default)]
struct Presents {
    count: u32,
    dismissed: bool,
}

impl Presents {
    /// Record one window present; true exactly once, when the shield drops.
    fn record(&mut self, warm: bool) -> bool {
        if self.dismissed {
            return false;
        }
        self.count += 1;
        self.dismissed = self.count >= WARM_PRESENTS && (warm || self.count >= MAX_PRESENTS);
        self.dismissed
    }
}

/// Bevy's final blit claims the drawable before it checks that its pipeline is
/// ready. When the pipeline is not ready it draws nothing, but the claim alone
/// marks the frame for presentation, and an unwritten Metal drawable shows as
/// fuchsia. Bevy paints over this on macOS only. Do the same here: for the
/// launch frames and while any pipeline is pending, fill the drawable with the
/// clear colour first. The blit clears again once it runs, so a ready frame is
/// unchanged.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn paint_unwritten_drawables(
    windows: Res<bevy::render::view::ExtractedWindows>,
    pipelines: Res<bevy::render::render_resource::PipelineCache>,
    clear: Res<ClearColor>,
    device: Res<bevy::render::renderer::RenderDevice>,
    queue: Res<bevy::render::renderer::RenderQueue>,
    mut launch_frames: Local<u32>,
) {
    use bevy::render::render_resource::{
        CommandEncoderDescriptor, LoadOp, Operations, RenderPassColorAttachment,
        RenderPassDescriptor, StoreOp,
    };

    if !needs_paint(
        *launch_frames,
        pipelines.waiting_pipelines().next().is_some(),
    ) {
        return;
    }
    let colour = clear.0.to_linear();
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    let mut painted = false;
    for window in windows.values() {
        let Some(view) = window.swap_chain_texture_view.as_ref() else {
            continue;
        };
        // Dropping the pass at the end of this statement records the clear.
        encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("unwritten_drawable_paint"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(colour.into()),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        painted = true;
    }
    if painted {
        *launch_frames = launch_frames.saturating_add(1);
        queue.submit([encoder.finish()]);
    }
}

/// Runs after frame submission. Main-world update ticks are not proof of
/// visible pixels; a window camera's present is.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn dismiss_after_presents(
    ready: Res<BootShieldReady>,
    views: Query<(
        &bevy::render::view::ViewTarget,
        &bevy::render::camera::ExtractedCamera,
    )>,
    pipelines: Res<bevy::render::render_resource::PipelineCache>,
    mut presents: Local<Presents>,
) {
    if presents.dismissed {
        return;
    }
    let presented = views.iter().any(|(target, camera)| {
        matches!(
            camera.target,
            Some(bevy::camera::NormalizedRenderTarget::Window(_))
        ) && target.needs_present()
    });
    if presented && presents.record(pipelines.waiting_pipelines().next().is_none()) {
        ready.mark_ready();
        super::first_frame_presented();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_frames_and_pending_pipelines_are_painted() {
        for frame in 0..LAUNCH_PAINT_FRAMES {
            assert!(needs_paint(frame, false), "{frame}");
        }
        assert!(!needs_paint(LAUNCH_PAINT_FRAMES, false));
        assert!(needs_paint(LAUNCH_PAINT_FRAMES, true));
        assert!(needs_paint(u32::MAX, true));
    }

    #[test]
    fn warm_pipelines_dismiss_on_the_second_present() {
        let mut presents = Presents::default();
        assert!(!presents.record(true));
        assert!(presents.record(true));
        assert!(!presents.record(true), "dismisses once");
    }

    #[test]
    fn cold_pipelines_wait_until_warm_or_the_present_cap() {
        let mut presents = Presents::default();
        for _ in 1..MAX_PRESENTS {
            assert!(!presents.record(false));
        }
        assert!(presents.record(false));

        let mut presents = Presents::default();
        for _ in 0..4 {
            assert!(!presents.record(false));
        }
        assert!(presents.record(true));
    }

    #[test]
    fn readiness_is_shared_between_clones() {
        let ready = BootShieldReady(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
            false,
        )));
        let render = ready.clone();
        render.mark_ready();
        assert!(ready.is_ready());
    }

    #[test]
    fn desktop_plugin_is_ready_without_a_renderer() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, BootShieldPlugin));
        app.finish();
        app.update();
        assert!(app.world().resource::<BootShieldReady>().is_ready());
    }
}
