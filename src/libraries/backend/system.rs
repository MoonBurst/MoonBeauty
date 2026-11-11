#![allow(clippy::type_complexity)]
#![allow(clippy::empty_line_after_doc_comments)]
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
    pub vram_total_gib: u32,
    pub top_procs: Vec<ProcessItem>,
}

#[derive(Debug, Clone, Default)]
pub struct GpuInfo {
    pub card_id: String,
    pub pci_addr: String,
    pub name: String,
    pub is_nvidia: bool,
    pub nv_index: u32,
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
    pub top_procs: Vec<ProcessItem>,
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

fn read_cpu_temp() -> u32 {
    // Thermal zones first
    for i in 0..10 {
        let path = format!("/sys/class/thermal/thermal_zone{}/temp", i);
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(v) = raw.trim().parse::<u32>() {
                let t = if v > 1000 { v / 1000 } else { v };
                if t > 0 {
                    return t;
                }
            }
        }
    }
    // Fallback: k10temp / coretemp / zenpower
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let dir = entry.path();
            let name = fs::read_to_string(dir.join("name"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if name == "k10temp" || name == "coretemp" || name == "zenpower" {
                if let Ok(raw) = fs::read_to_string(dir.join("temp1_input")) {
                    if let Ok(v) = raw.trim().parse::<u32>() {
                        return v / 1000;
                    }
                }
            }
        }
    }
    0
}

pub struct SystemCollector {
    prev_bandwidth: std::collections::HashMap<u32, (u64, u64)>,
    prev_rx_bytes: u64,
    prev_tx_bytes: u64,
    prev_iface: String,
    prev_net_time: std::time::Instant,
}

impl Default for SystemCollector {
    fn default() -> Self {
        Self::new()
    }
}

fn short_gpu_name(full: &str) -> String {
    let s = full.trim();
    for p in ["Radeon ", "GeForce ", "NVIDIA ", "AMD ", "Intel "] {
        if let Some(rest) = s.strip_prefix(p) {
            return rest.to_string();
        }
    }
    s.to_string()
}

impl SystemCollector {
    pub fn new() -> Self {
        Self {
            prev_bandwidth: std::collections::HashMap::new(),
            prev_rx_bytes: 0,
            prev_tx_bytes: 0,
            prev_iface: String::new(),
            prev_net_time: std::time::Instant::now(),
        }
    }

