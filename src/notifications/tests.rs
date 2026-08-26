//! Policy tests against the desktop fake. Nothing here needs a device: what is
//! being checked is what a consumer's own code has to get right.

use super::*;

/// The fake backend is a process-global singleton, so tests that drive it must
/// not run concurrently. Holding this guard serializes them and resets the fake
/// (and clears env knobs) to a clean slate.
static FAKE_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn guarded() -> std::sync::MutexGuard<'static, ()> {
    let guard = FAKE_GUARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    backend::reset();
    for key in [
        "BEVY_IOS_FAKE_NOTIFICATIONS",
        "BEVY_IOS_FAKE_NOTIFICATION_OPENED",
        "BEVY_IOS_FAKE_NOTIFICATIONS_REFUSE",
    ] {
        // SAFETY: every test touching these holds the guard above.
        unsafe { std::env::remove_var(key) };
    }
    guard
}

fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(NotificationsPlugin);
    app
}

fn write<M: Message>(app: &mut App, message: M) {
    app.world_mut().resource_mut::<Messages<M>>().write(message);
}

/// Take the messages out of the buffer. Reading with a fresh cursor would
/// re-read them — Bevy double-buffers for two frames — which quietly turns
/// "delivered once" into a claim these tests cannot make.
fn drain<M: Message>(app: &mut App) -> Vec<M> {
    app.world_mut()
        .resource_mut::<Messages<M>>()
        .drain()
        .collect()
}

fn schedule(id: &str, after: Duration) -> ScheduleNotification {
    ScheduleNotification {
        id: id.into(),
        title: "THE VAN CAME".into(),
        body: "Something new is in the machine.".into(),
        after,
    }
}

fn granted(app: &mut App) {
    write(app, RequestNotificationPermission);
    app.update(); // pumps the request
    app.update(); // polls the resolved status
}

#[test]
fn permission_starts_unasked_and_a_request_resolves_it() {
    let _guard = guarded();
    let mut app = build_app();
    app.update();
    assert_eq!(
        *app.world().resource::<NotificationPermission>(),
        NotificationPermission::NotDetermined
    );

    granted(&mut app);
    assert_eq!(
        *app.world().resource::<NotificationPermission>(),
        NotificationPermission::Authorized
    );
    let changes = drain::<NotificationPermissionChanged>(&mut app);
    assert_eq!(changes.len(), 1, "one change, not one a frame");
    assert_eq!(changes[0].0, NotificationPermission::Authorized);
}

#[test]
fn a_refusal_is_reported_with_a_reason_and_nothing_is_pending() {
    let _guard = guarded();
    // SAFETY: the guard serializes every test that reads this.
    unsafe { std::env::set_var("BEVY_IOS_FAKE_NOTIFICATIONS", "denied") };
    let mut app = build_app();
    granted(&mut app);
    assert_eq!(
        *app.world().resource::<NotificationPermission>(),
        NotificationPermission::Denied
    );

    write(&mut app, schedule("restock", Duration::from_secs(60)));
    app.update();
    let failures = drain::<NotificationScheduleFailed>(&mut app);
    assert_eq!(failures.len(), 1);
    assert!(
        failures[0].reason.contains("not authorized"),
        "{:?}",
        failures[0].reason
    );
    assert!(app.world().resource::<PendingNotifications>().is_empty());
}

#[test]
fn the_same_id_replaces_rather_than_stacks() {
    let _guard = guarded();
    let mut app = build_app();
    granted(&mut app);

    for _ in 0..3 {
        write(&mut app, schedule("restock", Duration::from_secs(60)));
        app.update();
    }
    let pending = app.world().resource::<PendingNotifications>();
    assert_eq!(pending.len(), 1, "a slipping timer must not queue three");
    assert!(pending.contains("restock"));
    assert_eq!(backend::scheduled(), vec!["restock".to_string()]);

    // A second, different id is its own thing.
    write(&mut app, schedule("route", Duration::from_secs(120)));
    app.update();
    assert_eq!(app.world().resource::<PendingNotifications>().len(), 2);
}

#[test]
fn cancelling_drops_one_or_all_of_them() {
    let _guard = guarded();
    let mut app = build_app();
    granted(&mut app);
    for id in ["restock", "route", "delivery"] {
        write(&mut app, schedule(id, Duration::from_secs(60)));
    }
    app.update();
    assert_eq!(app.world().resource::<PendingNotifications>().len(), 3);

    write(&mut app, CancelNotification("route".into()));
    app.update();
    let pending = app.world().resource::<PendingNotifications>();
    assert_eq!(pending.len(), 2);
    assert!(!pending.contains("route"));
    assert!(!backend::scheduled().contains(&"route".to_string()));

    write(&mut app, CancelAllNotifications);
    app.update();
    assert!(app.world().resource::<PendingNotifications>().is_empty());
    assert!(backend::scheduled().is_empty());
}

