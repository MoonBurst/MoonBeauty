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

pub struct SystemCollector {
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
    let mut s = full.to_string();
    for p in ["Radeon ", "GeForce ", "AMD ", "NVIDIA "] {
        if s.starts_with(p) {
            s = s[p.len()..].to_string();
        }
    }
    for suf in [" XTX", " XT", " GRE", " Ti", " SUPER", " Super", " Laptop", " Mobile", " M"] {
        if s.ends_with(suf) {
            s = s[..s.len() - suf.len()].to_string();
        }
    }
    s.replace(" ", "")
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

        let read_stat = || {
            let stat = fs::read_to_string("/proc/stat").unwrap_or_default();
            let line = stat.lines().next().unwrap_or("");
            let parts: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
            if parts.len() >= 4 {
                let idle = parts[3] + parts[4]; // idle + iowait
                let total: u64 = parts.iter().sum();
                (idle, total)
            } else { (0, 0) }
        };
        let (i1, t1) = read_stat();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let (i2, t2) = read_stat();
        let usage_pct = if t2 > t1 {
            let dt = t2 - t1;
            let di = i2 - i1;
            (100.0 * (dt.saturating_sub(di)) as f64 / dt as f64).clamp(0.0, 100.0) as u8
        } else {
            0
        };

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
                top_procs: Vec::new(),
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

