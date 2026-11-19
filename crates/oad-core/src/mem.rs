//! 把空出来的堆内存还给系统。
//!
//! 灌大量数据、建索引时堆会涨到几百 MB；用完释放了，系统的 malloc（macOS、glibc）也不会马上还，
//! 实测灌 10 万段后真正在用的约 50MB，进程却占着 300MB 以上。大块内存放掉以后调 `release_soon`，
//! 安静 2 秒后统一还一次（`malloc_zone_pressure_relief` / `malloc_trim`）；别的平台什么都不做。
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static PENDING: AtomicBool = AtomicBool::new(false);
static START: Once = Once::new();

/// 记一笔“刚放掉了一大块”，过一会儿还给系统（连着调多次只还一次）。
pub fn release_soon() {
    PENDING.store(true, Ordering::Relaxed);
    START.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("oad-mem".into())
            .spawn(|| {
                loop {
                    std::thread::sleep(Duration::from_secs(2));
                    // 这 2 秒里还有新的就再等等（批量卸载、连续提交时只在最后还一次）
                    if PENDING.swap(false, Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_secs(2));
                        if !PENDING.load(Ordering::Relaxed) {
                            release_now();
                        } else {
                            PENDING.store(true, Ordering::Relaxed);
                        }
                    }
                }
            });
    });
}

/// 马上把空闲的堆还给系统。
pub fn release_now() {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        // 空指针 = 所有 zone；goal 0 = 能还多少还多少
        unsafe {
            malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
        }
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> std::ffi::c_int;
        }
        unsafe {
            malloc_trim(0);
        }
    }
}
