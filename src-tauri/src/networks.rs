//! Radmin VPN 控制管道（`\\.\pipe\RadminVpnGuiChannel`）读写与编解码。
//!
//! 线格式（对照 `协议还原-CJoinNetwork.md`，并用真实管道帧逐字节校验）：
//!
//! ```text
//! 帧   = [u32 LE total_len][payload]
//! 记录 = [u32 BE value_len][u8 type][u8 0][u16 BE field_id][value]
//! ```
//!
//! `type` + `field_id` 组成服务端代码里的 32 位 field key：
//! `0x00` 嵌套消息（field_id 即消息 ID）、`0x01`/`0x0b` u32 大端、
//! `0x02` u64 大端、`0x03`/`0x08` UTF-16BE 字符串（尾部 U+0001）、
//! `0x09`/`0x0d` 16 字节定长（IPv6 / GUID）。
//!
//! # 状态转储结构（实测）
//!
//! ```text
//! 0x1320 根         本机概要（0x0304 别名 / 0x0305 IPv4+IPv6 / 0x01c9 公网）
//! └ 0x1321 集合
//!   ├ 0x1323 × N    网络：0x0306 名称、0x0309 GUID、0x030a 启用、0x0347 访问码
//!   │ └ 0x1324 × M  成员槽位：0x01e1 = 成员 ID（即 Radmin RID，十进制展示）、0x030a 有效
//!   └ 0x1322 × K    成员明细：0x0304 名称、0x0305 IP、0x01c9 公网、0x0326 会话状态
//!     └ 0x1266 × S  会话：0x01e9 「本地 -> 远端」端点文本
//! ```
//!
//! 网络与成员之间的关联键是 `0x01e1`：槽位里的成员 ID 去成员明细里查，
//! 实测 2878/2878 全部命中。`0x0326` 是会话状态（`0` 无连接，非 `0` 在线）。

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

const PIPE_NAME: &str = r"\\.\pipe\RadminVpnGuiChannel";
const MAX_FRAME_SIZE: usize = 8 * 1024 * 1024;

// ===== 消息 ID =====
const MSG_ROOT: u16 = 0x1320;
const MSG_COLLECTION: u16 = 0x1321;
const MSG_NETWORK: u16 = 0x1323;
const MSG_MEMBER_SLOT: u16 = 0x1324;
const MSG_MEMBER: u16 = 0x1322;
const MSG_SESSION: u16 = 0x1266;
const MSG_ENDPOINT: u16 = 0x127d;
const MSG_JOIN_NETWORK: u16 = 0x131b;

// ===== 字段 ID =====
const F_MEMBER_ID: u16 = 0x01e1;
const F_ALIAS: u16 = 0x0304;
const F_NETWORK_NAME: u16 = 0x0306;
const F_NETWORK_GUID: u16 = 0x0309;
const F_ENABLED: u16 = 0x030a;
const F_IP: u16 = 0x0305;
const F_PUBLIC_ADDRESS: u16 = 0x01c9;
const F_MEMBER_COUNT: u16 = 0x03a3;
const F_ACCESS_CODE: u16 = 0x0347;
const F_SHARE_LINK: u16 = 0x0348;
const F_SESSION_STATE: u16 = 0x0326;
const F_SERVICE_PORT: u16 = 0x0330;
const F_ENDPOINT_TEXT: u16 = 0x01e9;
const F_ENDPOINT_HOST: u16 = 0x01c2;
const F_ENDPOINT_PORT: u16 = 0x01c3;

// ===== 命令 =====
const CMD_JOIN_NETWORK: u32 = 3;
const CMD_FIELD_MSG_ID: u16 = 0x0300;
const CMD_FIELD_REQUEST_ID: u16 = 0x034a;
const REPLY_FIELD_RESULT: u16 = 0x0310;

// ===== 数据结构 =====

