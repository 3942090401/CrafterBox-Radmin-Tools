use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MC_LAN_ADDR: &str = "224.0.2.60";
pub const MC_LAN_PORT: u16 = 4445;

// ============================================
//  广播（发送）
// ============================================

#[derive(Default)]
pub struct BroadcastState {
    pub running: bool,
    pub paused: bool,
    pub packets_sent: u64,
    pub current_rate: u64,
}

pub type SharedState = Arc<Mutex<BroadcastState>>;

pub fn start_broadcast(
    state: SharedState,
    count: usize,
    motd_template: String,
    port_prefix: String,
) {
    thread::spawn(move || {
        let sock = match UdpSocket::bind("0.0.0.0:0") {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[broadcast] bind failed: {}", e);
                return;
            }
        };
        let _ = sock.set_multicast_ttl_v4(2);

        loop {
            {
                let s = state.lock().unwrap();
                if !s.running {
                    break;
                }
                if s.paused {
                    drop(s);
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
            }

            let mut sent_this_round = 0u64;
            for i in 0..count {
                {
                    let s = state.lock().unwrap();
                    if !s.running || s.paused {
                        break;
                    }
                }

                let motd = motd_template.replace("{num}", &(i + 1).to_string());
                let port_str = format!("{}{}", port_prefix, i + 1);
                let payload = format!("[MOTD]{}[/MOTD][AD]{}[/AD]", motd, port_str);
                let target = format!("{}:{}", MC_LAN_ADDR, MC_LAN_PORT);

                if sock.send_to(payload.as_bytes(), &target).is_ok() {
                    sent_this_round += 1;
                }
            }

            {
                let mut s = state.lock().unwrap();
                s.packets_sent += sent_this_round;
                s.current_rate = sent_this_round;
            }

            thread::sleep(Duration::from_millis(1500));
        }
    });
}

pub fn stop_broadcast(state: SharedState) {
    if let Ok(mut s) = state.lock() {
        s.running = false;
        s.paused = false;
    }
}

pub fn pause_broadcast(state: SharedState) {
    if let Ok(mut s) = state.lock() {
        s.paused = true;
    }
}

pub fn resume_broadcast(state: SharedState) {
    if let Ok(mut s) = state.lock() {
        s.paused = false;
    }
}

// ============================================
//  MOTD 扫描（接收）
// ============================================

#[derive(Clone, serde::Serialize)]
pub struct ScanResult {
    pub motd: String,
    pub ad: String,
    pub ip: String,
    pub address: String,
    pub time: u64,
    pub count: u64,
    pub icon: String,
}

pub struct ScanState {
    pub running: bool,
    pub generation: u64,
    pub results: Vec<ScanResult>,
    pub max_per_ip: u32,
}

impl Default for ScanState {
    fn default() -> Self {
        Self {
            running: false,
            generation: 0,
            results: Vec::new(),
            max_per_ip: 3,
        }
    }
}

pub type SharedScan = Arc<Mutex<ScanState>>;

pub fn start_scan(state: SharedScan) {
    let generation = {
        let mut s = state.lock().unwrap();
        if s.running {
            return;
        }
        s.running = true;
        s.generation = s.generation.wrapping_add(1);
        s.generation
    };

    thread::spawn(move || {
        let socket = match UdpSocket::bind("0.0.0.0:4445") {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[scan] bind 4445 failed: {}", e);
                if let Ok(mut s) = state.lock() {
                    if s.generation == generation {
                        s.running = false;
                    }
                }
                return;
            }
        };

        let _ =
            socket.join_multicast_v4(&"224.0.2.60".parse().unwrap(), &"0.0.0.0".parse().unwrap());
        let _ = socket.set_read_timeout(Some(Duration::from_millis(500)));

        let mut buf = [0u8; 4096];

        loop {
            {
                let s = state.lock().unwrap();
                if !s.running || s.generation != generation {
                    break;
                }
            }

            match socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    if let Some((motd, ad, icon)) = parse_motd_packet(&data) {
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);

                        let mut s = state.lock().unwrap();
                        let ip = addr.ip().to_string();
                        let limit = if s.max_per_ip == 0 {
                            u32::MAX
                        } else {
                            s.max_per_ip
                        };

                        let same_ip_count = s.results.iter().filter(|r| r.ip == ip).count() as u32;
                        let is_dup = s.results.iter().any(|r| r.motd == motd && r.ad == ad);

                        if same_ip_count >= limit && !is_dup {
                            continue;
                        }

                        if let Some(idx) =
                            s.results.iter().position(|r| r.motd == motd && r.ad == ad)
                        {
                            s.results[idx].count += 1;
                            s.results[idx].time = now;
                            s.results[idx].ip = ip;
                            s.results[idx].address = addr.to_string();
                        } else {
                            s.results.push(ScanResult {
                                motd,
                                ad,
                                ip,
                                address: addr.to_string(),
                                time: now,
                                count: 1,
                                icon,
                            });
                        }
                    }
                }
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => {
                    eprintln!("[scan] recv error: {}", e);
                }
            }
        }
    });
}

