use crate::backend::{Backend, BackendFlags};
use crate::test::TestData;
use crate::tests::Test;
use crate::tlog::LogState;
use parking_lot::Mutex;
use rayon::prelude::*;
use std::cell::{Cell, RefCell};
use std::fs::OpenOptions;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;
use tokio::task::LocalSet;

pub struct Execution {
    pub dir: PathBuf,
}

struct BackendExecution {
    pub dir: PathBuf,
}

pub fn run_tests(exec: &Execution, backend: &dyn Backend, tests: &[Box<dyn Test>]) {
    let be = BackendExecution {
        dir: exec.dir.join(backend.name()),
    };
    log::info!("Running tests for backend {}", backend.name());
    let num_failed = AtomicUsize::new(0);
    let rto = |(idx, test): (usize, &Box<dyn Test>)| {
        run_test_outer(&be, backend, &**test, idx + 1, tests.len(), &num_failed)
    };
    if backend.flags().contains(BackendFlags::MT_SAFE) {
        tests.par_iter().enumerate().for_each(rto);
    } else {
        tests.iter().enumerate().for_each(rto);
    }
    log::info!(
        "{} out of {} tests failed",
        num_failed.load(Relaxed),
        tests.len()
    );
}

fn run_test_outer(
    be: &BackendExecution,
    backend: &dyn Backend,
    test: &dyn Test,
    idx: usize,
    total: usize,
    num_failed: &AtomicUsize,
) {
    let failed = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let missing_flags = test.required_flags() & !backend.flags();
        if !missing_flags.is_empty() {
            log::warn!(
                "{}/{}: Skipping unsupported test {}. Missing flags: {:?}",
                idx,
                total,
                test.name(),
                missing_flags,
            );
            return false;
        }
        log::info!("{}/{}: Running test {}", idx, total, test.name());
        run_test(&be, backend, test)
    }));
    if failed.unwrap_or(true) {
        num_failed.fetch_add(1, Relaxed);
        log::error!("{}/{}: Test {} failed", idx, total, test.name());
    }
}

fn run_test(exec: &BackendExecution, backend: &dyn Backend, test: &dyn Test) -> bool {
    let test_dir = exec.dir.join(test.name());
    std::fs::create_dir_all(&test_dir).unwrap();
    let td = TestData {
        log_state: Mutex::new(LogState::new(
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(test_dir.join("log"))
                .unwrap(),
        )),
        test_dir,
        next_image_id: Default::default(),
        error: Cell::new(false),
        instance: RefCell::new(None),
    };
    crate::test::set_test_data_and_run(&td, || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .on_thread_park(|| {
                crate::test::with_test_data(|td| {
                    td.instance.borrow().as_ref().unwrap().before_poll();
                })
            })
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let ls = LocalSet::new();
            ls.run_until(async {
                let instance = Rc::new(backend.instantiate());
                *td.instance.borrow_mut() = Some(instance.clone());
                if tokio::time::timeout(Duration::from_secs(5), test.run(&**instance))
                    .await
                    .is_err()
                {
                    log::error!("Test timed out");
                }
                *td.instance.borrow_mut() = None;
            })
            .await;
            ls.await;
        });
        if td.error.get() {
            log::error!("Test failed due to previous error");
        }
    });
    td.error.get()
}
