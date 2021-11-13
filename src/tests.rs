macro_rules! test {
    ($f:ident) => {
        test!($f, crate::backend::BackendFlags::empty());
    };
    ($f:ident, $flags:expr) => {
        pub struct Test;

        impl super::Test for Test {
            fn name(&self) -> &str {
                module_path!().trim_start_matches("winit_it::tests::")
            }

            fn run<'a>(
                &'a self,
                instance: &'a dyn Instance,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>> {
                Box::pin($f(instance))
            }

            fn required_flags(&self) -> crate::backend::BackendFlags {
                $flags
            }
        }
    };
}

mod always_on_top;
#[cfg(target_os = "linux")]
mod class;
mod decorations;
mod delete_window;
mod maximize;
mod minimize;
mod physical_inner_size;
mod physical_outer_position;
mod physical_size_bounds;
#[cfg(target_os = "linux")]
mod ping;
mod title;
mod urgency;
mod visible;
mod window_keyboard;
mod resizable;

use crate::backend::{BackendFlags, Instance};
use std::future::Future;
use std::pin::Pin;

pub trait Test: Sync {
    fn name(&self) -> &str;
    fn run<'a>(&'a self, instance: &'a dyn Instance) -> Pin<Box<dyn Future<Output = ()> + 'a>>;

    fn required_flags(&self) -> BackendFlags {
        BackendFlags::empty()
    }
}

pub fn tests() -> Vec<Box<dyn Test>> {
    vec![
        //
        Box::new(window_keyboard::Test),
        Box::new(visible::Test),
        Box::new(always_on_top::Test),
        Box::new(decorations::Test),
        Box::new(physical_inner_size::Test),
        Box::new(physical_outer_position::Test),
        Box::new(title::Test),
        Box::new(maximize::Test),
        Box::new(physical_size_bounds::Test),
        Box::new(urgency::Test),
        #[cfg(target_os = "linux")]
        Box::new(class::Test),
        Box::new(delete_window::Test),
        #[cfg(target_os = "linux")]
        Box::new(ping::Test),
        Box::new(minimize::Test),
        Box::new(resizable::Test),
    ]
}
