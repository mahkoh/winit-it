use crate::backend::Instance;

test!(run);

async fn run(instance: &dyn Instance) {
    let el = instance.create_event_loop();

    let id = {
        let window = el.create_window(Default::default());
        window.mapped(true).await;
        window.winit_id()
    };

    let we = el.window_destroyed_event().await;
    assert_eq!(we.window_id, id);
}