    pub async fn collect_all() -> (CpuMetrics, RamMetrics) {
        let temp_c = read_cpu_temp();

        tokio::task::spawn_blocking(move || {
            use std::collections::HashMap;
            use std::sync::Mutex;
            static PREV_TOTAL: Mutex<Option<(u64, u64)>> = Mutex::new(None);
            static PREV_PROCS: Mutex<Option<HashMap<u32, u64>>> = Mutex::new(None);

            let is_first = PREV_TOTAL.lock().unwrap().is_none();
            let loops = if is_first { 2 } else { 1 };
            let mut usage_pct = 0;
            let mut top_procs_cpu = Vec::new();
            let mut procs_ram_raw = Vec::new();

            for i in 0..loops {
                if i == 1 {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }

                let stat_str = std::fs::read_to_string("/proc/stat").unwrap_or_default();
                let mut delta_total = 0;
                if let Some(line) = stat_str.lines().next() {
                    let parts: Vec<u64> = line
                        .split_whitespace()
                        .skip(1)
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    if parts.len() >= 4 {
                        let idle = parts[3] + parts.get(4).copied().unwrap_or(0);
                        let total = parts.iter().sum::<u64>();
                        let mut g = PREV_TOTAL.lock().unwrap();
                        if let Some((prev_idle, prev_total)) = *g {
                            delta_total = total.saturating_sub(prev_total);
                            let delta_idle = idle.saturating_sub(prev_idle);
                            if delta_total > 0 {
                                usage_pct = (((delta_total - delta_idle) as f64
                                    / delta_total as f64)
                                    * 100.0) as u8;
                            }
                        }
                        *g = Some((idle, total));
                    }
                }

                let mut next_procs = HashMap::new();
                let mut procs_cpu = Vec::new();
                let mut seen_ram = std::collections::HashSet::new();
                procs_ram_raw.clear();

                if let Ok(entries) = std::fs::read_dir("/proc") {
                    for entry in entries.flatten() {
                        if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() {
                            if let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) {
                                if let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')'))
                                {
                                    let name = &stat[open + 1..close];
                                    let rest = &stat[close + 2..];
                                    let fields: Vec<&str> = rest.split_whitespace().collect();

                                    // CPU
                                    if fields.len() > 12 {
                                        if let (Ok(utime), Ok(stime)) =
                                            (fields[11].parse::<u64>(), fields[12].parse::<u64>())
                                        {
                                            let proc_ticks = utime + stime;
                                            next_procs.insert(pid, proc_ticks);

                                            let g_procs = PREV_PROCS.lock().unwrap();
                                            if let Some(prev_map) = g_procs.as_ref() {
                                                if let Some(&prev_ticks) = prev_map.get(&pid) {
                                                    let delta_proc =
                                                        proc_ticks.saturating_sub(prev_ticks);
                                                    let cpu_pct = if delta_total > 0 {
                                                        (delta_proc as f64 / delta_total as f64)
                                                            * 100.0
                                                    } else {
                                                        0.0
                                                    };
                                                    procs_cpu.push((
                                                        pid,
                                                        name.to_string(),
                                                        cpu_pct as f32,
                                                    ));
                                                }
                                            }
                                        }
                                    }

                                    // RAM (read statm on the final loop)
                                    if i == loops - 1 {
                                        if let Ok(statm) =
                                            std::fs::read_to_string(entry.path().join("statm"))
                                        {
                                            if let Some(rss_str) = statm.split_whitespace().nth(1) {
                                                if let Ok(rss_pages) = rss_str.parse::<u64>() {
                                                    let rss_bytes = rss_pages * 4096; // Assuming 4KB pages
                                                    // Only dedupe Firefox/Chrome-style
                                                    // renderer subprocesses (name has
                                                    // " #N" suffix). Ordinary processes
                                                    // like "moonbeauty" can legitimately
                                                    // run as multiple PIDs (bar +
                                                    // notif-surface) and should all
                                                    // appear in the tooltip.
                                                    if !name.contains(" #") {
                                                        procs_ram_raw.push((
                                                            pid,
                                                            name.to_string(),
                                                            rss_bytes,
                                                        ));
                                                    } else {
                                                        let base_name = name
                                                            .split(" #")
                                                            .next()
                                                            .unwrap_or("")
                                                            .to_string();
                                                        if seen_ram.insert(base_name) {
                                                            procs_ram_raw.push((
                                                                pid,
                                                                name.to_string(),
                                                                rss_bytes,
                                                            ));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                *PREV_PROCS.lock().unwrap() = Some(next_procs);

                if i == loops - 1 {
                    procs_cpu
                        .sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
                    top_procs_cpu = procs_cpu
                        .into_iter()
                        .take(15)
                        .map(|(pid, name, pct)| ProcessItem {
                            pid,
                            name,
                            metric: format!("{:.1}%", pct),
                            is_highlight: false,
                        })
                        .collect();
                }
            }

            let cpu = CpuMetrics {
                usage_pct,
                temp_c,
                top_procs: top_procs_cpu,
            };

            // System Totals from /proc/meminfo
            let mut total_kb = 0;
            let mut avail_kb = 0;
            if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
                for line in meminfo.lines() {
                    if line.starts_with("MemTotal:") {
                        total_kb = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(0);
                    } else if line.starts_with("MemAvailable:") {
                        avail_kb = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(0);
                    }
                }
            }
            let total_gib = total_kb as f32 / (1024.0 * 1024.0);
            let avail_gib = avail_kb as f32 / (1024.0 * 1024.0);

            // ZRAM
            let zram_orig = std::fs::read_to_string("/sys/block/zram0/orig_data_size")
                .ok()
                .and_then(|s| s.trim().parse::<f32>().ok())
                .unwrap_or(0.0);
            let zram_comp = std::fs::read_to_string("/sys/block/zram0/compr_data_size")
                .ok()
                .and_then(|s| s.trim().parse::<f32>().ok())
                .unwrap_or(0.0);
            let (zram_ratio, zram_saved_gib) = if zram_comp > 0.0 && zram_orig > 0.0 {
                (
                    zram_orig / zram_comp,
                    (zram_orig - zram_comp) / (1024.0 * 1024.0 * 1024.0),
                )
            } else {
                (1.0, 0.0)
            };

            procs_ram_raw.sort_by_key(|x| std::cmp::Reverse(x.2));
            let top_procs_ram = procs_ram_raw
                .into_iter()
                .take(15)
                .map(|(pid, name, bytes)| ProcessItem {
                    pid,
                    name,
                    metric: format!("{} MiB", bytes / (1024 * 1024)),
                    is_highlight: false,
                })
                .collect();

            let ram = RamMetrics {
                total_gib,
                available_gib: avail_gib,
                effective_avail_gib: avail_gib + zram_saved_gib,
                zram_ratio,
                zram_saved_gib,
                top_procs: top_procs_ram,
            };

            (cpu, ram)
        })
        .await
        .unwrap_or_default()
    }

    pub async fn collect_gpu(card_id: &str, is_nvidia: bool, nv_index: u32) -> GpuMetrics {
        if is_nvidia {
            if let Ok(output) = Command::new("nvidia-smi")
                .args([
                    &format!("--id={}", nv_index),
                    "--query-gpu=utilization.gpu,temperature.gpu,power.draw,memory.free,memory.total",
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
                    let v_free = parts[3].parse::<u32>().unwrap_or(0) / 1024;
                    let v_total = parts.get(4).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0) / 1024;
                    return GpuMetrics {
                        name: format!("NVIDIA GPU {}", nv_index),
                        usage_pct: u,
                        temp_c: t,
                        power_w: p,
                        vram_free_gib: v_free,
                        vram_total_gib: v_total,
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
                if temp_c > 0 && power_w > 0 {
                    break;
                }
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

        let vram_total_gib = fs::read_to_string(format!("{}/mem_info_vram_total", base))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|v| (v / (1024 * 1024 * 1024)) as u32)
            .unwrap_or(0);

        let top_procs = Self::get_top_gpu_processes(card_id).await;

        GpuMetrics {
            name: card_id.to_uppercase(),
            usage_pct,
            temp_c,
            power_w,
            vram_free_gib,
            vram_total_gib,
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
        let mut chosen_iface = String::from("none");
        let mut chosen_rx = 0;
        let mut chosen_tx = 0;

        // Read directly from /proc/net/dev to bypass sysinfo 0.31 bugs
        if let Ok(dev) = fs::read_to_string("/proc/net/dev") {
            for line in dev.lines().skip(2) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 10 {
                    let name = parts[0].trim_end_matches(':');
                    if name == "lo"
                        || name.starts_with("docker")
                        || name.starts_with("veth")
                        || name.starts_with("br-")
                    {
                        continue;
                    }
                    let rx: u64 = parts[1].parse().unwrap_or(0);
                    let tx: u64 = parts[9].parse().unwrap_or(0);

                    // Pick the active interface (the one with the most traffic)
                    if chosen_iface == "none" || rx > chosen_rx {
                        chosen_iface = name.to_string();
                        chosen_rx = rx;
                        chosen_tx = tx;
                    }
                }
            }
        }

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
                top_procs: Vec::new(),
            };
        }

        let now = std::time::Instant::now();
        let dt = now
            .duration_since(self.prev_net_time)
            .as_secs_f32()
            .max(0.1);
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

        // Keep the cached ping
        let ping_str = {
            use std::sync::{Mutex, OnceLock};
            use std::time::Instant;
            static PING_CACHE: OnceLock<Mutex<Option<(Instant, String)>>> = OnceLock::new();
            let cache = PING_CACHE.get_or_init(|| Mutex::new(None));
            let mut g = cache.lock().unwrap();
            if g.as_ref()
                .map_or(true, |(t, _)| t.elapsed().as_secs() >= 15)
            {
                let mut new_ping = "OFFLINE".to_string();
                if let Ok(addr) = "1.1.1.1:80".parse() {
                    let t0 = std::time::Instant::now();
                    // #3: 400ms -> 150ms. A reachable host responds in
                    // <50ms; >150ms is functionally offline for UI
                    // feedback. Cuts worst-case blocking 60% even
                    // before spawn_blocking.
                    if std::net::TcpStream::connect_timeout(
                        &addr,
                        std::time::Duration::from_millis(150),
                    )
                    .is_ok()
                    {
                        new_ping = format!("{}ms", t0.elapsed().as_millis());
                    }
                }
                *g = Some((Instant::now(), new_ping));
            }
            format!("{:>6}", g.as_ref().unwrap().1)
        };

        NetMetrics {
            iface: chosen_iface,
            hw_type: hw_type.to_string(),
            ip: "".into(),
            down_str: Self::format_bits(rx_rate),
            up_str: Self::format_bits(tx_rate),
            ping_str,
            top_procs: Self::get_top_net_procs(),
        }
    }

    pub fn get_top_net_procs_bandwidth(&mut self) -> Vec<ProcessItem> {
        let out = std::process::Command::new("ss").args(["-tunapi"]).output();
        let Ok(out) = out else {
            return Vec::new();
        };
        let text = String::from_utf8_lossy(&out.stdout);

        let mut current_snapshot: std::collections::HashMap<u32, (u64, u64)> =
            std::collections::HashMap::new();
        let mut pid_to_name = std::collections::HashMap::new();
        let mut current_pid: Option<u32> = None;

        for line in text.lines() {
            if let Some(pid_idx) = line.find("pid=") {
                let rest = &line[pid_idx + 4..];
                if let Some(end) = rest.find(',') {
                    if let Ok(pid) = rest[..end].parse::<u32>() {
                        current_pid = Some(pid);
                        let mut name = "unknown".to_string();
                        if let Some(u_idx) = line.find("users:((\"") {
                            let n_rest = &line[u_idx + 9..];
                            if let Some(n_end) = n_rest.find("\"") {
                                name = n_rest[..n_end].to_string();
                            }
                        }
                        pid_to_name.insert(pid, name);
                    }
                }
            }

            if let Some(pid) = current_pid {
                let mut sent = 0;
                let mut recv = 0;
                if let Some(s_idx) = line.find("bytes_sent:") {
                    let rest = &line[s_idx + 11..];
                    if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                        sent = rest[..end].parse::<u64>().unwrap_or(0);
                    } else {
                        sent = rest.parse::<u64>().unwrap_or(0);
                    }
                }
                if let Some(r_idx) = line.find("bytes_received:") {
                    let rest = &line[r_idx + 15..];
                    if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                        recv = rest[..end].parse::<u64>().unwrap_or(0);
                    } else {
                        recv = rest.parse::<u64>().unwrap_or(0);
                    }
                }
                if sent > 0 || recv > 0 {
                    let entry = current_snapshot.entry(pid).or_insert((0, 0));
                    entry.0 += recv;
                    entry.1 += sent;
                }
            }
        }

        let mut procs = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for (&pid, &(recv, sent)) in &current_snapshot {
            let (delta_recv, delta_sent) =
                if let Some((prev_recv, prev_sent)) = self.prev_bandwidth.get(&pid) {
                    (
                        recv.saturating_sub(*prev_recv),
                        sent.saturating_sub(*prev_sent),
                    )
                } else {
                    (0, 0)
                };

            let name = pid_to_name.get(&pid).cloned().unwrap_or_default();
            let base_name = name.split(" #").next().unwrap_or("").to_string();

            if seen.insert(base_name) {
                let rx_rate = delta_recv as f32 * 8.0; // bits
                let tx_rate = delta_sent as f32 * 8.0;
                procs.push(ProcessItem {
                    pid,
                    name,
                    metric: format!(
                        "▼{}/s ▲{}/s",
                        Self::format_bits(rx_rate),
                        Self::format_bits(tx_rate)
                    ),
                    is_highlight: false,
                });
            }
        }

        self.prev_bandwidth = current_snapshot;

        procs.sort_by(|a, b| {
            let parse = |s: &str| -> f32 {
                let parts: Vec<&str> = s.split_whitespace().collect();
                let p1 = parts
                    .get(0)
                    .unwrap_or(&"0")
                    .trim_start_matches('▼')
                    .trim_end_matches("/s");
                let p2 = parts
                    .get(1)
                    .unwrap_or(&"0")
                    .trim_start_matches('▲')
                    .trim_end_matches("/s");
                let parse_val = |v: &str| -> f32 {
                    if v.ends_with("Mb") {
                        v.trim_end_matches("Mb").parse::<f32>().unwrap_or(0.0) * 1_000_000.0
                    } else if v.ends_with("Kb") {
                        v.trim_end_matches("Kb").parse::<f32>().unwrap_or(0.0) * 1_000.0
                    } else {
                        v.trim_end_matches("b").parse::<f32>().unwrap_or(0.0)
                    }
                };
                parse_val(p1) + parse_val(p2)
            };
            let va = parse(&a.metric);
            let vb = parse(&b.metric);
            vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
        });
        procs
    }

    pub fn get_top_net_procs() -> Vec<ProcessItem> {
        // Cached for 15s. The `ss` subprocess takes 20-50ms and this is
        // called synchronously on the UI thread every hardware tick — which
        // is 2s by default. Throttling removes real UI jank.
        use std::sync::{Mutex, OnceLock};
        use std::time::Instant;
        static CACHE: OnceLock<Mutex<(Option<Instant>, Vec<ProcessItem>)>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new((None, Vec::new())));

        {
            let g = cache.lock().unwrap();
            if let Some(t) = g.0 {
                if t.elapsed().as_secs() < 3 {
                    return g.1.clone();
                }
            }
        }

        let out = std::process::Command::new("ss").args(["-tunap"]).output();
        let Ok(out) = out else {
            // On transient failure, return the previous result rather than empty.
            return cache.lock().unwrap().1.clone();
        };
        let text = String::from_utf8_lossy(&out.stdout);
        let mut procs = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for line in text.lines().skip(1) {
            if let Some(idx) = line.find("pid=") {
                let rest = &line[idx + 4..];
                if let Some(end) = rest.find(',') {
                    if let Ok(pid) = rest[..end].parse::<u32>() {
                        let mut name = "unknown".to_string();
                        if let Some(u_idx) = line.find("users:((\"") {
                            let n_rest = &line[u_idx + 9..];
                            if let Some(n_end) = n_rest.find("\"") {
                                name = n_rest[..n_end].to_string();
                            }
                        }

                        // Deduplicate by process name instead of PID
                        let base_name = name.split(" #").next().unwrap_or("").to_string();
                        if seen.insert(base_name) {
                            procs.push(ProcessItem {
                                pid,
                                name,
                                metric: "net".into(), // Changed from "active"
                                is_highlight: false,
                            });
                        }
                    }
                }
            }
        }
        procs.truncate(15);

        {
            let mut g = cache.lock().unwrap();
            g.0 = Some(Instant::now());
            g.1 = procs.clone();
        }
        procs
    }

    pub fn format_bits(bps: f32) -> String {
        if bps >= 1_000_000_000.0 {
            format!("{:>5.1}Gb", bps / 1_000_000_000.0)
        } else if bps >= 1_000_000.0 {
            format!("{:>5.1}Mb", bps / 1_000_000.0)
        } else {
            format!("{:>5.1}Kb", (bps / 1_000.0).max(0.0))
        }
    }

    pub async fn kill_process(pid: u32) -> Result<(), std::io::Error> {
        if pid <= 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Refusing to terminate PID <= 1",
            ));
        }
        Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status()
            .await?;
        Ok(())
    }
}

impl SystemCollector {
    /// Enumerate GPUs via /sys/class/drm (fallback when lspci is unavailable).
    fn list_gpus_via_sysfs() -> Vec<GpuInfo> {
        let mut gpus = Vec::new();
        let drm_dir = match std::fs::read_dir("/sys/class/drm") {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[gpu-detect] cannot read /sys/class/drm: {}", e);
                return gpus;
            }
        };
        let mut seen_pci: std::collections::HashSet<String> = std::collections::HashSet::new();
        for entry in drm_dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }
            let card_id = name.clone();
            let uevent_path = entry.path().join("device/uevent");
            let uevent = match std::fs::read_to_string(&uevent_path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let mut pci_id = String::new();
            let mut pci_addr = String::new();
            for line in uevent.lines() {
                if let Some(v) = line.strip_prefix("PCI_ID=") {
                    pci_id = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("PCI_SLOT_NAME=") {
                    pci_addr = v.trim().to_string();
                }
            }
            if pci_id.is_empty() || !seen_pci.insert(pci_addr.clone()) {
                continue;
            }
            let is_nvidia = pci_id.to_lowercase().starts_with("10de:");
            let name = pci_id_to_name(&pci_id)
                .map(short_gpu_name)
                .unwrap_or_else(|| format!("GPU {}", card_id));
            let nv_idx: u32 = gpus.iter().filter(|g| g.is_nvidia).count() as u32;
            eprintln!(
                "[gpu-detect] sysfs: {} pci={} pci_id={} name={}",
                card_id, pci_addr, pci_id, name
            );
            gpus.push(GpuInfo {
                card_id,
                pci_addr,
                name,
                is_nvidia,
                nv_index: if is_nvidia { nv_idx } else { 0 },
            });
        }
        gpus.sort_by(|a, b| a.card_id.cmp(&b.card_id));
        gpus
    }

