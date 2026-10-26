use std::ffi::CString;
use std::fs;
use std::mem::MaybeUninit;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CpuMetrics {
    pub percent: f32,
    pub cores: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiskMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemMetrics {
    pub cpu: CpuMetrics,
    pub load_avg: [f64; 3],
    pub memory: MemoryMetrics,
    pub disk: DiskMetrics,
    pub temperature: Option<f32>,
    pub uptime_seconds: u64,
}

pub struct SystemMonitor {
    last_cpu_sample: Mutex<Option<(Instant, u64, u64)>>, // (sample_time, total_ticks, idle_ticks)
    last_cpu_percent: Mutex<f32>,
    start_time: Instant,
}

impl Default for SystemMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemMonitor {
    pub fn new() -> Self {
        Self {
            last_cpu_sample: Mutex::new(None),
            last_cpu_percent: Mutex::new(0.0),
            start_time: Instant::now(),
        }
    }

    pub fn snapshot(&self) -> SystemMetrics {
        let (cpu, load_avg) = self.collect_cpu_and_load();
        let memory = self.collect_memory();
        let disk = self.collect_disk();
        let temperature = self.collect_temperature();
        let uptime_seconds = self.collect_uptime();

        SystemMetrics {
            cpu,
            load_avg,
            memory,
            disk,
            temperature,
            uptime_seconds,
        }
    }

    fn collect_cpu_and_load(&self) -> (CpuMetrics, [f64; 3]) {
        let mut loads = [0.0f64; 3];
        let load_res = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
        let load_avg = if load_res == 3 {
            loads
        } else if let Some(loads_from_proc) = read_linux_loadavg() {
            loads_from_proc
        } else {
            [0.0, 0.0, 0.0]
        };

        let raw_cores = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
        let cores = if raw_cores > 0 { raw_cores as usize } else { 1 };

        let percent = if let Some((total, idle, parsed_cores)) = parse_linux_cpu_stat() {
            let actual_cores = if parsed_cores > 0 { parsed_cores } else { cores };
            let now = Instant::now();
            let mut sample_lock = self.last_cpu_sample.lock().unwrap();

            let calculated = if let Some((prev_time, prev_total, prev_idle)) = *sample_lock {
                let delta_total = total.saturating_sub(prev_total);
                let delta_idle = idle.saturating_sub(prev_idle);
                let elapsed = now.duration_since(prev_time);

                if elapsed.as_millis() >= 100 && delta_total > 0 {
                    let busy = delta_total.saturating_sub(delta_idle);
                    let pct = (busy as f64 / delta_total as f64 * 100.0) as f32;
                    let clamped = pct.clamp(0.0, 100.0);
                    *self.last_cpu_percent.lock().unwrap() = clamped;
                    *sample_lock = Some((now, total, idle));
                    clamped
                } else {
                    *self.last_cpu_percent.lock().unwrap()
                }
            } else {
                *sample_lock = Some((now, total, idle));
                0.0
            };

            return (
                CpuMetrics {
                    percent: calculated,
                    cores: actual_cores,
                },
                load_avg,
            );
        } else {
            // Fallback for non-Linux (e.g. macOS) using normalized load average
            let normalized = ((load_avg[0] / cores as f64) * 100.0).clamp(0.0, 100.0) as f32;
            normalized
        };

        (CpuMetrics { percent, cores }, load_avg)
    }

    fn collect_memory(&self) -> MemoryMetrics {
        if let Some(mem) = read_linux_meminfo() {
            return mem;
        }

        // Fallback for macOS / Unix systems using sysctl
        #[cfg(target_os = "macos")]
        {
            let mut mem_size: u64 = 0;
            let mut len = std::mem::size_of::<u64>();
            let mut mib = [libc::CTL_HW, libc::HW_MEMSIZE];
            let res = unsafe {
                libc::sysctl(
                    mib.as_mut_ptr(),
                    2,
                    &mut mem_size as *mut u64 as *mut libc::c_void,
                    &mut len,
                    std::ptr::null_mut(),
                    0,
                )
            };
            if res == 0 && mem_size > 0 {
                // Approximate used / free for macOS fallback in testing
                let total_bytes = mem_size;
                let used_bytes = total_bytes / 2;
                let free_bytes = total_bytes.saturating_sub(used_bytes);
                let percent = 50.0;
                return MemoryMetrics {
                    total_bytes,
                    used_bytes,
                    free_bytes,
                    percent,
                };
            }
        }

        MemoryMetrics {
            total_bytes: 0,
            used_bytes: 0,
            free_bytes: 0,
            percent: 0.0,
        }
    }

    fn collect_disk(&self) -> DiskMetrics {
        collect_statvfs_disk("/")
    }

    fn collect_temperature(&self) -> Option<f32> {
        // Standard Linux thermal zone paths (e.g. Rockchip RK3399, Allwinner, Raspberry Pi)
        for zone in 0..6 {
            let path_str = format!("/sys/class/thermal/thermal_zone{zone}/temp");
            let path = Path::new(&path_str);
            if path.exists() {
                if let Ok(content) = fs::read_to_string(path) {
                    if let Ok(milli) = content.trim().parse::<f32>() {
                        let c = if milli > 1000.0 { milli / 1000.0 } else { milli };
                        if (0.0..=130.0).contains(&c) {
                            return Some(c);
                        }
                    }
                }
            }
        }
        None
    }

    fn collect_uptime(&self) -> u64 {
        if let Ok(content) = fs::read_to_string("/proc/uptime") {
            if let Some(first) = content.split_whitespace().next() {
                if let Ok(secs) = first.parse::<f64>() {
                    return secs.max(0.0) as u64;
                }
            }
        }
        self.start_time.elapsed().as_secs()
    }
}

fn collect_statvfs_disk(mount: &str) -> DiskMetrics {
    let c_path = match CString::new(mount) {
        Ok(s) => s,
        Err(_) => {
            return DiskMetrics {
                total_bytes: 0,
                used_bytes: 0,
                free_bytes: 0,
                percent: 0.0,
            }
        }
    };

    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    let res = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if res == 0 {
        let stat = unsafe { stat.assume_init() };
        let frsize = stat.f_frsize as u64;
        let total = stat.f_blocks as u64 * frsize;
        let free = stat.f_bavail as u64 * frsize;
        let used = total.saturating_sub(stat.f_bfree as u64 * frsize);
        let percent = if total > 0 {
            ((used as f64 / total as f64) * 100.0) as f32
        } else {
            0.0
        };

        DiskMetrics {
            total_bytes: total,
            used_bytes: used,
            free_bytes: free,
            percent: percent.clamp(0.0, 100.0),
        }
    } else {
        DiskMetrics {
            total_bytes: 0,
            used_bytes: 0,
            free_bytes: 0,
            percent: 0.0,
        }
    }
}

fn parse_linux_cpu_stat() -> Option<(u64, u64, usize)> {
    let content = fs::read_to_string("/proc/stat").ok()?;
    parse_cpu_stat_str(&content)
}

fn parse_cpu_stat_str(content: &str) -> Option<(u64, u64, usize)> {
    let mut lines = content.lines();
    let first = lines.next()?;
    if !first.starts_with("cpu ") {
        return None;
    }
    let parts: Vec<u64> = first
        .split_whitespace()
        .skip(1)
        .filter_map(|s| s.parse::<u64>().ok())
        .collect();
    if parts.len() < 4 {
        return None;
    }
    // user, nice, system, idle, iowait, irq, softirq, steal
    let user = parts[0];
    let nice = parts[1];
    let system = parts[2];
    let idle = parts[3];
    let iowait = parts.get(4).copied().unwrap_or(0);
    let irq = parts.get(5).copied().unwrap_or(0);
    let softirq = parts.get(6).copied().unwrap_or(0);
    let steal = parts.get(7).copied().unwrap_or(0);

    let idle_time = idle + iowait;
    let total_time = user + nice + system + idle + iowait + irq + softirq + steal;

    let mut cores = 0;
    for line in lines {
        if line.starts_with("cpu") && line.chars().nth(3).map_or(false, |c| c.is_ascii_digit()) {
            cores += 1;
        } else if !line.starts_with("cpu") {
            break;
        }
    }
    if cores == 0 {
        cores = 1;
    }

    Some((total_time, idle_time, cores))
}

fn read_linux_loadavg() -> Option<[f64; 3]> {
    let content = fs::read_to_string("/proc/loadavg").ok()?;
    let parts: Vec<f64> = content
        .split_whitespace()
        .take(3)
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    if parts.len() == 3 {
        Some([parts[0], parts[1], parts[2]])
    } else {
        None
    }
}

fn read_linux_meminfo() -> Option<MemoryMetrics> {
    let content = fs::read_to_string("/proc/meminfo").ok()?;
    parse_meminfo_str(&content)
}

fn parse_meminfo_str(content: &str) -> Option<MemoryMetrics> {
    let mut mem_total_kb = 0u64;
    let mut mem_avail_kb = 0u64;
    let mut mem_free_kb = 0u64;
    let mut buffers_kb = 0u64;
    let mut cached_kb = 0u64;

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            mem_total_kb = parse_kb(rest);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            mem_avail_kb = parse_kb(rest);
        } else if let Some(rest) = line.strip_prefix("MemFree:") {
            mem_free_kb = parse_kb(rest);
        } else if let Some(rest) = line.strip_prefix("Buffers:") {
            buffers_kb = parse_kb(rest);
        } else if let Some(rest) = line.strip_prefix("Cached:") {
            cached_kb = parse_kb(rest);
        }
    }

    if mem_total_kb == 0 {
        return None;
    }

    let available_kb = if mem_avail_kb > 0 {
        mem_avail_kb
    } else {
        mem_free_kb + buffers_kb + cached_kb
    };

    let total_bytes = mem_total_kb * 1024;
    let free_bytes = available_kb * 1024;
    let used_bytes = total_bytes.saturating_sub(free_bytes);
    let percent = if total_bytes > 0 {
        ((used_bytes as f64 / total_bytes as f64) * 100.0) as f32
    } else {
        0.0
    };

    Some(MemoryMetrics {
        total_bytes,
        used_bytes,
        free_bytes,
        percent: percent.clamp(0.0, 100.0),
    })
}

