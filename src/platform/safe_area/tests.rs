use super::*;
use bevy::time::TimeUpdateStrategy;
use std::sync::Mutex;

#[test]
fn startup_poll_is_immediate_then_rate_limited() {
    let mut polling = SafeAreaPolling::default();
    assert!(polling.take_due(Duration::ZERO));
    assert!(!polling.take_due(RETRY_INTERVAL / 2));
    assert!(polling.take_due(RETRY_INTERVAL));
}

#[test]
fn polling_falls_back_to_a_coarse_interval_after_its_retry_budget() {
    let mut polling = SafeAreaPolling::default();
    for tick in 0..RETRIES {
        assert!(polling.take_due(RETRY_INTERVAL * u32::from(tick)));
    }
    let last_retry = RETRY_INTERVAL * u32::from(RETRIES - 1);
    assert!(!polling.take_due(last_retry + IDLE_INTERVAL / 2));
    assert!(polling.take_due(last_retry + IDLE_INTERVAL));
}

#[test]
fn relevant_event_restarts_polling_immediately() {
    let mut polling = SafeAreaPolling::default();
    for tick in 0..RETRIES {
        assert!(polling.take_due(RETRY_INTERVAL * u32::from(tick)));
    }
    let resumed_at = Duration::from_secs(10);
    polling.request(resumed_at);
    assert!(polling.take_due(resumed_at));
    assert!(!polling.take_due(resumed_at + RETRY_INTERVAL / 2));
}

#[test]
fn repeated_events_do_not_bypass_the_retry_rate_limit() {
    let mut polling = SafeAreaPolling::default();
    assert!(polling.take_due(Duration::ZERO));
    for milliseconds in 1..250 {
        let now = Duration::from_millis(milliseconds);
        polling.request(now);
        assert!(!polling.take_due(now));
    }
    assert!(polling.take_due(RETRY_INTERVAL));
}

#[test]
fn measured_size_rejects_unset_and_degenerate_sizes() {
    let area = |size| SafeArea { size, ..default() };
    assert_eq!(area(Vec2::ZERO).measured_size(), None);
    assert_eq!(area(Vec2::new(1.0, 800.0)).measured_size(), None);
    assert_eq!(area(Vec2::new(f32::NAN, 800.0)).measured_size(), None);
    assert_eq!(
        area(Vec2::new(390.0, 844.0)).measured_size(),
        Some(Vec2::new(390.0, 844.0))
    );
}

#[test]
fn safe_area_plugin_works_alone_and_beside_the_platform_plugin() {
    for platform_first in [false, true] {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        if platform_first {
            app.add_plugins((crate::platform::PlatformPlugin, SafeAreaPlugin));
        } else {
            app.add_plugins((SafeAreaPlugin, crate::platform::PlatformPlugin));
        }
        app.update();
        assert_eq!(*app.world().resource::<SafeArea>(), SafeArea::default());
    }
    let mut app = App::new();
    app.add_plugins(SafeAreaPlugin);
    app.update();
    assert_eq!(*app.world().resource::<SafeArea>(), SafeArea::default());
}

