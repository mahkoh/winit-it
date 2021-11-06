macro_rules! test {
    ($f:ident) => {
        pub struct Test;

        impl super::Test for Test {
            fn name(&self) -> &str {
                module_path!().trim_start_matches("winit_it::tests::")
            }

            fn run<'a>(&'a self, instance: &'a dyn Instance) -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + 'a>> {
                Box::pin($f(instance))
            }
        }
    };
}

mod window_keyboard;

use std::future::Future;
use std::pin::Pin;
use crate::backend::{Backend, Instance};

pub trait Test {
    fn name(&self) -> &str;
    fn run<'a>(&'a self, instance: &'a dyn Instance) -> Pin<Box<dyn Future<Output=()> + 'a>>;

    fn supports(&self, backend: &dyn Backend) -> bool {
        let _ = backend;
        true
    }
}

pub fn tests() -> Vec<Box<dyn Test>> {
    vec![Box::new(window_keyboard::Test)]
}
