use std::path::Path;
use crate::runner::Execution;
use chrono::Local;

mod backend;
mod event;
mod runner;
#[allow(dead_code)]
mod screenshot;
mod test;
mod tests;
mod tlog;

fn main() {
    tlog::init();
    let backends = backend::backends();
    let tests = tests::tests();
    let testruns_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("testruns");
    let current_dir = testruns_dir.join("latest");
    let testrun_dir = testruns_dir.join("records").join(format!("{} {:x}", Local::now().format("%Y-%m-%d %H:%M"), std::process::id()));
    std::fs::create_dir_all(&testrun_dir).unwrap();
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(&current_dir);
        let _ = std::os::unix::fs::symlink(&testrun_dir, &current_dir);
    }
    #[cfg(windows)]
    {
        let _ = std::fs::remove_dir(&current_dir);
        let _ = std::os::windows::fs::symlink_dir(&testrun_dir, &current_dir);
    }
    let exec = Execution {
        dir: testrun_dir,
    };
    for backend in &backends {
        runner::run_tests(&exec, &**backend, &tests);
    }
}
