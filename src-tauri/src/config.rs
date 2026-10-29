use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct RegistryValueSnapshot {
    pub name: String,
    pub value_type: u32,
    pub bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RegistryKeySnapshot {
    pub hive: String,
    pub path: String,
    pub values: Vec<RegistryValueSnapshot>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RadminAccount {
    pub alias: String,
    pub ipv4: String,
    #[serde(default)]
    pub ipv6: String,
    pub rid: String,
    #[serde(
        default,
        rename = "ServerPassword",
        alias = "server_password",
        alias = "serverpassword",
        alias = "password",
        with = "server_password_serde"
    )]
    pub server_password: Vec<u8>,
    #[serde(default, skip_serializing)]
    pub registry: Vec<RegistryKeySnapshot>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AccountProfile {
    pub file: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Default)]
struct AccountProfileIndex {
    profiles: Vec<AccountProfile>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub effect: u8,
    pub opacity: u8,
    pub window_alpha: u8,
    pub theme_index: u8,
    pub top: bool,
    pub thin_border: bool,
    pub public_broadcast: bool,
    pub broadcast_motd: String,
    pub broadcast_count: String,
    pub broadcast_prefix: String,
    pub port_pad: bool,
    pub scan_max_per_ip: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            effect: 4,
            opacity: 30,
            window_alpha: 25,
            theme_index: 0,
            top: false,
            thin_border: true,
            public_broadcast: true,
            broadcast_motd: "服务器{num}".to_string(),
            broadcast_count: "10".to_string(),
            broadcast_prefix: "".to_string(),
            port_pad: false,
            scan_max_per_ip: 3,
        }
    }
}

pub fn config_path() -> PathBuf {
    let mut p = dirs_next();
    p.push("crafterbox-config.json");
    p
}

pub fn accounts_path() -> PathBuf {
    let mut p = dirs_next();
    p.push("accounts.json");
    p
}

fn dirs_next() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(appdata).join("crafterbox");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    if !path.exists() {
        return AppConfig::default();
    }
    match fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config(cfg: &AppConfig) -> Result<(), String> {
    let path = config_path();
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_accounts() -> Result<Vec<RadminAccount>, String> {
    let path = accounts_path();
    if !path.exists() {
        return Ok(vec![]);
    }
    let contents = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let mut accounts: Vec<RadminAccount> =
        serde_json::from_str(&contents).map_err(|error| error.to_string())?;
    normalize_accounts(&mut accounts);
    let formatted = serde_json::to_string_pretty(&accounts).map_err(|error| error.to_string())?;
    if contents != formatted {
        fs::write(&path, formatted).map_err(|error| error.to_string())?;
    }
    Ok(accounts)
}

pub fn save_accounts(accounts: &[RadminAccount]) -> Result<(), String> {
    let mut accounts = accounts.to_vec();
    normalize_accounts(&mut accounts);
    validate_accounts(&accounts)?;
    let path = accounts_path();
    let json = serde_json::to_string_pretty(&accounts).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn import_account_profile(contents: &str) -> Result<String, String> {
    if contents.len() > 1024 * 1024 {
        return Err("配置文件超过 1 MB，无法导入".to_string());
    }
    let mut accounts: Vec<RadminAccount> =
        serde_json::from_str(contents).map_err(|error| format!("配置文件格式无效：{error}"))?;
    normalize_accounts(&mut accounts);
    validate_accounts(&accounts)?;
    write_new_account_profile(&accounts, "保存导入配置失败")
}

pub fn save_account_profile(accounts: &[RadminAccount]) -> Result<String, String> {
    let mut accounts = accounts.to_vec();
    normalize_accounts(&mut accounts);
    validate_accounts(&accounts)?;
    write_new_account_profile(&accounts, "保存配置文件失败")
}

fn write_new_account_profile(
    accounts: &[RadminAccount],
    error_prefix: &str,
) -> Result<String, String> {
    let dir = dirs_next();
    for index in 1..=9999 {
        let file_name = if index == 1 {
            "import-config.json".to_string()
        } else {
            format!("import-config#{index}.json")
        };
        let path = dir.join(&file_name);
        if !path.exists() {
            let json = serde_json::to_string_pretty(accounts).map_err(|error| error.to_string())?;
            fs::write(path, json).map_err(|error| format!("{error_prefix}：{error}"))?;
            list_account_profiles()?;
            return Ok(file_name);
        }
    }
    Err("配置文件数量已达上限".to_string())
}

fn validate_accounts(accounts: &[RadminAccount]) -> Result<(), String> {
    if accounts.len() > 1000 {
        return Err("配置文件账号数量超过 1000 条".to_string());
    }
    for account in accounts {
        if account.alias.trim().is_empty() {
            return Err("账号名称不能为空".to_string());
        }
        if !account.rid.is_empty() && !account.rid.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(format!("账号 {} 的 RID 必须是十进制数字", account.alias));
        }
        if account.server_password.len() > 4096 {
            return Err(format!(
                "账号 {} 的 ServerPassword 超出 4096 字节",
                account.alias
            ));
        }
    }
    Ok(())
}

pub fn list_account_profiles() -> Result<Vec<AccountProfile>, String> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dirs_next()).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_account_profile_name(&name) && entry.path().is_file() {
            let path = entry.path();
            let contents = fs::read_to_string(&path).map_err(|error| error.to_string())?;
            if contents.len() > 1024 * 1024 {
                return Err(format!("配置文件 {name} 超过 1 MB，无法读取"));
            }
            let mut accounts: Vec<RadminAccount> = serde_json::from_str(&contents)
                .map_err(|error| format!("配置文件 {name} 格式无效：{error}"))?;
            normalize_accounts(&mut accounts);
            validate_accounts(&accounts)?;
            let formatted =
                serde_json::to_string_pretty(&accounts).map_err(|error| error.to_string())?;
            if contents != formatted {
                fs::write(path, formatted)
                    .map_err(|error| format!("无法整理配置文件 {name}：{error}"))?;
            }
            files.push(name);
        }
    }
    files.sort_by_key(|name| profile_number(name));
    let mut index = load_profile_index()?;
    let mut ordered = Vec::new();
    for profile in &index.profiles {
        if files.contains(&profile.file)
            && !ordered
                .iter()
                .any(|item: &AccountProfile| item.file == profile.file)
        {
            ordered.push(profile.clone());
        }
    }
    for file in files {
        if !ordered.iter().any(|profile| profile.file == file) {
            ordered.push(AccountProfile {
                file: file.clone(),
                name: file,
            });
        }
    }
    if index.profiles != ordered {
        index.profiles = ordered.clone();
        save_profile_index(&index.profiles)?;
    }
    Ok(ordered)
}

