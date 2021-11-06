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

mod window_keyboard;

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
    ]
}
