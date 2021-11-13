use crate::backend::{BackendFlags, Instance};

test!(run, BackendFlags::WINIT_SET_MINIMIZED);

async fn run(instance: &dyn Instance) {
    let el = instance.create_event_loop();

    {
        let window = el.create_window(Default::default());
        window.minimized(false).await;
        window.winit_set_minimized(true);
        window.minimized(true).await;
        window.winit_set_minimized(false);
        window.minimized(false).await;
    }
}
