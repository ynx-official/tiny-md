//! Release-only tool. Key material is read from a file or environment, never argv.
use anyhow::{Context, Result, ensure};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use std::{fs::OpenOptions, io::Write, path::Path};
use tiny_md_updater::{
    parse_manifest,
    signing::{PUBLIC_KEY, decode_hex, encode_hex, verify},
};

fn create_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(encode_hex(bytes).as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 2 && args[0] == "--verify" {
        let bytes = std::fs::read(&args[1])?;
        parse_manifest(std::str::from_utf8(&bytes)?)?;
        verify(
            &bytes,
            &std::fs::read_to_string(format!("{}.sig", args[1]))?,
            PUBLIC_KEY,
        )?;
        println!("发布者签名验证通过");
        return Ok(());
    }
    if args.len() == 3 && args[0] == "--generate-key" {
        let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .map_err(|_| anyhow::anyhow!("生成签名密钥失败"))?;
        let pair = Ed25519KeyPair::from_pkcs8(document.as_ref())
            .map_err(|_| anyhow::anyhow!("密钥无效"))?;
        // On Windows the caller must provision a private ACL on the parent first.
        ensure!(
            !Path::new(&args[2]).exists() || std::fs::read_to_string(&args[2])?.trim().is_empty(),
            "拒绝覆盖已有发布公钥"
        );
        create_private(Path::new(&args[1]), document.as_ref())?;
        std::fs::write(
            &args[2],
            format!("{}\n", encode_hex(pair.public_key().as_ref())),
        )?;
        println!("签名密钥已生成；私钥未输出，请安全备份。");
        return Ok(());
    }
    ensure!(
        args.len() == 1,
        "Usage: sign_manifest MANIFEST | --verify MANIFEST | --generate-key PRIVATE_FILE PUBLIC_FILE"
    );
    let private =
        std::env::var("TINY_MD_UPDATE_SIGNING_KEY").context("缺少 TINY_MD_UPDATE_SIGNING_KEY")?;
    let pair = Ed25519KeyPair::from_pkcs8(&decode_hex(&private)?)
        .map_err(|_| anyhow::anyhow!("签名私钥格式无效"))?;
    ensure!(
        encode_hex(pair.public_key().as_ref()) == PUBLIC_KEY.trim(),
        "签名私钥与客户端内置公钥不匹配"
    );
    let bytes = std::fs::read(&args[0])?;
    parse_manifest(std::str::from_utf8(&bytes)?)?;
    let signature = encode_hex(pair.sign(&bytes).as_ref());
    verify(&bytes, &signature, PUBLIC_KEY)?;
    std::fs::write(format!("{}.sig", args[0]), format!("{signature}\n"))?;
    println!("更新清单已签名并验证");
    Ok(())
}