#[test]
fn emulated_insets_replace_the_reading_until_removed() {
    let phone = Insets {
        top: 59.0,
        bottom: 34.0,
        ..default()
    };
    let mut app = App::new();
    app.add_plugins(bevy::time::TimePlugin)
        .add_plugins(SafeAreaPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
        .insert_resource(SafeAreaEmulation(phone));
    app.update();
    assert_eq!(app.world().resource::<SafeArea>().insets, phone);
    // Changes apply at once, inside the poll's rate limit.
    let landscape = Insets {
        left: 59.0,
        right: 59.0,
        bottom: 21.0,
        ..default()
    };
    app.insert_resource(SafeAreaEmulation(landscape));
    app.update();
    assert_eq!(app.world().resource::<SafeArea>().insets, landscape);
    app.world_mut().remove_resource::<SafeAreaEmulation>();
    app.update();
    assert_eq!(*app.world().resource::<SafeArea>(), SafeArea::default());
}

#[test]
fn insets_parse_top_bottom_left_right() {
    assert_eq!(
        Insets::parse("59,34"),
        Ok(Insets {
            top: 59.0,
            bottom: 34.0,
            ..default()
        })
    );
    assert_eq!(
        Insets::parse(" 0, 21 ,59,59.5 "),
        Ok(Insets {
            top: 0.0,
            bottom: 21.0,
            left: 59.0,
            right: 59.5,
        })
    );
    assert_eq!(Insets::parse("0"), Ok(Insets::default()));
    for bad in ["", "59,", "-1", "NaN", "inf", "a,b", "1,2,3,4,5"] {
        assert!(Insets::parse(bad).is_err(), "{bad}");
    }
}

// The test source is a plain `fn`, so its readings live in a static.
static READING: Mutex<(SafeArea, u32)> = Mutex::new((
    SafeArea {
        insets: Insets {
            top: 0.0,
            bottom: 0.0,
            left: 0.0,
            right: 0.0,
        },
        size: Vec2::ZERO,
    },
    0,
));

fn fake_source() -> SafeArea {
    let mut reading = READING.lock().unwrap();
    reading.1 += 1;
    reading.0
}

fn set_reading(top: f32) {
    READING.lock().unwrap().0 = SafeArea {
        insets: Insets {
            top,
            bottom: 34.0,
            ..default()
        },
        size: Vec2::new(390.0, 844.0),
    };
}

#[derive(Resource, Default)]
struct Changes(u32);

fn reads() -> u32 {
    READING.lock().unwrap().1
}

fn advance(app: &mut App, by: Duration) {
    app.insert_resource(TimeUpdateStrategy::ManualDuration(by));
    app.update();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
}

#[test]
fn resource_follows_window_events_and_writes_only_on_change() {
    let mut app = App::new();
    app.add_plugins(bevy::time::TimePlugin);
    app.add_plugins(crate::platform::PlatformPlugin)
        .insert_resource(SafeAreaSource(fake_source));
    // Drive real time by hand; TimePlugin would otherwise use the clock.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.init_resource::<Changes>().add_systems(
        Update,
        |mut changes: ResMut<Changes>, area: Res<SafeArea>| {
            if area.is_changed() {
                changes.0 += 1;
            }
        },
    );

    // Before the key window exists UIKit reports zero; the startup
    // retries pick up the inset once it appears.
    app.update();
    let startup = reads();
    assert!(startup >= 1);
    assert_eq!(app.world().resource::<SafeArea>().insets.top, 0.0);
    set_reading(59.0);
    app.update();
    assert_eq!(reads(), startup, "a second read inside 250 ms");
    advance(&mut app, RETRY_INTERVAL);
    assert_eq!(app.world().resource::<SafeArea>().insets.top, 59.0);
    assert_eq!(
        app.world().resource::<SafeArea>().measured_size(),
        Some(Vec2::new(390.0, 844.0))
    );

    // Exhaust the settling burst; the next idle poll is seconds away.
    for _ in 0..RETRIES {
        advance(&mut app, RETRY_INTERVAL);
    }
    let idle = reads();
    advance(&mut app, RETRY_INTERVAL);
    assert_eq!(reads(), idle);

    // A rotation arrives as a resize and is read on the same frame.
    set_reading(0.0);
    app.world_mut().write_message(WindowResized {
        window: Entity::PLACEHOLDER,
        width: 844.0,
        height: 390.0,
    });
    app.update();
    assert_eq!(reads(), idle + 1);
    assert_eq!(app.world().resource::<SafeArea>().insets.top, 0.0);

    // Unchanged readings never mark the resource changed.
    let changes = app.world().resource::<Changes>().0;
    advance(&mut app, IDLE_INTERVAL);
    advance(&mut app, IDLE_INTERVAL);
    assert_eq!(app.world().resource::<Changes>().0, changes);
    assert_eq!(reads(), idle + 3);
}

#[test]
fn focus_and_resume_request_a_refresh_but_blur_does_not() {
    let mut app = App::new();
    app.add_message::<WindowCreated>()
        .add_message::<WindowResized>()
        .add_message::<WindowFocused>()
        .add_message::<AppLifecycle>();
    let requested = |app: &mut App| {
        app.world_mut()
            .run_system_cached(|mut events: RefreshEvents| events.requested())
            .unwrap()
    };
    assert!(!requested(&mut app));
    app.world_mut().write_message(WindowFocused {
        window: Entity::PLACEHOLDER,
        focused: false,
    });
    app.world_mut().write_message(AppLifecycle::WillSuspend);
    assert!(!requested(&mut app));
    app.world_mut().write_message(WindowFocused {
        window: Entity::PLACEHOLDER,
        focused: true,
    });
    assert!(requested(&mut app));
    for event in [AppLifecycle::WillResume, AppLifecycle::Running] {
        app.world_mut().write_message(event);
        assert!(requested(&mut app));
    }
}