#[derive(Debug, Clone, Serialize)]
pub struct RadminPeer {
    /// 成员 ID，即 Radmin RID（`0x01e1` 的十进制值）
    pub rid: String,
    pub name: String,
    pub ipv4: Option<String>,
    /// 虚拟网段前缀长度：Radmin VPN 的网段是 26.0.0.0/8
    pub ipv4_prefix: Option<u8>,
    pub ipv6: Option<String>,
    pub public_address: Option<String>,
    pub online: bool,
    /// 会话状态码（`0x0326` 原值，便于核对）
    pub state_code: u32,
    /// 通道类型：状态码 5 走转发（UDP），其余直连为 TCP
    pub channel: Option<String>,
    /// 会话端点「本地 -> 远端」
    pub session_remote: Option<String>,
    /// 成员侧服务端口（`0x0330`，如 4899 / 25565）
    pub service_port: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RadminNetwork {
    pub name: String,
    pub guid: String,
    pub enabled: Option<bool>,
    pub access_code: Option<String>,
    pub share_link: Option<String>,
    /// 状态转储里声明的数值（`0x03a3`，语义未确认，仅作参考）
    pub declared_count: Option<u32>,
    pub peers: Vec<RadminPeer>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RadminSelf {
    pub alias: Option<String>,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub public_address: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RadminSnapshot {
    pub self_node: RadminSelf,
    pub networks: Vec<RadminNetwork>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct JoinReply {
    pub msg_id: u32,
    /// 返回码：0 = 成功
    pub code: u32,
    pub request_id: u32,
    pub message: String,
}

// ===== TLV =====

#[derive(Debug, Clone)]
struct Record {
    kind: u8,
    id: u16,
    value: Vec<u8>,
}

fn parse_records(data: &[u8]) -> Result<Vec<Record>, String> {
    let mut records = Vec::new();
    let mut offset = 0usize;
    while offset < data.len() {
        let header_end = offset
            .checked_add(8)
            .ok_or_else(|| "Radmin 状态消息偏移溢出".to_string())?;
        if header_end > data.len() {
            return Err("Radmin 状态消息包含不完整的 TLV 头".into());
        }
        let value_len = u32::from_be_bytes(
            data[offset..offset + 4]
                .try_into()
                .map_err(|_| "Radmin TLV 长度字段无效".to_string())?,
        ) as usize;
        let value_end = header_end
            .checked_add(value_len)
            .ok_or_else(|| "Radmin TLV 长度溢出".to_string())?;
        if value_end > data.len() {
            return Err("Radmin 状态消息包含不完整的 TLV 值".into());
        }
        records.push(Record {
            kind: data[offset + 4],
            id: u16::from_be_bytes([data[offset + 6], data[offset + 7]]),
            value: data[header_end..value_end].to_vec(),
        });
        offset = value_end;
    }
    Ok(records)
}

fn push_record(out: &mut Vec<u8>, kind: u8, id: u16, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.push(kind);
    out.push(0);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(value);
}

fn push_u32(out: &mut Vec<u8>, kind: u8, id: u16, value: u32) {
    push_record(out, kind, id, &value.to_be_bytes());
}

fn push_string(out: &mut Vec<u8>, kind: u8, id: u16, value: &str) {
    let mut bytes = Vec::with_capacity(value.len() * 2 + 2);
    for unit in value.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    // CUniversalString 的 wire 定界符
    bytes.extend_from_slice(&[0x00, 0x01]);
    push_record(out, kind, id, &bytes);
}

fn as_string(record: &Record) -> Option<String> {
    if record.kind != 0x03 && record.kind != 0x08 {
        return None;
    }
    if record.value.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = record
        .value
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .collect();
    let mut text = String::from_utf16(&units).ok()?;
    if text.ends_with('\u{0001}') {
        text.pop();
    }
    Some(text)
}

fn as_u32(record: &Record) -> Option<u32> {
    if (record.kind != 0x01 && record.kind != 0x0b) || record.value.len() != 4 {
        return None;
    }
    Some(u32::from_be_bytes([
        record.value[0],
        record.value[1],
        record.value[2],
        record.value[3],
    ]))
}

fn as_u64(record: &Record) -> Option<u64> {
    if record.kind != 0x02 || record.value.len() != 8 {
        return None;
    }
    Some(u64::from_be_bytes(record.value.as_slice().try_into().ok()?))
}

fn find_string(records: &[Record], id: u16) -> Option<String> {
    records
        .iter()
        .filter(|record| record.id == id)
        .find_map(as_string)
        .filter(|text| !text.is_empty())
}

fn find_u32(records: &[Record], id: u16) -> Option<u32> {
    records
        .iter()
        .filter(|record| record.id == id)
        .find_map(as_u32)
}

fn find_u64(records: &[Record], id: u16) -> Option<u64> {
    records
        .iter()
        .filter(|record| record.id == id)
        .find_map(as_u64)
}

fn messages_of(records: &[Record], message_id: u16) -> Result<Vec<Vec<Record>>, String> {
    let mut out = Vec::new();
    for record in records.iter().filter(|record| record.kind == 0) {
        if record.id == message_id {
            out.push(parse_records(&record.value)?);
        }
    }
    Ok(out)
}

fn ipv4_of(records: &[Record]) -> Option<String> {
    records
        .iter()
        .filter(|record| record.id == F_IP)
        .find_map(|record| match (record.kind, record.value.len()) {
            (0x01, 4) | (0x0b, 4) => Some(format!(
                "{}.{}.{}.{}",
                record.value[0], record.value[1], record.value[2], record.value[3]
            )),
            _ => None,
        })
}

fn ipv6_of(records: &[Record]) -> Option<String> {
    records
        .iter()
        .filter(|record| record.id == F_IP)
        .find_map(|record| match (record.kind, record.value.len()) {
            (0x09, 16) | (0x0d, 16) => {
                let bytes: [u8; 16] = record.value.as_slice().try_into().ok()?;
                Some(std::net::Ipv6Addr::from(bytes).to_string())
            }
            _ => None,
        })
}

// ===== 解码 =====

/// 成员明细：由集合下的 `0x1322` 消息建立，按 `0x01e1` 建索引。
#[derive(Debug, Clone, Default)]
struct MemberDetail {
    name: Option<String>,
    ipv4: Option<String>,
    ipv6: Option<String>,
    public_address: Option<String>,
    state_code: u32,
    session_remote: Option<String>,
    service_port: Option<u32>,
}

fn decode_member_detail(records: &[Record]) -> Result<Option<(u64, MemberDetail)>, String> {
    let Some(id) = find_u64(records, F_MEMBER_ID) else {
        return Ok(None);
    };

    let sessions = messages_of(records, MSG_SESSION)?;
    let mut session_remote = None;
    for session in &sessions {
        if let Some(text) = find_string(session, F_ENDPOINT_TEXT) {
            session_remote = Some(text);
            break;
        }
    }
    if session_remote.is_none() {
        'outer: for session in &sessions {
            for endpoint in messages_of(session, MSG_ENDPOINT)? {
                let host = find_string(&endpoint, F_ENDPOINT_HOST);
                let port = find_u32(&endpoint, F_ENDPOINT_PORT);
                if let (Some(host), Some(port)) = (host, port) {
                    session_remote = Some(format!("{host}:{port}"));
                    break 'outer;
                }
            }
        }
    }

    Ok(Some((
        id,
        MemberDetail {
            name: find_string(records, F_ALIAS),
            ipv4: ipv4_of(records),
            ipv6: ipv6_of(records),
            public_address: find_string(records, F_PUBLIC_ADDRESS),
            state_code: find_u32(records, F_SESSION_STATE).unwrap_or(0),
            session_remote,
            service_port: find_u32(records, F_SERVICE_PORT).filter(|port| *port != 0),
        },
    )))
}

fn channel_of(state_code: u32) -> Option<String> {
    match state_code {
        0 => None,
        5 => Some("UDP".to_string()),
        _ => Some("TCP".to_string()),
    }
}

fn decode_network(
    records: &[Record],
    directory: &BTreeMap<u64, MemberDetail>,
) -> Result<RadminNetwork, String> {
    let mut peers = Vec::new();
    for slot in messages_of(records, MSG_MEMBER_SLOT)? {
        let Some(id) = find_u64(&slot, F_MEMBER_ID) else {
            continue;
        };
        let detail = directory.get(&id).cloned().unwrap_or_default();
        let ipv4 = detail.ipv4.clone();
        peers.push(RadminPeer {
            rid: id.to_string(),
            name: detail.name.unwrap_or_else(|| "未知成员".to_string()),
            ipv4_prefix: ipv4.as_ref().map(|address| {
                if address.starts_with("26.") {
                    8
                } else {
                    24
                }
            }),
            ipv4,
            ipv6: detail.ipv6,
            public_address: detail.public_address,
            online: detail.state_code != 0,
            state_code: detail.state_code,
            channel: channel_of(detail.state_code),
            session_remote: detail.session_remote,
            service_port: detail.service_port,
        });
    }

    // 在线成员优先，其次按名称排序
    peers.sort_by(|left, right| {
        right
            .online
            .cmp(&left.online)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    let guid = records
        .iter()
        .find(|record| record.id == F_NETWORK_GUID && record.value.len() == 16)
        .map(|record| {
            record
                .value
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        })
        .unwrap_or_default();

    Ok(RadminNetwork {
        name: find_string(records, F_NETWORK_NAME).unwrap_or_else(|| "未命名网络".to_string()),
        guid,
        enabled: find_u32(records, F_ENABLED).map(|value| value != 0),
        access_code: find_string(records, F_ACCESS_CODE),
        share_link: find_string(records, F_SHARE_LINK),
        declared_count: find_u32(records, F_MEMBER_COUNT),
        peers,
    })
}

fn decode_self(records: &[Record]) -> RadminSelf {
    RadminSelf {
        alias: find_string(records, F_ALIAS),
        ipv4: ipv4_of(records),
        ipv6: ipv6_of(records),
        public_address: find_string(records, F_PUBLIC_ADDRESS),
    }
}

fn decode_snapshot(frames: &[Vec<u8>]) -> Result<RadminSnapshot, String> {
    let started = Instant::now();
    let mut self_node = RadminSelf::default();
    let mut networks = Vec::new();

    for frame in frames {
        let Ok(records) = parse_records(frame) else {
            continue;
        };
        for root in messages_of(&records, MSG_ROOT)? {
            if self_node.alias.is_none() {
                let candidate = decode_self(&root);
                if candidate.alias.is_some() || candidate.ipv4.is_some() {
                    self_node = candidate;
                }
            }
            for collection in messages_of(&root, MSG_COLLECTION)? {
                let mut directory: BTreeMap<u64, MemberDetail> = BTreeMap::new();
                for member in messages_of(&collection, MSG_MEMBER)? {
                    if let Some((id, detail)) = decode_member_detail(&member)? {
                        directory.insert(id, detail);
                    }
                }
                for network in messages_of(&collection, MSG_NETWORK)? {
                    networks.push(decode_network(&network, &directory)?);
                }
            }
        }
    }

    networks.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(RadminSnapshot {
        self_node,
        networks,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

// ===== 管道 IO =====

/// 读取连接建立后服务端推送的状态帧，直到拿到含 `0x1320` 的全网转储。
fn read_frames() -> Result<Vec<Vec<u8>>, String> {
    let mut pipe = Pipe::connect(Duration::from_millis(1500))?;
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut frames: Vec<Vec<u8>> = Vec::new();
    while frames.len() < 8 {
        let frame = match pipe.read_frame(deadline) {
            Ok(frame) => frame,
            Err(error) => {
                if frames.is_empty() {
                    return Err(error);
                }
                break;
            }
        };
        let has_dump = parse_records(&frame)
            .map(|records| {
                records
                    .iter()
                    .any(|record| record.kind == 0 && record.id == MSG_ROOT)
            })
            .unwrap_or(false);
        frames.push(frame);
        if has_dump {
            break;
        }
    }
    if frames.is_empty() {
        return Err("Radmin GUI 管道没有返回状态帧".into());
    }
    Ok(frames)
}

pub fn read_snapshot() -> Result<RadminSnapshot, String> {
    decode_snapshot(&read_frames()?)
}

/// 发送 `CJoinNetwork`（msg id = 3）并读取回执。
pub fn join_network(name: &str, password: &str, flag: u32) -> Result<JoinReply, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("网络名不能为空".into());
    }

    let request_id = next_request_id();
    let mut inner = Vec::new();
    push_string(&mut inner, 0x03, F_NETWORK_NAME, name);
    push_u32(&mut inner, 0x0b, 0x033f, flag);
    if !password.is_empty() {
        push_string(&mut inner, 0x08, 0x030d, password);
    }

    let mut payload = Vec::with_capacity(inner.len() + 64);
    push_u32(&mut payload, 0x01, CMD_FIELD_MSG_ID, CMD_JOIN_NETWORK);
    push_u32(&mut payload, 0x01, CMD_FIELD_REQUEST_ID, request_id);
    push_record(&mut payload, 0x00, MSG_JOIN_NETWORK, &inner);

    let mut pipe = Pipe::connect(Duration::from_millis(2000))?;
    pipe.write_frame(&payload)?;

    // 连接后服务端会先推状态帧，回执夹在其中，按截止时间持续读
    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline {
        let frame = match pipe.read_frame(deadline) {
            Ok(frame) => frame,
            Err(_) => break,
        };
        let Ok(records) = parse_records(&frame) else {
            continue;
        };
        let Some(msg_id) = find_u32(&records, CMD_FIELD_MSG_ID) else {
            continue;
        };
        if msg_id != CMD_JOIN_NETWORK {
            continue;
        }
        let Some(code) = find_u32(&records, REPLY_FIELD_RESULT) else {
            continue;
        };
        return Ok(JoinReply {
            msg_id,
            code,
            request_id: find_u32(&records, CMD_FIELD_REQUEST_ID).unwrap_or(0),
            message: describe_result(code).to_string(),
        });
    }
    Err("等待加网回执超时：服务端没有返回结果码".into())
}

fn describe_result(code: u32) -> &'static str {
    match code {
        0 => "已加入网络",
        1002 => "网络名不存在（标志位非 0）",
        1012 => "网络名不存在或拼写不符，需与网络名完全一致（含空格）",
        1015 => "参数组合无效",
        1017 => "网络名为空",
        _ => "未知返回码",
    }
}

fn next_request_id() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0x1000);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.subsec_nanos())
        .unwrap_or(0);
    nanos ^ COUNTER.fetch_add(0x9e37, Ordering::Relaxed)
}

#[cfg(target_os = "windows")]
struct Pipe {
    file: std::fs::File,
    handle: *mut core::ffi::c_void,
}

#[cfg(target_os = "windows")]
impl Pipe {
    fn connect(wait: Duration) -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::io::AsRawHandle;

        #[link(name = "kernel32")]
        extern "system" {
            fn WaitNamedPipeW(name: *const u16, timeout: u32) -> i32;
        }

        let wide: Vec<u16> = std::ffi::OsStr::new(PIPE_NAME)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        if unsafe { WaitNamedPipeW(wide.as_ptr(), wait.as_millis() as u32) } == 0 {
            return Err(format!(
                "无法连接 Radmin GUI 管道（Radmin VPN 是否在运行？）：{}",
                std::io::Error::last_os_error()
            ));
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(PIPE_NAME)
            .map_err(|error| format!("打开 Radmin GUI 管道失败：{error}"))?;
        let handle = file.as_raw_handle() as *mut core::ffi::c_void;
        Ok(Self { file, handle })
    }

    fn wait_for(&self, needed: usize, deadline: Instant) -> Result<(), String> {
        #[link(name = "kernel32")]
        extern "system" {
            fn PeekNamedPipe(
                pipe: *mut core::ffi::c_void,
                buffer: *mut core::ffi::c_void,
                buffer_size: u32,
                bytes_read: *mut u32,
                bytes_available: *mut u32,
                bytes_left: *mut u32,
            ) -> i32;
        }

        loop {
            let mut available = 0u32;
            let ok = unsafe {
                PeekNamedPipe(
                    self.handle,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(format!(
                    "读取 Radmin 管道失败：{}",
                    std::io::Error::last_os_error()
                ));
            }
            if available as usize >= needed {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("等待 Radmin 管道数据超时".into());
            }
            std::thread::sleep(Duration::from_millis(8));
        }
    }

    fn read_frame(&mut self, deadline: Instant) -> Result<Vec<u8>, String> {
        use std::io::Read;

        self.wait_for(4, deadline)?;
        let mut prefix = [0u8; 4];
        self.file
            .read_exact(&mut prefix)
            .map_err(|error| format!("读取管道长度前缀失败：{error}"))?;
        let len = u32::from_le_bytes(prefix) as usize;
        if len == 0 || len > MAX_FRAME_SIZE {
            return Err(format!("Radmin 状态帧长度无效：{len} 字节"));
        }
        self.wait_for(len, deadline)?;
        let mut buffer = vec![0u8; len];
        self.file
            .read_exact(&mut buffer)
            .map_err(|error| format!("读取管道载荷失败：{error}"))?;
        Ok(buffer)
    }

    fn write_frame(&mut self, payload: &[u8]) -> Result<(), String> {
        use std::io::Write;

        let mut frame = Vec::with_capacity(payload.len() + 4);
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(payload);
        self.file
            .write_all(&frame)
            .and_then(|_| self.file.flush())
            .map_err(|error| format!("写入 Radmin 管道失败：{error}"))
    }
}

// ===== ICMP 延迟探测 =====

/// 批量测量虚拟地址的 ICMP 往返时延（毫秒）。失败的主机不会出现在结果里。
#[cfg(target_os = "windows")]
pub fn ping_hosts(hosts: &[String], timeout_ms: u32) -> BTreeMap<String, u32> {
    use std::sync::{Arc, Mutex};

    let unique: Vec<String> = {
        let mut seen = BTreeSet::new();
        hosts
            .iter()
            .filter(|host| host.parse::<std::net::Ipv4Addr>().is_ok())
            .filter(|host| seen.insert((*host).clone()))
            .cloned()
            .collect()
    };
    if unique.is_empty() {
        return BTreeMap::new();
    }

    let results = Arc::new(Mutex::new(BTreeMap::new()));
    const WORKERS: usize = 24;
    let chunk = unique.len().div_ceil(WORKERS).max(1);
    let mut handles = Vec::new();
    for slice in unique.chunks(chunk) {
        let slice: Vec<String> = slice.to_vec();
        let results = results.clone();
        handles.push(std::thread::spawn(move || {
            let Some(handle) = IcmpHandle::open() else {
                return;
            };
            for host in slice {
                if let Some(rtt) = handle.ping(&host, timeout_ms) {
                    results.lock().unwrap().insert(host, rtt);
                }
            }
        }));
    }
    for handle in handles {
        let _ = handle.join();
    }

    match Arc::try_unwrap(results) {
        Ok(mutex) => mutex.into_inner().unwrap(),
        Err(shared) => shared.lock().unwrap().clone(),
    }
}

#[cfg(not(target_os = "windows"))]
pub fn ping_hosts(_hosts: &[String], _timeout_ms: u32) -> BTreeMap<String, u32> {
    BTreeMap::new()
}

#[cfg(target_os = "windows")]
struct IcmpHandle(*mut core::ffi::c_void);

#[cfg(target_os = "windows")]
impl IcmpHandle {
    fn open() -> Option<Self> {
        #[link(name = "iphlpapi")]
        extern "system" {
            fn IcmpCreateFile() -> *mut core::ffi::c_void;
        }
        let handle = unsafe { IcmpCreateFile() };
        if handle.is_null() || handle as isize == -1 {
            None
        } else {
            Some(Self(handle))
        }
    }

    fn ping(&self, host: &str, timeout_ms: u32) -> Option<u32> {
        #[link(name = "iphlpapi")]
        extern "system" {
            fn IcmpSendEcho(
                handle: *mut core::ffi::c_void,
                destination: u32,
                request_data: *const core::ffi::c_void,
                request_size: u16,
                request_options: *const core::ffi::c_void,
                reply_buffer: *mut core::ffi::c_void,
                reply_size: u32,
                timeout: u32,
            ) -> u32;
        }

        let address: std::net::Ipv4Addr = host.parse().ok()?;
        // IPAddr 联合体按值传递，S_addr 为网络字节序
        let destination = u32::from_ne_bytes(address.octets());
        let mut payload = [0u8; 8];
        // ICMP_ECHO_REPLY 在 x64 上 32 字节，留足缓冲区
        let mut reply = [0u8; 96];
        let count = unsafe {
            IcmpSendEcho(
                self.0,
                destination,
                payload.as_mut_ptr() as *const core::ffi::c_void,
                payload.len() as u16,
                std::ptr::null(),
                reply.as_mut_ptr() as *mut core::ffi::c_void,
                reply.len() as u32,
                timeout_ms,
            )
        };
        if count == 0 {
            return None;
        }
        // 布局：Address(4) Status(4) RoundTripTime(4) DataSize(2) Reserved(2) ...
        let status = u32::from_ne_bytes(reply[4..8].try_into().ok()?);
        if status != 0 {
            return None;
        }
        let rtt = u32::from_ne_bytes(reply[8..12].try_into().ok()?);
        Some(rtt)
    }
}

#[cfg(target_os = "windows")]
impl Drop for IcmpHandle {
    fn drop(&mut self) {
        #[link(name = "iphlpapi")]
        extern "system" {
            fn IcmpCloseHandle(handle: *mut core::ffi::c_void) -> i32;
        }
        unsafe {
            IcmpCloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(kind: u8, id: u16, value: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8 + value.len());
        bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&[kind, 0]);
        bytes.extend_from_slice(&id.to_be_bytes());
        bytes.extend_from_slice(value);
        bytes
    }

    fn text(value: &str) -> Vec<u8> {
        let mut bytes: Vec<u8> = value.encode_utf16().flat_map(u16::to_be_bytes).collect();
        bytes.extend_from_slice(&[0, 1]);
        bytes
    }

    #[test]
    fn links_member_slots_to_member_details() {
        let mut member = Vec::new();
        member.extend(record(2, F_MEMBER_ID, &1_234_567u64.to_be_bytes()));
        member.extend(record(3, F_ALIAS, &text("Host")));
        member.extend(record(1, F_IP, &[26, 10, 0, 7]));
        member.extend(record(
            0x09,
            F_IP,
            &[0xfd, 0xfd, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 10, 0, 7],
        ));
        member.extend(record(3, F_PUBLIC_ADDRESS, &text("1.2.3.4")));
        member.extend(record(1, F_SESSION_STATE, &1u32.to_be_bytes()));
        member.extend(record(1, F_SERVICE_PORT, &25565u32.to_be_bytes()));
        let mut session = Vec::new();
        session.extend(record(3, F_ENDPOINT_TEXT, &text("0.0.0.0:49678 -> 5.44.170.107:63918")));
        member.extend(record(0, MSG_SESSION, &session));

        let mut slot = Vec::new();
        slot.extend(record(2, F_MEMBER_ID, &1_234_567u64.to_be_bytes()));
        slot.extend(record(1, F_ENABLED, &1u32.to_be_bytes()));

        let mut network = Vec::new();
        network.extend(record(3, F_NETWORK_NAME, &text("LAN")));
        network.extend(record(1, F_MEMBER_COUNT, &2u32.to_be_bytes()));
        network.extend(record(0, MSG_MEMBER_SLOT, &slot));

        let collection = record(
            0,
            MSG_COLLECTION,
            &[record(0, MSG_NETWORK, &network), record(0, MSG_MEMBER, &member)].concat(),
        );
        let hierarchy = record(0, MSG_ROOT, &collection);

        let snapshot = decode_snapshot(&[hierarchy]).unwrap();
        assert_eq!(snapshot.networks.len(), 1);
        let network = &snapshot.networks[0];
        assert_eq!(network.name, "LAN");
        assert_eq!(network.declared_count, Some(2));
        assert_eq!(network.peers.len(), 1);
        let peer = &network.peers[0];
        assert_eq!(peer.rid, "1234567");
        assert_eq!(peer.name, "Host");
        assert_eq!(peer.ipv4.as_deref(), Some("26.10.0.7"));
        assert_eq!(peer.ipv6.as_deref(), Some("fdfd::1a0a:7"));
        assert_eq!(peer.public_address.as_deref(), Some("1.2.3.4"));
        assert_eq!(peer.state_code, 1);
        assert!(peer.online);
        assert_eq!(peer.channel.as_deref(), Some("TCP"));
        assert_eq!(peer.service_port, Some(25565));
        assert_eq!(
            peer.session_remote.as_deref(),
            Some("0.0.0.0:49678 -> 5.44.170.107:63918")
        );
    }

    #[test]
    fn marks_missing_member_as_offline_placeholder() {
        let mut slot = Vec::new();
        slot.extend(record(2, F_MEMBER_ID, &42u64.to_be_bytes()));
        let mut network = Vec::new();
        network.extend(record(3, F_NETWORK_NAME, &text("LAN")));
        network.extend(record(0, MSG_MEMBER_SLOT, &slot));
        let collection = record(0, MSG_COLLECTION, &record(0, MSG_NETWORK, &network));
        let snapshot = decode_snapshot(&[record(0, MSG_ROOT, &collection)]).unwrap();
        let peer = &snapshot.networks[0].peers[0];
        assert_eq!(peer.rid, "42");
        assert_eq!(peer.name, "未知成员");
        assert!(!peer.online);
    }

    #[test]
    fn join_payload_matches_documented_layout() {
        let mut payload = Vec::new();
        push_u32(&mut payload, 0x01, CMD_FIELD_MSG_ID, CMD_JOIN_NETWORK);
        push_u32(&mut payload, 0x01, CMD_FIELD_REQUEST_ID, 0xbeef);
        let mut inner = Vec::new();
        push_string(&mut inner, 0x03, F_NETWORK_NAME, "Minecraft [English 3]");
        push_u32(&mut inner, 0x0b, 0x033f, 0);
        push_string(&mut inner, 0x08, 0x030d, "secret");
        push_record(&mut payload, 0x00, MSG_JOIN_NETWORK, &inner);

        let records = parse_records(&payload).unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(as_u32(&records[0]), Some(3));
        assert_eq!(records[2].id, MSG_JOIN_NETWORK);
        let inner = parse_records(&records[2].value).unwrap();
        assert_eq!(
            find_string(&inner, F_NETWORK_NAME).as_deref(),
            Some("Minecraft [English 3]")
        );
        assert_eq!(find_u32(&inner, 0x033f), Some(0));
        assert_eq!(find_string(&inner, 0x030d).as_deref(), Some("secret"));
    }

    #[test]
    fn rejects_truncated_tlv() {
        assert!(parse_records(&[0, 0, 0, 3, 1]).is_err());
    }

    /// 真实管道自检：`cargo test live_snapshot -- --nocapture --ignored`
    #[test]
    #[ignore]
    fn live_snapshot() {
        let snapshot = read_snapshot().expect("读取真实管道失败");
        println!("本机 = {:?}", snapshot.self_node);
        for network in &snapshot.networks {
            let online = network.peers.iter().filter(|peer| peer.online).count();
            println!(
                "{:?} 成员={} 在线={} 声明={:?}",
                network.name,
                network.peers.len(),
                online,
                network.declared_count
            );
            for peer in network.peers.iter().take(3) {
                println!(
                    "  {} | {} | {:?} | {:?} | {:?} | state={} | {:?}",
                    peer.rid,
                    peer.name,
                    peer.ipv4,
                    peer.ipv6,
                    peer.public_address,
                    peer.state_code,
                    peer.channel
                );
            }
        }
    }

    /// 导出真实快照给前端预览用：`cargo test dump_snapshot_json -- --ignored`
    #[test]
    #[ignore]
    fn dump_snapshot_json() {
        let snapshot = read_snapshot().expect("读取真实管道失败");
        let json = serde_json::to_string(&snapshot).expect("序列化失败");
        let path = std::path::Path::new("../public/snapshot.json");
        std::fs::write(path, json).expect("写入失败");
        println!("已写入 {}", path.display());
    }

    /// 真实加网回执自检（虚构网络名，服务端只会拒绝）：`cargo test live_join_reply -- --nocapture --ignored`
    #[test]
    #[ignore]
    fn live_join_reply() {
        let reply = join_network("ZZ_HUTAOBOX_SELFTEST_NOT_EXIST", "", 0).expect("发送加网请求失败");
        println!("回执 = {reply:?}");
        assert_eq!(reply.msg_id, CMD_JOIN_NETWORK);
        assert_ne!(reply.code, 0, "虚构网络名不应加网成功");
    }

    /// 真实 ICMP 自检：`cargo test live_ping -- --nocapture --ignored`
    #[test]
    #[ignore]
    fn live_ping() {
        let snapshot = read_snapshot().expect("读取真实管道失败");
        let hosts: Vec<String> = snapshot
            .networks
            .iter()
            .flat_map(|network| network.peers.iter())
            .filter(|peer| peer.online)
            .filter_map(|peer| peer.ipv4.clone())
            .take(24)
            .collect();
        println!("探测主机数 = {}", hosts.len());
        let results = ping_hosts(&hosts, 1200);
        println!("收到回应 {} 个: {:?}", results.len(), results);
    }
}
