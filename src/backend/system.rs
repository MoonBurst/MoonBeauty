use std::fs;
use std::path::Path;
use tokio::process::Command;

#[derive(Debug, Clone, Default)]
pub struct CpuMetrics {
    pub usage_pct: u8,
    pub temp_c: u32,
    pub top_procs: Vec<ProcessItem>,
}

#[derive(Debug, Clone, Default)]
pub struct GpuMetrics {
    pub name: String,
    pub usage_pct: u8,
    pub temp_c: u32,
    pub power_w: u32,
    pub vram_free_gib: u32,
    pub top_procs: Vec<ProcessItem>,
}

#[derive(Debug, Clone, Default)]
pub struct RamMetrics {
    pub total_gib: f32,
    pub available_gib: f32,
    pub effective_avail_gib: f32,
    pub zram_ratio: f32,
    pub zram_saved_gib: f32,
    pub top_procs: Vec<ProcessItem>,
}

#[derive(Debug, Clone, Default)]
pub struct NetMetrics {
    pub iface: String,
    pub hw_type: String,
    pub ip: String,
    pub down_str: String,
    pub up_str: String,
    pub ping_str: String,
}

#[derive(Debug, Clone, Default)]
pub struct BatteryMetrics {
    pub has_battery: bool,
    pub percent: u8,
    pub status: String,
    pub power_w: f32,
}

#[derive(Debug, Clone)]
pub struct ProcessItem {
    pub pid: u32,
    pub name: String,
    pub metric: String,
    pub is_highlight: bool,
}

pub struct SystemCollector {
    prev_rx_bytes: u64,
    prev_tx_bytes: u64,
    prev_iface: String,
    prev_net_time: std::time::Instant,
}

impl SystemCollector {
    pub fn new() -> Self {
        Self {
            prev_rx_bytes: 0,
            prev_tx_bytes: 0,
            prev_iface: String::new(),
            prev_net_time: std::time::Instant::now(),
        }
    }