    pub fn list_gpus_sync() -> Vec<GpuInfo> {
        let mut gpus: Vec<GpuInfo> = Vec::new();
        let mut seen_pci = std::collections::HashSet::new();

        if let Ok(out) = std::process::Command::new("lspci")
            .args(["-D", "-mm"])
            .output()
        {
            if out.status.success() {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    if !(line.contains("VGA compatible controller")
                        || line.contains("3D controller")
                        || line.contains("Display controller"))
                    {
                        continue;
                    }
                    let parts: Vec<&str> = line.split('"').collect();
                    if parts.len() < 6 {
                        continue;
                    }
                    let pci_addr = parts[0].trim().to_string();
                    if !seen_pci.insert(pci_addr.clone()) {
                        continue;
                    }
                    let vendor = parts[3].trim();
                    let device = parts[5].trim();
                    let is_nv = vendor.contains("NVIDIA");

                    // Precompute nv_idx from the current state of gpus
                    // (borrow ends before we mutate gpus below).
                    let nv_idx: u32 = gpus.iter().filter(|g| g.is_nvidia).count() as u32;

                    let card_id = find_drm_card_for_pci(&pci_addr).unwrap_or_else(|| {
                        if is_nv {
                            format!("nvidia{}", nv_idx)
                        } else {
                            pci_addr.clone()
                        }
                    });

                    let name = short_gpu_name(
                        &pci_id_to_name_from_addr(&pci_addr)
                            .unwrap_or_else(|| clean_gpu_name(device)),
                    );

                    gpus.push(GpuInfo {
                        card_id,
                        pci_addr,
                        name,
                        is_nvidia: is_nv,
                        nv_index: if is_nv { nv_idx } else { 0 },
                    });
                }
            }
        }

