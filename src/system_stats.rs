//! Lightweight live system metrics for the HUD overlay.
//!
//! A background thread polls sysfs/proc at 1 Hz and publishes results
//! through atomics so the render loop can read them without locking or
//! blocking. FPS is set directly from `frame::render_frame` when its
//! rolling perf window closes. See T5 in the debug plan.
//!
//! All sysfs paths here were verified on pingo (ThinkPad E480, Debian
//! 13, UHD 620). Only three thermal zones exist on that box —
//! `thermal_zone0` acpitz (whole-SOC), `thermal_zone2` x86_pkg_temp
//! (best iGPU die proxy). `thermal_zone1` is pch_skylake and ignored.

use std::fs;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub struct SystemStats {
    /// FPS × 10, written by the render loop each perf window.
    pub fps_x10: AtomicU32,
    /// CPU busy percent × 10, computed from /proc/stat deltas.
    pub cpu_pct_x10: AtomicU32,
    /// acpitz temp (°C × 10). i32 because a cold boot can briefly read 0.
    pub cpu_temp_c_x10: AtomicI32,
    /// x86_pkg_temp (°C × 10) — proxy for iGPU die temp on i915.
    pub pkg_temp_c_x10: AtomicI32,
    /// Own-process RSS in MiB.
    pub rss_mb: AtomicU32,
    /// System MemAvailable in MiB.
    pub mem_avail_mb: AtomicU32,
    /// i915 current GT frequency (MHz).
    pub gpu_cur_mhz: AtomicU32,
    /// i915 max GT frequency (MHz). Cached at startup.
    pub gpu_max_mhz: AtomicU32,
}

impl SystemStats {
    /// Construct, seed initial values, and spawn the 1 Hz poller.
    /// Returned `Arc` is shared between the poller thread and the render
    /// loop. The thread runs until process exit — there's no explicit
    /// stop because vgalizer only exits the process wholesale.
    pub fn new() -> Arc<Self> {
        let stats = Arc::new(Self {
            fps_x10: AtomicU32::new(0),
            cpu_pct_x10: AtomicU32::new(0),
            cpu_temp_c_x10: AtomicI32::new(0),
            pkg_temp_c_x10: AtomicI32::new(0),
            rss_mb: AtomicU32::new(0),
            mem_avail_mb: AtomicU32::new(0),
            gpu_cur_mhz: AtomicU32::new(0),
            gpu_max_mhz: AtomicU32::new(0),
        });

        // GPU max frequency doesn't change at runtime; read it once.
        if let Some(v) = read_u32_file("/sys/class/drm/card0/gt_max_freq_mhz") {
            stats.gpu_max_mhz.store(v, Ordering::Relaxed);
        }

        // Seed a first snapshot synchronously so the HUD has real numbers
        // on frame 1 instead of zeros.
        stats.refresh_non_cpu();

        let bg = stats.clone();
        thread::Builder::new()
            .name("vgalizer-sysstats".into())
            .spawn(move || {
                let mut prev = read_proc_stat();
                loop {
                    thread::sleep(Duration::from_secs(1));

                    // CPU% — compute over a 1 s delta of /proc/stat.
                    let cur = read_proc_stat();
                    if let (Some(p), Some(c)) = (prev.as_ref(), cur.as_ref()) {
                        let total_delta = c.total.saturating_sub(p.total) as i64;
                        let idle_delta = c.idle.saturating_sub(p.idle) as i64;
                        if total_delta > 0 {
                            let busy = (total_delta - idle_delta).max(0);
                            // × 10 so we keep one decimal of precision.
                            let pct = ((busy * 1000) / total_delta) as u32;
                            bg.cpu_pct_x10.store(pct, Ordering::Relaxed);
                        }
                    }
                    prev = cur;

                    bg.refresh_non_cpu();
                }
            })
            .expect("failed to spawn vgalizer-sysstats thread");

        stats
    }

