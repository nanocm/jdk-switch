
pub struct ProgressDisplay;

impl ProgressDisplay {

    pub fn simple_callback() -> impl Fn(u64, u64) {
        let last_percent = std::sync::atomic::AtomicU32::new(u32::MAX);
        move |cur, total| {
            if total == 0 { return; }
            let percentage = (cur as f64 / total as f64 * 100.0) as u32;
            if last_percent.swap(percentage, std::sync::atomic::Ordering::Relaxed) == percentage {
                return;
            }
            let cur_mb = cur / 1024 / 1024;
            let total_mb = total / 1024 / 1024;
            print!("\r  Progress: {:3}% ({:4}/{:4} MB)",
                   percentage, cur_mb, total_mb
            );
            use std::io::{self, Write};
            io::stdout().flush().ok();
            if cur >= total {
                println!();
            }
        }
    }
}