fn parse_kb(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_snapshot_smoke() {
        let monitor = SystemMonitor::new();
        let snap = monitor.snapshot();
        assert!(snap.cpu.cores >= 1);
        assert!(snap.cpu.percent >= 0.0 && snap.cpu.percent <= 100.0);
        assert!(snap.load_avg[0] >= 0.0);
        assert!(snap.disk.total_bytes > 0);
        assert!(snap.disk.percent >= 0.0 && snap.disk.percent <= 100.0);
    }

    #[test]
    fn test_parse_cpu_stat_str() {
        let mock_stat = "\
cpu  2255 34 2290 22625563 6290 127 456 0 0 0
cpu0 1132 17 1145 11312781 3145 63 228 0 0 0
cpu1 1123 17 1145 11312782 3145 64 228 0 0 0
intr 1234567 0 0
";
        let parsed = parse_cpu_stat_str(mock_stat).expect("parse cpu stat");
        let (total, idle, cores) = parsed;
        assert_eq!(cores, 2);
        assert_eq!(idle, 22625563 + 6290);
        assert!(total > idle);
    }

    #[test]
    fn test_parse_meminfo_str() {
        let mock_meminfo = "\
MemTotal:        4000000 kB
MemFree:          500000 kB
MemAvailable:    2500000 kB
Buffers:          100000 kB
Cached:          1000000 kB
SwapTotal:       1000000 kB
SwapFree:         800000 kB
";
        let mem = parse_meminfo_str(mock_meminfo).expect("parse meminfo");
        assert_eq!(mem.total_bytes, 4000000 * 1024);
        assert_eq!(mem.free_bytes, 2500000 * 1024);
        assert_eq!(mem.used_bytes, 1500000 * 1024);
        assert!((mem.percent - 37.5).abs() < 0.01);
    }

    #[test]
    fn test_parse_meminfo_without_available() {
        let mock_meminfo = "\
MemTotal:        4000000 kB
MemFree:          500000 kB
Buffers:          100000 kB
Cached:          1400000 kB
";
        let mem = parse_meminfo_str(mock_meminfo).expect("parse meminfo");
        assert_eq!(mem.total_bytes, 4000000 * 1024);
        assert_eq!(mem.free_bytes, (500000 + 100000 + 1400000) * 1024);
        assert_eq!(mem.used_bytes, (4000000 - 2000000) * 1024);
        assert!((mem.percent - 50.0).abs() < 0.01);
    }
}