    /// Refresh everything that doesn't need a delta. Called from the
    /// poller thread and once synchronously during `new()`.
    fn refresh_non_cpu(&self) {
        if let Some(v) = read_temp_c_x10("/sys/class/thermal/thermal_zone0/temp") {
            self.cpu_temp_c_x10.store(v, Ordering::Relaxed);
        }
        if let Some(v) = read_temp_c_x10("/sys/class/thermal/thermal_zone2/temp") {
            self.pkg_temp_c_x10.store(v, Ordering::Relaxed);
        }
        if let Some(v) = read_vmrss_mb() {
            self.rss_mb.store(v, Ordering::Relaxed);
        }
        if let Some(v) = read_memavail_mb() {
            self.mem_avail_mb.store(v, Ordering::Relaxed);
        }
        if let Some(v) = read_u32_file("/sys/class/drm/card0/gt_cur_freq_mhz") {
            self.gpu_cur_mhz.store(v, Ordering::Relaxed);
        }
    }

    /// Publish the latest rolling-window FPS from the render loop.
    pub fn set_fps(&self, fps: f32) {
        let v = (fps * 10.0).round().max(0.0) as u32;
        self.fps_x10.store(v, Ordering::Relaxed);
    }

    /// Format the HUD stats line. Returns an empty string if the
    /// poller hasn't populated real values yet (still the first second
    /// of process lifetime with no set_fps call).
    pub fn format_line(&self) -> String {
        let fps = self.fps_x10.load(Ordering::Relaxed) as f32 / 10.0;
        let cpu = self.cpu_pct_x10.load(Ordering::Relaxed) as f32 / 10.0;
        let ct = self.cpu_temp_c_x10.load(Ordering::Relaxed) as f32 / 10.0;
        let pt = self.pkg_temp_c_x10.load(Ordering::Relaxed) as f32 / 10.0;
        let rss = self.rss_mb.load(Ordering::Relaxed);
        let av = self.mem_avail_mb.load(Ordering::Relaxed);
        let g = self.gpu_cur_mhz.load(Ordering::Relaxed);
        let gm = self.gpu_max_mhz.load(Ordering::Relaxed);
        format!(
            "FPS {:.1}  CPU {:.0}%  T {:.0}°/{:.0}°  RAM {}/{} MB  GPU {}/{} MHz",
            fps, cpu, ct, pt, rss, av, g, gm
        )
    }
}

/// Snapshot of `/proc/stat` totals we care about for CPU%.
struct ProcStat {
    total: u64,
    idle: u64,
}

fn read_proc_stat() -> Option<ProcStat> {
    let s = fs::read_to_string("/proc/stat").ok()?;
    let line = s.lines().next()?;
    let mut it = line.split_ascii_whitespace();
    if it.next()? != "cpu" {
        return None;
    }
    // user nice sys idle iowait irq softirq steal guest guest_nice
    let vals: Vec<u64> = it.filter_map(|v| v.parse().ok()).collect();
    if vals.len() < 4 {
        return None;
    }
    let total: u64 = vals.iter().sum();
    // idle + iowait, matching the convention used by htop et al.
    let idle = vals[3] + vals.get(4).copied().unwrap_or(0);
    Some(ProcStat { total, idle })
}

fn read_u32_file(path: &str) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Read a `thermal_zoneN/temp` millicelsius file and return °C × 10.
fn read_temp_c_x10(path: &str) -> Option<i32> {
    let raw: i32 = fs::read_to_string(path).ok()?.trim().parse().ok()?;
    // millicelsius → °C × 10 = divide by 100
    Some(raw / 100)
}

fn read_vmrss_mb() -> Option<u32> {
    let s = fs::read_to_string("/proc/self/status").ok()?;
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u32 = rest.split_ascii_whitespace().next()?.parse().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

fn read_memavail_mb() -> Option<u32> {
    let s = fs::read_to_string("/proc/meminfo").ok()?;
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let kb: u32 = rest.split_ascii_whitespace().next()?.parse().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}
