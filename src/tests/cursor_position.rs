use std::time::Duration;
use crate::backend::{BackendFlags, Instance};
use winit::dpi::{PhysicalPosition};

test!(run, BackendFlags::WINIT_SET_CURSOR_POSITION);

async fn run(instance: &dyn Instance) {
    let seat = instance.default_seat();
    seat.position_cursor(0, 0);

    let el = instance.create_event_loop();

    let window = el.create_window(Default::default());
    window.mapped(true).await;
    window.set_outer_position(100, 100);
    window.outer_position(100, 100).await;
    window.winit_set_cursor_position(PhysicalPosition { x: 20, y: 30 });

    instance.cursor_position(120 + window.inner_offset().0, 130 + window.inner_offset().1).await;
}
