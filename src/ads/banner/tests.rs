use super::*;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(BannerPlugin);
    app.world_mut().resource_mut::<BannerIntent>().visible = true;
    app
}

fn requests(app: &mut App) -> (usize, usize) {
    (
        app.world_mut()
            .resource_mut::<Messages<ShowBanner>>()
            .drain()
            .count(),
        app.world_mut()
            .resource_mut::<Messages<HideBanner>>()
            .drain()
            .count(),
    )
}

#[test]
fn readiness_and_cancellation_do_not_wait_for_a_visible_banner() {
    let mut app = app();
    app.update();
    assert_eq!(requests(&mut app), (0, 0));
    app.world_mut().resource_mut::<AdmobState>().can_request_ads = true;
    app.update();
    assert_eq!(requests(&mut app), (1, 0));
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(requests(&mut app), (0, 0));
    app.world_mut().resource_mut::<BannerIntent>().visible = false;
    app.update();
    assert_eq!(requests(&mut app), (0, 1));
    app.update();
    assert_eq!(requests(&mut app), (0, 0));
}

#[test]
fn no_fill_retries_with_backoff_and_preserves_gameplay_frames() {
    let mut app = app();
    app.world_mut().resource_mut::<AdmobState>().can_request_ads = true;
    app.update();
    assert_eq!(requests(&mut app), (1, 0));
    for delay in [30, 60, 120, 120] {
        app.world_mut().write_message(AdLoadFailed {
            format: AdFormat::Banner,
            error: "no fill".into(),
        });
        app.update();
        assert_eq!(requests(&mut app), (0, 1));
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(delay - 1));
        app.update();
        assert_eq!(requests(&mut app), (0, 0));
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(1));
        app.update();
        assert_eq!(requests(&mut app), (1, 0));
    }
}

#[test]
fn background_cancels_loading_and_return_can_request_again() {
    let mut app = app();
    app.world_mut().resource_mut::<AdmobState>().can_request_ads = true;
    app.update();
    assert_eq!(requests(&mut app), (1, 0));
    app.world_mut()
        .write_message(WindowEvent::AppLifecycle(AppLifecycle::Suspended));
    app.update();
    assert_eq!(requests(&mut app), (0, 1));
    app.world_mut()
        .write_message(WindowEvent::AppLifecycle(AppLifecycle::Running));
    app.update();
    assert_eq!(requests(&mut app), (1, 0));
}

#[test]
fn ios_active_notification_recovers_focus_after_a_system_prompt() {
    for ios in [false, true] {
        let mut banner = Banner::default();
        banner.observe(
            &WindowEvent::WindowFocused(bevy::window::WindowFocused {
                window: Entity::PLACEHOLDER,
                focused: false,
            }),
            Duration::ZERO,
            ios,
        );
        banner.observe(
            &WindowEvent::AppLifecycle(AppLifecycle::Suspended),
            Duration::ZERO,
            ios,
        );
        assert!(!banner.focused && banner.suspended);
        banner.observe(
            &WindowEvent::AppLifecycle(AppLifecycle::WillResume),
            Duration::ZERO,
            ios,
        );
        assert!(!banner.focused);
        banner.observe(
            &WindowEvent::AppLifecycle(AppLifecycle::Running),
            Duration::ZERO,
            ios,
        );
        assert!(!banner.suspended);
        assert_eq!(banner.focused, ios);
    }
}

#[test]
fn a_missing_callback_has_a_bounded_load_deadline() {
    let mut app = app();
    app.world_mut().resource_mut::<AdmobState>().can_request_ads = true;
    app.update();
    requests(&mut app);
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(LOAD_WAIT);
    app.update();
    assert_eq!(requests(&mut app), (0, 1));
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(RETRY_MIN);
    app.update();
    assert_eq!(requests(&mut app), (1, 0));
}

#[test]
fn width_changes_coalesce_until_the_adaptive_size_settles() {
    let mut app = app();
    app.world_mut().resource_mut::<AdmobState>().can_request_ads = true;
    let resize = |width| {
        WindowEvent::WindowResized(bevy::window::WindowResized {
            window: Entity::PLACEHOLDER,
            width,
            height: 800.0,
        })
    };
    app.world_mut().write_message(resize(320.0));
    app.update();
    assert_eq!(requests(&mut app), (1, 0));
    app.world_mut().resource_mut::<AdmobState>().banner_visible = true;
    app.world_mut().write_message(resize(400.0));
    app.update();
    assert_eq!(requests(&mut app), (0, 0));
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_millis(200));
    app.world_mut().write_message(resize(450.0));
    app.update();
    assert_eq!(requests(&mut app), (0, 0));
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_millis(299));
    app.update();
    assert_eq!(requests(&mut app), (0, 0));
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_millis(1));
    app.update();
    assert_eq!(requests(&mut app), (1, 0));
}

#[test]
fn failure_and_resize_cancel_before_requesting_a_replacement() {
    let mut app = app();
    app.world_mut().resource_mut::<AdmobState>().can_request_ads = true;
    for width in [320.0, 400.0] {
        app.world_mut()
            .write_message(WindowEvent::WindowResized(bevy::window::WindowResized {
                window: Entity::PLACEHOLDER,
                width,
                height: 800.0,
            }));
        app.update();
        requests(&mut app);
    }
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_millis(300));
    app.world_mut().write_message(AdLoadFailed {
        format: AdFormat::Banner,
        error: "no fill".into(),
    });
    app.update();
    assert_eq!(requests(&mut app), (0, 1));
    app.update();
    assert_eq!(requests(&mut app), (1, 0));
}
