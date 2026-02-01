use crate::player::PlayerDesk;
use std::collections::HashMap;
use std::fs;

pub fn load_desks() -> HashMap<String, PlayerDesk> {
    let mut res = HashMap::new();
    for file in fs::read_dir("desks").unwrap() {
        if let Ok(file) = file {
            if file.path().is_file() {
                if let Some(file_name) = file.file_name().to_str() {
                    let code = fs::read_to_string(file.path()).unwrap();
                    let codes: Vec<String> = code
                        .trim()
                        .split("\n")
                        .map(|x| x.trim())
                        .filter(|part| !part.is_empty()) // 过滤空字符串
                        .map(|s| s.to_string()) // 转换为 String
                        .collect();
                    res.insert(file_name.to_string(), PlayerDesk(codes));
                }
            }
        }
    }
    res
}
