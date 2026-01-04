// src/main.rs

use loom::Driver;
use std::env;
use std::path::PathBuf; // 假设你的 crate 名字叫 loom_lang，或者是 use crate::Driver

fn main() {
    // 1. 获取命令行参数
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Usage: loom <script.lm>");
        return;
    }

    let filename = &args[1];
    let path = PathBuf::from(filename);

    if !path.exists() {
        eprintln!("Error: File '{}' not found.", filename);
        std::process::exit(1);
    }

    // 2. 初始化 Driver
    // 假设当前目录是项目根目录，或者你可以把 path 的父目录作为 root
    let root_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let mut driver = Driver::new(root_dir);

    // 3. 运行
    match driver.run_file(&path) {
        Ok(_) => {
            // 程序正常结束
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}
