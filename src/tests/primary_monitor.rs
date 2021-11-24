use crate::backend::{BackendFlags, Instance};
use std::time::Duration;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::monitor::VideoMode;

test!(run, BackendFlags::SECOND_MONITOR);

async fn run(instance: &dyn Instance) {
    let el = instance.create_event_loop();

    el.num_available_monitors(1).await;
    let monitor = el.primary_monitor().unwrap();
    assert_eq!(monitor.position().x, 0);

    instance.enable_second_monitor(true);

    el.num_available_monitors(2).await;
    let monitor = el.primary_monitor().unwrap();
    assert!(monitor.position().x > 0);

    instance.enable_second_monitor(false);

    el.num_available_monitors(1).await;
    let monitor = el.primary_monitor().unwrap();
    assert_eq!(monitor.position().x, 0);

    tokio::time::sleep(Duration::from_secs(1)).await;
}