#[test]
fn a_cold_launch_open_arrives_once() {
    let _guard = guarded();
    // SAFETY: the guard serializes every test that reads this.
    unsafe { std::env::set_var("BEVY_IOS_FAKE_NOTIFICATION_OPENED", "restock, route") };
    let mut app = build_app();
    app.update();

    let opened: Vec<String> = drain::<NotificationOpened>(&mut app)
        .into_iter()
        .map(|NotificationOpened(id)| id)
        .collect();
    assert_eq!(opened, vec!["restock".to_string(), "route".to_string()]);

    // Delivered once, not every frame.
    app.update();
    assert!(drain::<NotificationOpened>(&mut app).is_empty());
}

#[test]
fn an_open_clears_what_was_waiting() {
    let _guard = guarded();
    let mut app = build_app();
    granted(&mut app);
    write(&mut app, schedule("restock", Duration::from_secs(60)));
    app.update();
    assert!(
        app.world()
            .resource::<PendingNotifications>()
            .contains("restock")
    );

    backend::open("restock");
    app.update();
    let opened = drain::<NotificationOpened>(&mut app);
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].0, "restock");
    assert!(
        !app.world()
            .resource::<PendingNotifications>()
            .contains("restock"),
        "a delivered notification is no longer waiting"
    );
}

#[test]
fn a_schedule_that_fails_later_still_surfaces() {
    let _guard = guarded();
    // SAFETY: the guard serializes every test that reads this.
    unsafe { std::env::set_var("BEVY_IOS_FAKE_NOTIFICATIONS_REFUSE", "1") };
    let mut app = build_app();
    granted(&mut app);
    write(&mut app, schedule("restock", Duration::from_secs(60)));
    app.update(); // accepted at the boundary
    app.update(); // and refused on the way back

    let failures = drain::<NotificationScheduleFailed>(&mut app);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].id, "restock");
    assert!(
        app.world().resource::<PendingNotifications>().is_empty(),
        "a request that failed is not pending"
    );
}

#[test]
fn a_time_that_has_already_passed_is_refused() {
    let _guard = guarded();
    let mut app = build_app();
    granted(&mut app);
    write(&mut app, schedule("now", Duration::ZERO));
    app.update();
    let failures = drain::<NotificationScheduleFailed>(&mut app);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].reason.contains("greater than zero"));
    assert!(app.world().resource::<PendingNotifications>().is_empty());
}

#[test]
fn text_that_cannot_cross_the_boundary_is_refused_not_truncated() {
    let _guard = guarded();
    let mut app = build_app();
    granted(&mut app);
    write(
        &mut app,
        ScheduleNotification {
            id: "restock".into(),
            title: "THE VAN\0 CAME".into(),
            body: "body".into(),
            after: Duration::from_secs(60),
        },
    );
    app.update();
    let failures = drain::<NotificationScheduleFailed>(&mut app);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].reason.contains("NUL"));
    assert!(
        backend::scheduled().is_empty(),
        "nothing reached the backend"
    );
}

#[test]
fn permission_answers_the_two_questions_a_consumer_asks() {
    assert!(NotificationPermission::Authorized.can_deliver());
    assert!(
        NotificationPermission::Provisional.can_deliver(),
        "quiet delivery is still delivery"
    );
    assert!(!NotificationPermission::Denied.can_deliver());
    assert!(!NotificationPermission::NotDetermined.can_deliver());

    assert!(!NotificationPermission::NotDetermined.is_determined());
    for settled in [
        NotificationPermission::Denied,
        NotificationPermission::Authorized,
        NotificationPermission::Provisional,
    ] {
        assert!(settled.is_determined());
    }
}

#[test]
fn the_wire_format_is_the_one_the_shim_writes() {
    // The Swift side hand-builds this JSON; if the tags drift, opens vanish
    // silently. Pin both directions.
    let opened: Vec<Event> =
        serde_json::from_str(r#"[{"kind":"opened","id":"restock"}]"#).expect("parses");
    assert_eq!(
        opened,
        vec![Event::Opened {
            id: "restock".into()
        }]
    );
    let failed: Vec<Event> =
        serde_json::from_str(r#"[{"kind":"schedule_failed","id":"a","reason":"b"}]"#)
            .expect("parses");
    assert_eq!(
        failed,
        vec![Event::ScheduleFailed {
            id: "a".into(),
            reason: "b".into()
        }]
    );
    // Garbage is empty, never a panic.
    assert!(
        serde_json::from_str::<Vec<Event>>("not json")
            .unwrap_or_default()
            .is_empty()
    );
}