        // Lightweight TCP ping to measure latency without requiring root/ICMP.
        // Connects to Cloudflare DNS (1.1.1.1:80) with a 400ms timeout.
        let mut ping_str = "OFFLINE".to_string();
        if let Ok(addr) = "1.1.1.1:80".parse() {
            let start = std::time::Instant::now();
            if std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(400)).is_ok() {
                let ms = start.elapsed().as_millis();
                // Show only update if it changes drastically to avoid jitter? No, just show ms.
                ping_str = format!("{}ms", ms);
            }
        }

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

    pub fn get_top_net_procs() -> Vec<ProcessItem> {
        let out = std::process::Command::new("ss").args(["-tunap"]).output();
        let Ok(out) = out else { return Vec::new(); };
        let text = String::from_utf8_lossy(&out.stdout);
        let mut procs = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for line in text.lines().skip(1) {
            if let Some(idx) = line.find("pid=") {
                let rest = &line[idx+4..];
                if let Some(end) = rest.find(',') {
                    if let Ok(pid) = rest[..end].parse::<u32>() {
                        if seen.insert(pid) {
                            let mut name = "unknown".to_string();
                            if let Some(u_idx) = line.find("users:((\"") {
                                let n_rest = &line[u_idx+9..];
                                if let Some(n_end) = n_rest.find("\"") {
                                    name = n_rest[..n_end].to_string();
                                }
                            }
                            procs.push(ProcessItem { pid, name, metric: "active".into(), is_highlight: false });
                        }
                    }
                }
            }
        }
        procs.truncate(15);
        procs
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
            .take(15)
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
            .take(15)
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


impl SystemCollector {
    /// Enumerate all GPUs on the system (NVIDIA via nvidia-smi, others via /sys/class/drm).
    /// `card_id` for NVIDIA entries is a synthetic "nvidia0", "nvidia1", ...

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
                    let nv_idx: u32 = gpus
                        .iter()
                        .filter(|g| g.is_nvidia)
                        .count() as u32;

                    let card_id = find_drm_card_for_pci(&pci_addr).unwrap_or_else(|| {
                        if is_nv {
                            format!("nvidia{}", nv_idx)
                        } else {
                            pci_addr.clone()
                        }
                    });

                    let name = short_gpu_name(&pci_id_to_name_from_addr(&pci_addr)
                        .unwrap_or_else(|| clean_gpu_name(device)));

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

        // Resolve this card's PCI address. Try the symlink first (most reliable),
        // fall back to uevent.
        let pci_addr = std::fs::read_link(format!("/sys/class/drm/{}/device", card_id))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .or_else(|| {
                fs::read_to_string(format!("/sys/class/drm/{}/device/uevent", card_id))
                    .ok()
                    .and_then(|s| {
                        s.lines().find_map(|l| {
                            l.strip_prefix("PCI_SLOT_NAME=").map(|v| v.trim().to_string())
                        })
                    })
            });
        let Some(pci_addr) = pci_addr else {
            eprintln!("[gpu-procs] {} has no PCI address", card_id);
            return Vec::new();
        };
        eprintln!("[gpu-procs] scanning for pdev={}", pci_addr);

        let Ok(proc_dir) = fs::read_dir("/proc") else {
            return Vec::new();
        };

        let mut users: std::collections::HashMap<u32, (String, u64)> =
            std::collections::HashMap::new();
        let mut any_vram_reported = false;

        for entry in proc_dir.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(fds) = fs::read_dir(format!("/proc/{}/fdinfo", pid)) else {
                continue;
            };

            // Dedupe by client-id: the same client can appear on multiple FDs
            // (e.g. two opens of the same DRM node from the same process).
            let mut clients: std::collections::HashMap<String, u64> =
                std::collections::HashMap::new();

            for fd in fds.flatten() {
                let Ok(content) = fs::read_to_string(fd.path()) else {
                    continue;
                };
                let mut cid: Option<String> = None;
                let mut vram_kib = 0u64;
                let mut pdev = String::new();
                let mut is_amd_or_intel = false;

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
                    }
                }

                if !is_amd_or_intel {
                    continue;
                }
                // Match by PCI address. Some kernels prefix with "0000:" and some
                // don't — accept both directions of ends_with.
                if pdev != pci_addr
                    && !pdev.ends_with(&pci_addr)
                    && !pci_addr.ends_with(&pdev)
                {
                    continue;
                }
                if let Some(c) = cid {
                    let e = clients.entry(c).or_insert(0);
                    if vram_kib > *e {
                        *e = vram_kib;
                    }
                }
            }

            if !clients.is_empty() {
                let total_kib: u64 = clients.values().sum();
                let comm = fs::read_to_string(format!("/proc/{}/comm", pid))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| format!("pid {}", pid));
                users.insert(pid, (comm, total_kib));
            }
        }

        let mut procs: Vec<_> = users.into_iter().collect();
        procs.sort_by_key(|(_, (_, mem))| std::cmp::Reverse(*mem));

        eprintln!(
            "[gpu-procs] found {} clients on {}, vram_reported={}",
            procs.len(),
            pci_addr,
            any_vram_reported
        );

        procs
            .into_iter()
            .take(15)
            .map(|(pid, (name, kib))| ProcessItem {
                pid,
                name,
                metric: if any_vram_reported {
                    format!("{} MiB", kib / 1024)
                } else {
                    "—".to_string()
                },
                is_highlight: false,
            })
            .collect()
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
            let name = parts[1]
                .rsplit('/')
                .next()
                .unwrap_or(parts[1])
                .to_string();
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

fn resolve_gpu_name_sync(pci_addr: &str, card_id: &str) -> String {
    if !pci_addr.is_empty() {
        if let Ok(out) = std::process::Command::new("lspci")
            .args(["-vmm", "-s", pci_addr])
            .output()
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    if let Some(dev) = line.strip_prefix("Device:") {
                        return clean_gpu_name(dev.trim());
                    }
                }
            }
        }
    }
    card_id.to_uppercase()
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
    let uevent = std::fs::read_to_string(
        format!("/sys/class/drm/{}/device/uevent", card_id),
    )
    .ok()?;
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
        let (is_nv, nv_idx) = if let Some(idx_str) = card_id.strip_prefix("nvidia") {
            (true, idx_str.parse::<u32>().unwrap_or(0))
        } else {
            (false, 0)
        };
        let mut m = Self::collect_gpu(card_id, is_nv, nv_idx).await;
        m.name = resolve_gpu_name_sync_cached(card_id);
        m
    }
}