        if gpus.is_empty() {
            eprintln!("[gpu-detect] lspci returned no GPUs; falling back to /sys/class/drm");
            gpus = Self::list_gpus_via_sysfs();
        }

        if gpus.is_empty() {
            gpus.push(GpuInfo {
                card_id: "card0".to_string(),
                pci_addr: String::new(),
                name: "GPU".to_string(),
                is_nvidia: false,
                nv_index: 0,
            });
        }

        gpus
    }

    pub async fn collect_gpu_by_info(info: &GpuInfo) -> GpuMetrics {
        let mut m = Self::collect_gpu(&info.card_id, info.is_nvidia, info.nv_index).await;
        if !info.name.is_empty() && info.name != "GPU" && !info.name.starts_with("CARD") {
            m.name = info.name.clone();
        }
        m
    }

    /// Parse /proc/*/fdinfo for DRM clients whose `drm-pdev` (PCI address)
    /// matches this card, then sum `drm-memory-vram` per PID.
    ///
    /// Uses drm-pdev filtering (works per-card) instead of the clients file,
    /// which does not exist on many modern kernels. Also falls back to
    /// nvidia-smi compute apps for NVIDIA GPUs.
    async fn get_top_gpu_processes(card_id: &str) -> Vec<ProcessItem> {
        if card_id.starts_with("nvidia") {
            return Self::get_nvidia_compute_procs().await;
        }

        // Resolve this card's PCI address.
        let pci_addr = std::fs::read_link(format!("/sys/class/drm/{}/device", card_id))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .or_else(|| {
                fs::read_to_string(format!("/sys/class/drm/{}/device/uevent", card_id))
                    .ok()
                    .and_then(|s| {
                        s.lines().find_map(|l| {
                            l.strip_prefix("PCI_SLOT_NAME=")
                                .map(|v| v.trim().to_string())
                        })
                    })
            });
        let Some(pci_addr) = pci_addr else {
            return Vec::new();
        };

        let Ok(proc_dir) = fs::read_dir("/proc") else {
            return Vec::new();
        };

        let mut users: std::collections::HashMap<u32, (String, u64, u64)> =
            std::collections::HashMap::new();
        let mut any_vram_reported = false;

        for entry in proc_dir.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(fds) = fs::read_dir(format!("/proc/{}/fdinfo", pid)) else {
                continue;
            };

            let mut clients: std::collections::HashMap<String, (u64, u64)> =
                std::collections::HashMap::new();

            for fd in fds.flatten() {
                let Ok(content) = fs::read_to_string(fd.path()) else {
                    continue;
                };
                let mut cid: Option<String> = None;
                let mut vram_kib = 0u64;
                let mut pdev = String::new();
                let mut is_amd_or_intel = false;
                let mut engine_ns = 0u64;

                for line in content.lines() {
                    if let Some(v) = line.strip_prefix("drm-client-id:") {
                        cid = Some(v.trim().to_string());
                    } else if let Some(v) = line.strip_prefix("drm-memory-vram:") {
                        vram_kib = v
                            .split_whitespace()
                            .next()
                            .and_then(|n| n.parse::<u64>().ok())
                            .unwrap_or(0);
                        if vram_kib > 0 {
                            any_vram_reported = true;
                        }
                    } else if let Some(v) = line.strip_prefix("drm-pdev:") {
                        pdev = v.trim().to_string();
                    } else if let Some(v) = line.strip_prefix("drm-driver:") {
                        let d = v.trim();
                        is_amd_or_intel = d == "amdgpu" || d == "i915" || d == "xe";
                    } else if let Some(v) = line.strip_prefix("drm-engine-gfx:") {
                        if let Some(ns_str) = v.split_whitespace().next() {
                            if let Ok(ns) = ns_str.parse::<u64>() { engine_ns += ns; }
                        }
                    } else if let Some(v) = line.strip_prefix("drm-engine-compute:") {
                        if let Some(ns_str) = v.split_whitespace().next() {
                            if let Ok(ns) = ns_str.parse::<u64>() { engine_ns += ns; }
                        }
                    }
                }

                if !is_amd_or_intel {
                    continue;
                }
                if pdev != pci_addr && !pdev.ends_with(&pci_addr) && !pci_addr.ends_with(&pdev) {
                    continue;
                }
                
                if let Some(c) = cid {
                    let e = clients.entry(c).or_insert((0, 0));
                    if vram_kib > e.0 {
                        e.0 = vram_kib;
                    }
                    if engine_ns > e.1 {
                        e.1 = engine_ns;
                    }
                }
            }

            if !clients.is_empty() {
                let total_kib: u64 = clients.values().map(|(v, _)| *v).sum();
                let total_engine_ns: u64 = clients.values().map(|(_, e)| *e).sum();
                let comm = fs::read_to_string(format!("/proc/{}/comm", pid))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| format!("pid {}", pid));
                users.insert(pid, (comm, total_kib, total_engine_ns));
            }
        }

        let mut procs: Vec<_> = users.into_iter().collect();
        procs.sort_by_key(|(_, (_, mem, _))| std::cmp::Reverse(*mem));

        static PREV_ENGINE_TIMES: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u32, (std::time::Instant, u64)>>> = std::sync::OnceLock::new();
        let engine_cache = PREV_ENGINE_TIMES.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
        let mut cache_lock = engine_cache.lock().unwrap();
        let now = std::time::Instant::now();

        let result: Vec<ProcessItem> = procs
            .into_iter()
            .filter_map(|(pid, (name, kib, ns))| {
                let pct = if let Some((prev_time, prev_ns)) = cache_lock.get(&pid) {
                    let dt = now.duration_since(*prev_time).as_nanos() as f64;
                    let dns = ns.saturating_sub(*prev_ns) as f64;
                    if dt > 0.0 && ns > *prev_ns {
                        (dns / dt * 100.0).clamp(0.0, 100.0)
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                cache_lock.insert(pid, (now, ns));

                let metric = if ns > 0 {
                    format!("{:.1}% · {}M", pct, kib / 1024)
                } else if any_vram_reported {
                    format!("{} MiB", kib / 1024)
                } else {
                    "—".to_string()
                };

                if pct < 0.1 && kib < 1024 * 50 {
                    return None; // Filter out idle processes using <50MB VRAM
                }

                Some(ProcessItem {
                    pid,
                    name,
                    metric,
                    is_highlight: false,
                })
            })
            .take(15)
            .collect();

        result
    }

    /// Enumerate NVIDIA compute-app processes via nvidia-smi.
    async fn get_nvidia_compute_procs() -> Vec<ProcessItem> {
        let Ok(out) = tokio::process::Command::new("nvidia-smi")
            .args([
                "--query-compute-apps=pid,process_name,used_memory",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .await
        else {
            return Vec::new();
        };
        if !out.status.success() {
            return Vec::new();
        }
        let mut procs = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
            if parts.len() < 3 {
                continue;
            }
            let Ok(pid) = parts[0].parse::<u32>() else {
                continue;
            };
            let name = parts[1].rsplit('/').next().unwrap_or(parts[1]).to_string();
            let mem_mib = parts[2].parse::<u64>().unwrap_or(0);
            procs.push(ProcessItem {
                pid,
                name,
                metric: format!("{} MiB", mem_mib),
                is_highlight: false,
            });
        }
        procs.sort_by(|a, b| {
            let na = a
                .metric
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let nb = b
                .metric
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            nb.cmp(&na)
        });
        procs.truncate(15);
        procs
    }
}

fn clean_gpu_name(raw: &str) -> String {
    // lspci -vmm can output "Navi 31 [Radeon RX 7900 XT/7900 XTX/7900 GRE/7900M]"
    // for cards that share a PCI ID. In that case there is no way to know which
    // model without the subsystem ID, so we fall back to the codename.
    let t = raw.trim();
    if let Some(start) = t.rfind('[') {
        if let Some(end_rel) = t[start..].find(']') {
            let inner = t[start + 1..start + end_rel].trim();
            if inner.contains('/') {
                return t[..start].trim().to_string();
            }
            if !inner.is_empty() {
                return inner.to_string();
            }
        }
    }
    t.to_string()
}

// ── GPU name cache ──────────────────────────────────────────────────
// Cache the lspci-resolved model names so we only spawn lspci once per
// card, not on every hardware poll.

fn resolve_gpu_name_sync_cached(card_id: &str) -> String {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    {
        if let Ok(g) = cache.lock() {
            if let Some(n) = g.get(card_id) {
                return n.clone();
            }
        }
    }
    let gpus = SystemCollector::list_gpus_sync();
    let mut result = card_id.to_uppercase();
    if let Ok(mut g) = cache.lock() {
        for info in gpus {
            g.insert(info.card_id.clone(), info.name.clone());
        }
        if let Some(n) = g.get(card_id) {
            result = n.clone();
        }
    }
    result
}

/// Find the /sys/class/drm/cardN node that owns a given PCI address.
fn find_drm_card_for_pci(pci_addr: &str) -> Option<String> {
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let n = entry.file_name().to_string_lossy().to_string();
            if n.starts_with("card") && !n.contains('-') {
                if let Ok(link) = std::fs::read_link(format!("/sys/class/drm/{}/device", n)) {
                    if link.to_string_lossy().contains(pci_addr) {
                        return Some(n);
                    }
                }
            }
        }
    }
    None
}

