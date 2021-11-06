use crate::backend::Backend;
use crate::test::TestData;
use crate::tests::Test;
use crate::tlog::LogState;
use parking_lot::Mutex;
use std::cell::Cell;
use std::fs::OpenOptions;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
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
    let mut num_failed = 0;
    for (idx, test) in tests.iter().enumerate() {
        let failed = std::panic::catch_unwind(AssertUnwindSafe(|| {
            if !test.supports(backend) {
                log::info!(
                    "{}/{}: Skipping unsupported test {}",
                    idx + 1,
                    tests.len(),
                    test.name()
                );
                return false;
            }
            log::info!("{}/{}: Running test {}", idx + 1, tests.len(), test.name());
            run_test(&be, backend, &**test)
        }));
        if failed.unwrap_or(true) {
            num_failed += 1;
            log::error!("{}/{}: Test failed", idx + 1, tests.len());
        }
    }
    log::info!("{} out of {} tests failed", num_failed, tests.len());
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
    };
    crate::test::set_test_data_and_run(&td, || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let ls = LocalSet::new();
            ls.run_until(async {
                let instance = backend.instantiate();
                if tokio::time::timeout(Duration::from_secs(5), test.run(&*instance))
                    .await
                    .is_err()
                {
                    log::error!("Test timed out");
                }
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