fn parse_motd_packet(data: &str) -> Option<(String, String, String)> {
    fn tag_value<'a>(data: &'a str, tag: &str) -> Option<&'a str> {
        let start_tag = format!("[{tag}]");
        let end_tag = format!("[/{tag}]");
        let start = data.find(&start_tag)? + start_tag.len();
        let end = data[start..].find(&end_tag)? + start;
        Some(&data[start..end])
    }

    let motd = tag_value(data, "MOTD")?;
    let ad = tag_value(data, "AD")?;
    if motd.is_empty() || ad.is_empty() {
        return None;
    }
    let icon = tag_value(data, "ICON")
        .map(str::trim)
        .map(|value| {
            if value.starts_with("data:image/") {
                value.to_string()
            } else if !value.is_empty()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
            {
                format!("data:image/png;base64,{value}")
            } else {
                String::new()
            }
        })
        .unwrap_or_default();

    Some((motd.to_string(), ad.to_string(), icon))
}

#[cfg(test)]
mod tests {
    use super::parse_motd_packet;

    #[test]
    fn parses_broadcast_and_optional_png_icon() {
        let packet = "[MOTD]hello[/MOTD][AD]25565[/AD][ICON]aGVsbG8=[/ICON]";
        assert_eq!(
            parse_motd_packet(packet),
            Some((
                "hello".to_string(),
                "25565".to_string(),
                "data:image/png;base64,aGVsbG8=".to_string()
            ))
        );
    }

    #[test]
    fn accepts_packets_without_icon() {
        let packet = "[MOTD]hello[/MOTD][AD]25565[/AD]";
        assert_eq!(
            parse_motd_packet(packet),
            Some(("hello".to_string(), "25565".to_string(), String::new()))
        );
    }

    #[test]
    fn ignores_packets_without_motd_or_ad() {
        assert_eq!(parse_motd_packet("[MOTD]hello[/MOTD]"), None);
    }
}

pub fn stop_scan(state: SharedScan) {
    if let Ok(mut s) = state.lock() {
        s.running = false;
    }
}

pub fn get_scan_results(state: SharedScan) -> Vec<ScanResult> {
    state.lock().map(|s| s.results.clone()).unwrap_or_default()
}

pub fn clear_scan_results(state: SharedScan) {
    if let Ok(mut s) = state.lock() {
        s.results.clear();
    }
}

pub fn set_max_per_ip(state: SharedScan, limit: u32) {
    if let Ok(mut s) = state.lock() {
        s.max_per_ip = limit;
    }
}

pub fn get_max_per_ip(state: SharedScan) -> u32 {
    state.lock().map(|s| s.max_per_ip).unwrap_or(0)
}

// ============================================
//  公益广播
// ============================================

pub struct PublicBroadcastState {
    pub enabled: bool,
}

pub type SharedPublicBroadcast = Arc<Mutex<PublicBroadcastState>>;

impl Default for PublicBroadcastState {
    fn default() -> Self {
        Self { enabled: true }
    }
}

const PUBLIC_MOTD: &str = concat!(
    "§6§l§a免费改§dIP§a等功能的§b多功能§e工具来了！",
    "§b§l使用§6§lRust §r§c§l+ §9§lWebUI§d§l编写！",
    "\n端口：§6加Q群获取§b§7[§e§n332824844§r§7] ",
    "§a§l本提示由§e§o公益广播§r§6§l功能自动广播",
    "§c§l(非群主广播) §c§l关闭前往§b设置>公益广播§a§l以关闭"
);

const PUBLIC_AD: &str = concat!(
    "§6加Q群获取",
    "§b§7[",
    "§e§n332824844",
    "§r§7] ",
    "§a§l本提示由§e§o公益广播§r§6§l功能自动广播",
    "§c§l(非群主广播) ",
    "§c§l关闭前往§b设置>公益广播§a§l以关闭"
);

pub fn start_public_broadcast(state: SharedPublicBroadcast) {
    thread::spawn(move || loop {
        {
            let s = state.lock().unwrap();
            if !s.enabled {
                drop(s);
                thread::sleep(Duration::from_secs(3));
                let s2 = state.lock().unwrap();
                if !s2.enabled {
                    break;
                }
                continue;
            }
        }

        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
            let _ = sock.set_multicast_ttl_v4(2);
            let payload = format!("[MOTD]{}[/MOTD][AD]{}[/AD]", PUBLIC_MOTD, PUBLIC_AD);
            let target = format!("{}:{}", MC_LAN_ADDR, MC_LAN_PORT);
            let _ = sock.send_to(payload.as_bytes(), &target);
        }

        thread::sleep(Duration::from_secs(5));
    });
}

#[allow(dead_code)]
pub fn stop_public_broadcast(state: SharedPublicBroadcast) {
    if let Ok(mut s) = state.lock() {
        s.enabled = false;
    }
}

pub fn set_public_broadcast(state: SharedPublicBroadcast, enabled: bool) {
    if let Ok(mut s) = state.lock() {
        s.enabled = enabled;
    }
    if enabled {
        start_public_broadcast(state);
    }
}

pub fn is_public_broadcast_enabled(state: SharedPublicBroadcast) -> bool {
    state.lock().map(|s| s.enabled).unwrap_or(false)
}