/// Look up a card's PCI_ID via its DRM node, then map through pci_id_to_name.
fn pci_id_to_name_from_addr(pci_addr: &str) -> Option<String> {
    let card_id = find_drm_card_for_pci(pci_addr)?;
    let uevent =
        std::fs::read_to_string(format!("/sys/class/drm/{}/device/uevent", card_id)).ok()?;
    let pci_id = uevent
        .lines()
        .find_map(|l| l.strip_prefix("PCI_ID="))?
        .trim();
    pci_id_to_name(pci_id).map(|s| s.to_string())
}

/// Curated map of PCI IDs → exact marketing names.
/// Extend as needed. Unknown IDs fall back to lspci output.
fn pci_id_to_name(id: &str) -> Option<&'static str> {
    match id.to_lowercase().as_str() {
        // AMD RDNA3 (7000 series)
        "1002:744c" => Some("Radeon RX 7900 XTX"),
        "1002:744a" => Some("Radeon RX 7900 XT"),
        "1002:7448" => Some("Radeon RX 7900 GRE"),
        "1002:7480" => Some("Radeon RX 7800 XT"),
        "1002:747e" => Some("Radeon RX 7800 XT"),
        "1002:7470" => Some("Radeon RX 7700 XT"),
        "1002:7483" => Some("Radeon RX 7600 XT"),
        "1002:7489" => Some("Radeon RX 7600"),
        // AMD RDNA2 (6000 series)
        "1002:73bf" => Some("Radeon RX 6900 XT"),
        "1002:73af" => Some("Radeon RX 6800 XT"),
        "1002:73a5" => Some("Radeon RX 6800"),
        "1002:73df" => Some("Radeon RX 6700 XT"),
        "1002:73ff" => Some("Radeon RX 6600 XT"),
        "1002:73ef" => Some("Radeon RX 6650 XT"),
        // AMD RDNA2 mobile / low-end (Navi 24)
        "1002:7413" => Some("Radeon RX 6500 XT"),
        "1002:743f" => Some("Radeon RX 6400"),
        "1002:7422" => Some("Radeon RX 6500M"),
        "1002:7424" => Some("Radeon RX 6300M"),
        // AMD RDNA (5000 series)
        "1002:731f" => Some("Radeon RX 5700 XT"),
        // AMD Polaris
        "1002:67df" => Some("Radeon RX 580"),
        // NVIDIA RTX 40 series
        "10de:2684" => Some("GeForce RTX 4090"),
        "10de:2704" => Some("GeForce RTX 4080"),
        "10de:2685" => Some("GeForce RTX 4080 SUPER"),
        "10de:2782" => Some("GeForce RTX 4070 Ti"),
        "10de:2786" => Some("GeForce RTX 4070 SUPER"),
        "10de:2783" => Some("GeForce RTX 4070"),
        // NVIDIA RTX 30 series
        "10de:2206" => Some("GeForce RTX 3080"),
        "10de:2204" => Some("GeForce RTX 3090"),
        "10de:2484" => Some("GeForce RTX 3070"),
        "10de:2486" => Some("GeForce RTX 3060 Ti"),
        "10de:2504" => Some("GeForce RTX 3060"),
        // NVIDIA RTX 20 series
        "10de:1e87" => Some("GeForce RTX 2080"),
        _ => None,
    }
}

impl SystemCollector {
    /// Collect GPU metrics for a card_id like "card0" or "nvidia0".
    /// The display name is resolved via lspci (cached after first lookup).
    pub async fn collect_gpu_by_card_id(card_id: &str) -> GpuMetrics {
        use std::sync::{Mutex, OnceLock};
        use std::time::Instant;
        static CACHE: OnceLock<Mutex<std::collections::HashMap<String, (Instant, GpuMetrics)>>> =
            OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));

        {
            let g = cache.lock().unwrap();
            if let Some((t, m)) = g.get(card_id) {
                if t.elapsed().as_secs() < 5 {
                    return m.clone();
                }
            }
        }
        let (is_nv, nv_idx) = if let Some(idx_str) = card_id.strip_prefix("nvidia") {
            (true, idx_str.parse::<u32>().unwrap_or(0))
        } else {
            (false, 0)
        };
        let mut m = Self::collect_gpu(card_id, is_nv, nv_idx).await;
        m.name = resolve_gpu_name_sync_cached(card_id);
        {
            let mut g = cache.lock().unwrap();
            g.insert(card_id.to_string(), (Instant::now(), m.clone()));
        }
        m
    }
}