    pub async fn collect_cpu() -> CpuMetrics {
        let mut temp_c = 0;
        // Try /sys/class/thermal first (may be empty on some setups).
        for i in 0..10 {
            let path = format!("/sys/class/thermal/thermal_zone{}/temp", i);
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(v) = raw.trim().parse::<u32>() {
                    temp_c = if v > 1000 { v / 1000 } else { v };
                    if temp_c > 0 { break; }
                }
            }
        }
        // Fallback: /sys/class/hwmon k10temp (AMD), coretemp (Intel),
        // zenpower (AMD Ryzen OC). These systems have no thermal_zone files.
        if temp_c == 0 {
            if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
                for entry in entries.flatten() {
                    let dir = entry.path();
                    let name = fs::read_to_string(dir.join("name"))
                        .map(|s| s.trim().to_string()).unwrap_or_default();
                    if name == "k10temp" || name == "coretemp" || name == "zenpower" {
                        if let Ok(raw) = fs::read_to_string(dir.join("temp1_input")) {
                            if let Ok(v) = raw.trim().parse::<u32>() {
                                temp_c = v / 1000;
                                if temp_c > 0 { break; }
                            }
                        }
                    }
                }
            }
        }

        let mut usage_pct = 0;
        if let Ok(loadavg) = fs::read_to_string("/proc/loadavg") {
            if let Some(first) = loadavg.split_whitespace().next() {
                if let Ok(val) = first.parse::<f32>() {
                    let cores = std::thread::available_parallelism()
                        .map(|c| c.get())
                        .unwrap_or(1) as f32;
                    usage_pct = ((val / cores) * 100.0).clamp(0.0, 100.0) as u8;
                }
            }
        }

        let top_procs = Self::get_top_cpu_processes().await;

        CpuMetrics {
            usage_pct,
            temp_c,
            top_procs,
        }
    }

    pub async fn collect_gpu(card_id: &str, is_nvidia: bool, nv_index: u32) -> GpuMetrics {
        if is_nvidia {
            if let Ok(output) = Command::new("nvidia-smi")
                .args([
                    &format!("--id={}", nv_index),
                    "--query-gpu=utilization.gpu,temperature.gpu,power.draw,memory.free",
                    "--format=csv,noheader,nounits",
                ])
                .output()
                .await
            {
                let text = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = text.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 4 {
                    let u = parts[0].parse().unwrap_or(0);
                    let t = parts[1].parse().unwrap_or(0);
                    let p = parts[2].parse::<f32>().unwrap_or(0.0) as u32;
                    let v = parts[3].parse::<u32>().unwrap_or(0) / 1024;
                    return GpuMetrics {
                        name: format!("NVIDIA GPU {}", nv_index),
                        usage_pct: u,
                        temp_c: t,
                        power_w: p,
                        vram_free_gib: v,
                        top_procs: Vec::new(),
                    };
                }
            }
        }

        let base = format!("/sys/class/drm/{}/device", card_id);
        let usage_pct = fs::read_to_string(format!("{}/gpu_busy_percent", base))
            .ok()
            .and_then(|s| s.trim().parse::<u8>().ok())
            .unwrap_or(0);

        // Enumerate all hwmon entries under the card's device. The old code
        // hardcoded `hwmon0`, which on amdgpu/AMD setups does not exist;
        // amdgpu typically registers as hwmon2 or hwmon3.
        let mut temp_c: u32 = 0;
        let mut power_w: u32 = 0;
        if let Ok(entries) = fs::read_dir(format!("{}/hwmon", base)) {
            for entry in entries.flatten() {
                let hw = entry.path();
                if temp_c == 0 {
                    if let Ok(raw) = fs::read_to_string(hw.join("temp1_input")) {
                        if let Ok(v) = raw.trim().parse::<u32>() {
                            temp_c = v / 1000;
                        }
                    }
                }
                if power_w == 0 {
                    for fname in ["power1_average", "power1_input"] {
                        if let Ok(raw) = fs::read_to_string(hw.join(fname)) {
                            if let Ok(v) = raw.trim().parse::<u32>() {
                                power_w = v / 1_000_000;
                                break;
                            }
                        }
                    }
                }
                if temp_c > 0 && power_w > 0 { break; }
            }
        }

        let vram_free_gib = (|| {
            let total = fs::read_to_string(format!("{}/mem_info_vram_total", base))
                .ok()?
                .trim()
                .parse::<u64>()
                .ok()?;
            let used = fs::read_to_string(format!("{}/mem_info_vram_used", base))
                .ok()?
                .trim()
                .parse::<u64>()
                .ok()?;
            Some(((total.saturating_sub(used)) / (1024 * 1024 * 1024)) as u32)
        })()
        .unwrap_or(0);

        GpuMetrics {
            name: card_id.to_uppercase(),
            usage_pct,
            temp_c,
            power_w,
            vram_free_gib,
            top_procs: Vec::new(),
        }
    }

    pub async fn collect_ram() -> RamMetrics {
        let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let mut total_kb = 0.0;
        let mut avail_kb = 0.0;

        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            } else if line.starts_with("MemAvailable:") {
                avail_kb = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            }
        }

        let total_gib = total_kb / (1024.0 * 1024.0);
        let avail_gib = avail_kb / (1024.0 * 1024.0);

        let zram_orig = fs::read_to_string("/sys/block/zram0/orig_data_size")
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .unwrap_or(0.0);
        let zram_comp = fs::read_to_string("/sys/block/zram0/compr_data_size")
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .unwrap_or(0.0);

        let mut zram_ratio = 1.0;
        let mut zram_saved_gib = 0.0;
        if zram_comp > 0.0 && zram_orig > 0.0 {
            zram_ratio = zram_orig / zram_comp;
            zram_saved_gib = (zram_orig - zram_comp) / (1024.0 * 1024.0 * 1024.0);
        }

        let top_procs = Self::get_top_ram_processes().await;

        RamMetrics {
            total_gib,
            available_gib: avail_gib,
            effective_avail_gib: avail_gib + zram_saved_gib,
            zram_ratio,
            zram_saved_gib,
            top_procs,
        }
    }

    pub fn collect_battery() -> BatteryMetrics {
        let ps = Path::new("/sys/class/power_supply");
        if !ps.exists() {
            return BatteryMetrics::default();
        }

        let mut bat_dir = None;
        if let Ok(entries) = fs::read_dir(ps) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("BAT") {
                    bat_dir = Some(entry.path());
                    break;
                }
            }
        }

        let Some(b) = bat_dir else {
            return BatteryMetrics {
                has_battery: false,
                percent: 100,
                status: "Mains".into(),
                power_w: 0.0,
            };
        };

        let cap = fs::read_to_string(b.join("capacity"))
            .ok()
            .and_then(|s| s.trim().parse::<u8>().ok())
            .unwrap_or(0);

        let status = fs::read_to_string(b.join("status"))
            .unwrap_or_else(|_| "Unknown".into())
            .trim()
            .to_string();

        let power_now = fs::read_to_string(b.join("power_now"))
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .map(|p| p / 1_000_000.0)
            .unwrap_or(0.0);

        BatteryMetrics {
            has_battery: true,
            percent: cap,
            status,
            power_w: power_now,
        }
    }

    pub fn update_net(&mut self) -> NetMetrics {
        // Preferred interface: the one carrying the default route.
        let primary: Option<String> = (|| {
            let route = fs::read_to_string("/proc/net/route").ok()?;
            for line in route.lines().skip(1) {
                let p: Vec<&str> = line.split_whitespace().collect();
                if p.len() >= 2 && p[1] == "00000000" {
                    return Some(p[0].to_string());
                }
            }
            None
        })();

        let dev = fs::read_to_string("/proc/net/dev").unwrap_or_default();
        let mut chosen_iface = String::from("none");
        let mut chosen_rx: u64 = 0;
        let mut chosen_tx: u64 = 0;
        let mut fallback_rx: u64 = 0;
        let mut fallback_tx: u64 = 0;
        let mut fallback_iface = String::new();

        for line in dev.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 10 { continue; }
            let iface = parts[0].trim_end_matches(':');
            if iface == "lo" { continue; }
            let rx = parts[1].parse::<u64>().unwrap_or(0);
            let tx = parts[9].parse::<u64>().unwrap_or(0);

            if let Some(ref want) = primary {
                if iface == want {
                    chosen_iface = iface.to_string();
                    chosen_rx = rx;
                    chosen_tx = tx;
                    break;
                }
            }
            if rx > fallback_rx {
                fallback_rx = rx;
                fallback_tx = tx;
                fallback_iface = iface.to_string();
            }
        }
        if chosen_iface == "none" && !fallback_iface.is_empty() {
            chosen_iface = fallback_iface;
            chosen_rx = fallback_rx;
            chosen_tx = fallback_tx;
        }

        // If we're looking at a different interface than last time, seed the
        // deltas from this sample so the rate isn't computed against a
        // mismatched counter (which showed a huge spike on iface changes).
        if self.prev_iface != chosen_iface {
            self.prev_iface = chosen_iface.clone();
            self.prev_rx_bytes = chosen_rx;
            self.prev_tx_bytes = chosen_tx;
            self.prev_net_time = std::time::Instant::now();
            return NetMetrics {
                iface: chosen_iface,
                hw_type: "".into(),
                ip: "".into(),
                down_str: "0.0Kb".into(),
                up_str: "0.0Kb".into(),
                ping_str: "--".into(),
            };
        }

        let now = std::time::Instant::now();
        let dt = now.duration_since(self.prev_net_time).as_secs_f32().max(0.1);
        let rx_rate = (chosen_rx.saturating_sub(self.prev_rx_bytes) as f32 / dt) * 8.0;
        let tx_rate = (chosen_tx.saturating_sub(self.prev_tx_bytes) as f32 / dt) * 8.0;

        self.prev_rx_bytes = chosen_rx;
        self.prev_tx_bytes = chosen_tx;
        self.prev_net_time = now;

        let hw_type = if chosen_iface.starts_with("wl") {
            "WiFi"
        } else if chosen_iface.starts_with("ww") {
            "Cellular"
        } else if chosen_iface.starts_with("wg") {
            "WireGuard"
        } else {
            "Ethernet"
        };

        NetMetrics {
            iface: chosen_iface,
            hw_type: hw_type.to_string(),
            ip: "".into(),
            down_str: Self::format_bits(rx_rate),
            up_str: Self::format_bits(tx_rate),
            ping_str: "--".into(),
        }
    }

    fn format_bits(bps: f32) -> String {
        if bps >= 1_000_000_000.0 {
            format!("{:.1}Gb", bps / 1_000_000_000.0)
        } else if bps >= 1_000_000.0 {
            format!("{:.1}Mb", bps / 1_000_000.0)
        } else {
            format!("{:.1}Kb", (bps / 1_000.0).max(0.0))
        }
    }

    async fn get_top_cpu_processes() -> Vec<ProcessItem> {
        let Ok(out) = Command::new("ps")
            .args(["-eo", "pid,pcpu,comm", "--sort=-pcpu", "--no-headers"])
            .output()
            .await
        else {
            return Vec::new();
        };

        String::from_utf8_lossy(&out.stdout)
            .lines()
            .take(8)
            .filter_map(|l| {
                let parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() >= 3 {
                    let pid = parts[0].parse().ok()?;
                    let cpu = parts[1];
                    let name = parts[2..].join(" ");
                    Some(ProcessItem {
                        pid,
                        name,
                        metric: format!("{}%", cpu),
                        is_highlight: false,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    async fn get_top_ram_processes() -> Vec<ProcessItem> {
        let Ok(out) = Command::new("ps")
            .args(["-eo", "pid,rss,comm", "--sort=-rss", "--no-headers"])
            .output()
            .await
        else {
            return Vec::new();
        };

        String::from_utf8_lossy(&out.stdout)
            .lines()
            .take(8)
            .filter_map(|l| {
                let parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() >= 3 {
                    let pid = parts[0].parse().ok()?;
                    let rss_kb: u64 = parts[1].parse().ok()?;
                    let name = parts[2..].join(" ");
                    Some(ProcessItem {
                        pid,
                        name,
                        metric: format!("{} MiB", rss_kb / 1024),
                        is_highlight: false,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    pub async fn kill_process(pid: u32) -> Result<(), std::io::Error> {
        if pid <= 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Refusing to terminate PID <= 1",
            ));
        }
        Command::new("kill").args(["-9", &pid.to_string()]).status().await?;
        Ok(())
    }
}