pub fn read_account_profile(file: &str) -> Result<String, String> {
    if !is_account_profile_name(file) {
        return Err("配置文件名无效".to_string());
    }
    let contents = fs::read_to_string(dirs_next().join(file)).map_err(|error| error.to_string())?;
    let mut accounts: Vec<RadminAccount> =
        serde_json::from_str(&contents).map_err(|error| format!("配置文件格式无效：{error}"))?;
    normalize_accounts(&mut accounts);
    validate_accounts(&accounts)?;
    serde_json::to_string_pretty(&accounts).map_err(|error| error.to_string())
}

pub fn activate_account_profile(file: &str) -> Result<Vec<RadminAccount>, String> {
    let contents = read_account_profile(file)?;
    let accounts: Vec<RadminAccount> =
        serde_json::from_str(&contents).map_err(|error| error.to_string())?;
    save_accounts(&accounts)?;
    Ok(accounts)
}

pub fn delete_account_profile(file: &str) -> Result<(), String> {
    if !is_account_profile_name(file) {
        return Err("配置文件名无效".to_string());
    }
    fs::remove_file(dirs_next().join(file)).map_err(|error| error.to_string())?;
    let profiles = list_account_profiles()?
        .into_iter()
        .filter(|profile| profile.file != file)
        .collect::<Vec<_>>();
    save_profile_index(&profiles)
}

pub fn rename_account_profile(file: &str, name: &str) -> Result<String, String> {
    if !is_account_profile_name(file) {
        return Err("配置文件名无效".to_string());
    }
    let name = name.trim().trim_end_matches(".json");
    if name.is_empty()
        || name.len() > 80
        || name
            .chars()
            .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
        || name.ends_with('.')
    {
        return Err("配置文件名称需为 1 到 80 个字符".to_string());
    }
    let new_file = format!("import-config-{name}.json");
    if new_file != file && dirs_next().join(&new_file).exists() {
        return Err("同名配置文件已存在".to_string());
    }
    let mut profiles = list_account_profiles()?;
    let profile = profiles
        .iter_mut()
        .find(|profile| profile.file == file)
        .ok_or_else(|| "配置文件不存在".to_string())?;
    if new_file != file {
        fs::rename(dirs_next().join(file), dirs_next().join(&new_file))
            .map_err(|error| format!("重命名配置文件失败：{error}"))?;
    }
    profile.file = new_file.clone();
    profile.name = name.to_string();
    save_profile_index(&profiles)?;
    Ok(new_file)
}

pub fn move_account_profile(file: &str, direction: i32) -> Result<(), String> {
    if !is_account_profile_name(file) || !matches!(direction, -1 | 1) {
        return Err("配置文件排序请求无效".to_string());
    }
    let mut profiles = list_account_profiles()?;
    let index = profiles
        .iter()
        .position(|profile| profile.file == file)
        .ok_or_else(|| "配置文件不存在".to_string())?;
    let target = index as i32 + direction;
    if target < 0 || target >= profiles.len() as i32 {
        return Ok(());
    }
    profiles.swap(index, target as usize);
    save_profile_index(&profiles)
}

