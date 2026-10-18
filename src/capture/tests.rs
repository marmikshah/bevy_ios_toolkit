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
    assert!(parse_size("390xX844").is_err());
    assert!(parse_size("").is_err());
    for spaced in [
        " 390x844",
        "390x844\n",
        "390 x 844",
        "390X844",
        " 390 X 844 ",
    ] {
        assert_eq!(parse_size(spaced), Ok((390, 844)), "{spaced:?}");
    }
}

#[test]
fn window_resolutions_keep_pixels_and_divide_points_by_the_scale() {
    let phone = window_resolution(1320, 2868, 3.0);
    assert_eq!(
        (phone.physical_width(), phone.physical_height()),
        (1320, 2868)
    );
    assert_eq!((phone.width(), phone.height()), (440.0, 956.0));
    let tablet = window_resolution(2064, 2752, 2.0);
    assert_eq!((tablet.width(), tablet.height()), (1032.0, 1376.0));
    for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let resolution = window_resolution(390, 844, invalid);
        assert_eq!(resolution.scale_factor(), 1.0);
        assert_eq!((resolution.width(), resolution.height()), (390.0, 844.0));
    }
}

#[test]
fn attached_targets_render_into_a_new_image_at_the_scale() {
    use bevy::ecs::system::RunSystemOnce;

    let mut app = App::new();
    app.init_resource::<Assets<Image>>();
    let camera = app.world_mut().spawn(Camera::default()).id();
    let handle = app
        .world_mut()
        .run_system_once(
            move |mut commands: Commands, mut images: ResMut<Assets<Image>>| {
                let mut camera = commands.entity(camera);
                attach_image_target(&mut camera, &mut images, 1320, 2868, 3.0)
            },
        )
        .unwrap();
    let Some(RenderTarget::Image(target)) = app.world().get::<RenderTarget>(camera) else {
        panic!("an image target");
    };
    assert_eq!(target.handle, handle);
    assert_eq!(target.scale_factor, 3.0);
    let images = app.world().resource::<Assets<Image>>();
    assert_eq!(images.get(&handle).unwrap().size(), UVec2::new(1320, 2868));
}

#[test]
fn fixed_clocks_advance_one_frame_per_update() {
    for (fps, plugins_first) in [(60, false), (2, true), (0, false)] {
        let mut app = App::new();
        if plugins_first {
            app.add_plugins(bevy::time::TimePlugin);
        }
        let frame = fixed_clock(&mut app, fps);
        assert_eq!(frame, Duration::from_secs_f64(1.0 / f64::from(fps.max(1))));
        if !plugins_first {
            app.add_plugins(bevy::time::TimePlugin);
        }
        assert!(app.is_plugin_added::<ScheduleRunnerPlugin>());
        app.update();
        app.update();
        app.update();
        let time = app.world().resource::<Time<Virtual>>();
        // The first update starts the clock; each later one is one frame.
        assert_eq!(time.elapsed(), frame * 2, "{fps} fps");
        assert_eq!(time.delta(), frame);
    }
}

#[test]
fn fixed_clocks_keep_an_existing_runner() {
    let mut app = App::new();
    app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_millis(5)));
    fixed_clock(&mut app, 30);
    assert!(app.is_plugin_added::<ScheduleRunnerPlugin>());
}

#[test]
fn non_negative_numbers_reject_negatives_and_non_finite_values() {
    assert_eq!(parse_non_negative("0"), Ok(0.0));
    assert_eq!(parse_non_negative("90.5"), Ok(90.5));
    for bad in ["-1", "NaN", "inf", "", "ten"] {
        assert!(parse_non_negative(bad).is_err(), "{bad}");
    }
}