fn is_account_profile_name(name: &str) -> bool {
    if name == "import-config.json" {
        return true;
    }
    if let Some(index) = name
        .strip_prefix("import-config#")
        .and_then(|suffix| suffix.strip_suffix(".json"))
        .and_then(|index| index.parse::<u32>().ok())
    {
        return index > 1;
    }
    name.strip_prefix("import-config-")
        .and_then(|suffix| suffix.strip_suffix(".json"))
        .map(|stem| {
            !stem.is_empty()
                && !stem
                    .chars()
                    .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
                && !stem.ends_with('.')
        })
        .unwrap_or(false)
}

fn profile_index_path() -> PathBuf {
    dirs_next().join("profiles-index.json")
}

fn load_profile_index() -> Result<AccountProfileIndex, String> {
    let path = profile_index_path();
    if !path.exists() {
        return Ok(AccountProfileIndex::default());
    }
    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&contents).map_err(|error| error.to_string())
}

fn save_profile_index(profiles: &[AccountProfile]) -> Result<(), String> {
    let index = AccountProfileIndex {
        profiles: profiles.to_vec(),
    };
    let contents = serde_json::to_string(&index).map_err(|error| error.to_string())?;
    fs::write(profile_index_path(), contents).map_err(|error| error.to_string())
}

fn profile_number(name: &str) -> u32 {
    if name == "import-config.json" {
        return 1;
    }
    name.strip_prefix("import-config#")
        .and_then(|suffix| suffix.strip_suffix(".json"))
        .and_then(|number| number.parse().ok())
        .unwrap_or(u32::MAX)
}

mod server_password_serde {
    use serde::de::{Error, SeqAccess, Visitor};
    use serde::{Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut encoded = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
        serializer.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PasswordVisitor;

        impl<'de> Visitor<'de> for PasswordVisitor {
            type Value = Vec<u8>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a hexadecimal string or an array of bytes")
            }

            fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.len() % 2 != 0 {
                    return Err(E::custom("ServerPassword 的十六进制长度必须为偶数"));
                }
                value
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|pair| {
                        let high = hex_digit(pair[0])
                            .ok_or_else(|| E::custom("ServerPassword 含无效十六进制"))?;
                        let low = hex_digit(pair[1])
                            .ok_or_else(|| E::custom("ServerPassword 含无效十六进制"))?;
                        Ok((high << 4) | low)
                    })
                    .collect()
            }

            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut bytes = Vec::new();
                while let Some(byte) = sequence.next_element::<u8>()? {
                    bytes.push(byte);
                    if bytes.len() > 4096 {
                        return Err(A::Error::custom("ServerPassword 超出 4096 字节"));
                    }
                }
                Ok(bytes)
            }
        }

        deserializer.deserialize_any(PasswordVisitor)
    }

    fn hex_digit(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
}

fn normalize_accounts(accounts: &mut [RadminAccount]) {
    for account in accounts {
        if account.server_password.is_empty() {
            account.server_password = account
                .registry
                .iter()
                .flat_map(|key| &key.values)
                .find(|value| {
                    value.name.eq_ignore_ascii_case("ServerPassword")
                        && value.value_type == winreg::enums::REG_BINARY as u32
                })
                .map(|value| value.bytes.clone())
                .unwrap_or_default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RadminAccount;

    #[test]
    fn account_json_contains_only_compact_account_fields() {
        let account = RadminAccount {
            alias: "WIN大号".to_string(),
            ipv4: "26.236.137.244".to_string(),
            ipv6: "fdfd::1aec:89f4".to_string(),
            rid: "186493791".to_string(),
            server_password: vec![1, 2, 3],
            registry: Vec::new(),
        };
        let json = serde_json::to_string_pretty(&vec![account]).unwrap();
        assert!(json.contains("\"ipv6\": \"fdfd::1aec:89f4\""));
        assert!(json.contains("\"ServerPassword\": \"010203\""));
        assert!(!json.contains("registry"));
        let parsed: Vec<RadminAccount> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed[0].server_password, [1, 2, 3]);
    }

    #[test]
    fn reads_legacy_password_byte_arrays() {
        let json = r#"[{"alias":"old","ipv4":"1.2.3.4","rid":"10","password":[1,2,3]}]"#;
        let accounts: Vec<RadminAccount> = serde_json::from_str(json).unwrap();
        assert_eq!(accounts[0].server_password, [1, 2, 3]);
    }

    #[test]
    fn reads_hexadecimal_server_passwords_after_restart() {
        let json = r#"[{"alias":"account","ipv4":"1.2.3.4","rid":"10","ServerPassword":"51682268b765320f9251449945e5b384e9868aa5"}]"#;
        let accounts: Vec<RadminAccount> = serde_json::from_str(json).unwrap();
        assert_eq!(
            accounts[0].server_password,
            [
                0x51, 0x68, 0x22, 0x68, 0xb7, 0x65, 0x32, 0x0f, 0x92, 0x51, 0x44, 0x99, 0x45, 0xe5,
                0xb3, 0x84, 0xe9, 0x86, 0x8a, 0xa5
            ]
        );
    }
}
