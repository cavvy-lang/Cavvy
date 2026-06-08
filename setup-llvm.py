#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Cavvy LLVM Minimal Setup Script
跨平台LLVM最小化安装脚本
支持Windows和Linux x86_64平台
"""

import os
import sys
import platform
import urllib.request
import tarfile
import shutil
import configparser
from pathlib import Path
from typing import Optional, Tuple

# 强制使用UTF-8编码输出（解决Windows中文编码问题）
if sys.platform == "win32":
    import io
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace')
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8', errors='replace')


# ============ 配置区域 ============
CONFIG = {
    "github_repo": "cavvy-lang/Cavvy-src-Assets",
    "verinfo_path": ".verinfo",
    "install_dir": "llvm-minimal",
    "timeout_seconds": 300,
}


# ============ 工具函数 ============

def log_info(msg: str) -> None:
    """输出信息日志"""
    print(f"[INFO] {msg}")


def log_error(msg: str) -> None:
    """输出错误日志"""
    print(f"[ERROR] {msg}", file=sys.stderr)


def log_success(msg: str) -> None:
    """输出成功日志"""
    print(f"[SUCCESS] {msg}")


def detect_platform() -> Tuple[str, str]:
    """
    检测当前操作系统和架构
    返回: (os_name, arch)
    """
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "windows":
        os_name = "win"
    elif system == "linux":
        os_name = "linux"
    else:
        raise RuntimeError(f"不支持的操作系统: {system}")

    # 标准化架构名称
    if machine in ("amd64", "x86_64", "x64"):
        arch = "x86_64"
    else:
        raise RuntimeError(f"不支持的架构: {machine}")

    return os_name, arch


def parse_verinfo() -> Optional[str]:
    """
    解析.verinfo文件获取LLVM-MINIMAL版本号
    时间复杂度: O(n), n为文件行数
    空间复杂度: O(1)
    """
    verinfo_path = Path(CONFIG["verinfo_path"])

    if not verinfo_path.exists():
        log_error(f"版本信息文件不存在: {verinfo_path}")
        return None

    try:
        config = configparser.ConfigParser()
        config.read(verinfo_path, encoding="utf-8")

        if "LLVM-MINIMAL" not in config.sections():
            log_error("verinfo中缺少[LLVM-MINIMAL]节")
            return None

        version = config["LLVM-MINIMAL"].get("version")
        if not version:
            log_error("verinfo中LLVM-MINIMAL版本号为空")
            return None

        # 去除可能的引号
        version = version.strip().strip('"').strip("'")
        return version

    except Exception as e:
        log_error(f"解析verinfo文件失败: {e}")
        return None


def build_download_url(version: str, os_name: str, arch: str) -> str:
    """
    构建下载URL
    URL格式: https://github.com/{repo}/releases/download/llvm-minimal/{version}/{os}-{arch}/bin/{bin_name}.tar.xz
    """
    bin_name = "bin" if os_name == "win" else "bin-linux"
    url = (
        f"https://github.com/{CONFIG['github_repo']}/releases/download/"
        f"llvm-minimal/{version}/{os_name}-{arch}/bin/{bin_name}.tar.xz"
    )
    return url


def download_file(url: str, dest_path: Path, timeout: int = 300) -> bool:
    """
    下载文件到指定路径
    时间复杂度: O(n), n为文件大小
    磁盘IO: 顺序写入
    """
    log_info(f"下载: {url}")
    log_info(f"目标: {dest_path}")

    try:
        # 创建临时文件（原子写入准备）
        temp_path = dest_path.with_suffix(".tmp")

        # 确保目标目录存在
        dest_path.parent.mkdir(parents=True, exist_ok=True)

        # 下载文件
        req = urllib.request.Request(url, headers={
            "User-Agent": "Cavvy-LLVM-Setup/1.0"
        })

        with urllib.request.urlopen(req, timeout=timeout) as response:
            if response.status != 200:
                log_error(f"HTTP错误: {response.status}")
                return False

            total_size = int(response.headers.get("Content-Length", 0))
            downloaded = 0
            chunk_size = 8192  # 8KB chunks

            with open(temp_path, "wb") as f:
                while True:
                    chunk = response.read(chunk_size)
                    if not chunk:
                        break
                    f.write(chunk)
                    downloaded += len(chunk)

                    # 显示进度
                    if total_size > 0:
                        percent = (downloaded / total_size) * 100
                        sys.stdout.write(f"\r  进度: {percent:.1f}% ({downloaded}/{total_size} bytes)")
                        sys.stdout.flush()

        print()  # 换行

        # 验证下载完整性
        if total_size > 0 and temp_path.stat().st_size != total_size:
            log_error("下载文件大小不匹配")
            temp_path.unlink(missing_ok=True)
            return False

        # 原子重命名
        temp_path.replace(dest_path)
        log_success(f"下载完成: {dest_path.stat().st_size} bytes")
        return True

    except urllib.error.HTTPError as e:
        log_error(f"HTTP错误 {e.code}: {e.reason}")
        return False
    except urllib.error.URLError as e:
        log_error(f"URL错误: {e.reason}")
        return False
    except Exception as e:
        log_error(f"下载失败: {e}")
        # 清理临时文件
        if "temp_path" in dir():
            temp_path.unlink(missing_ok=True)
        return False


def extract_tar_xz(archive_path: Path, extract_to: Path) -> bool:
    """
    解压.tar.xz文件
    时间复杂度: O(n), n为归档内容大小
    磁盘IO: 顺序读取，随机写入
    """
    log_info(f"解压: {archive_path}")
    log_info(f"目标目录: {extract_to}")

    try:
        # 确保目标目录存在
        extract_to.mkdir(parents=True, exist_ok=True)

        # 打开并解压tar.xz文件
        with tarfile.open(archive_path, "r:xz") as tar:
            # 安全检查：防止路径遍历攻击
            for member in tar.getmembers():
                member_path = extract_to / member.name
                try:
                    member_path.resolve().relative_to(extract_to.resolve())
                except ValueError:
                    log_error(f"检测到不安全的路径遍历: {member.name}")
                    return False

            # 执行解压
            tar.extractall(path=extract_to)

        log_success("解压完成")
        return True

    except tarfile.TarError as e:
        log_error(f"tar文件错误: {e}")
        return False
    except Exception as e:
        log_error(f"解压失败: {e}")
        return False


def verify_installation(install_dir: Path, os_name: str) -> bool:
    """
    验证LLVM安装是否成功
    检查关键二进制文件是否存在
    """
    log_info("验证安装...")

    # 关键二进制文件列表
    essential_bins = ["clang", "ld.lld", "ld64.lld", "llc", "lld-link", "lld", "llvm-ar", "llvm-profdata", "llvm-profgen", "wasm-ld"]

    bin_dir = install_dir / "bin"
    if not bin_dir.exists():
        log_error(f"bin目录不存在: {bin_dir}")
        return False

    missing = []
    for binary in essential_bins:
        # Windows下添加.exe后缀
        exe_suffix = ".exe" if os_name == "win" else ""
        binary_path = bin_dir / f"{binary}{exe_suffix}"

        if not binary_path.exists():
            missing.append(binary)

    if missing:
        log_error(f"缺少关键二进制文件: {', '.join(missing)}")
        return False

    log_success("安装验证通过")
    return True


def cleanup(archive_path: Path) -> None:
    """清理临时文件"""
    if archive_path.exists():
        archive_path.unlink()
        log_info(f"清理临时文件: {archive_path}")


def setup_environment(install_dir: Path, os_name: str) -> None:
    """
    输出环境变量设置提示
    """
    bin_path = install_dir.resolve() / "bin"

    log_info("环境变量设置:")
    print()

    if os_name == "win":
        print("PowerShell:")
        print(f'  $env:PATH = "{bin_path};" + $env:PATH')
        print()
        print("CMD:")
        print(f'  set PATH={bin_path};%PATH%')
        print()
        print("永久设置 (PowerShell管理员):")
        print(f'  [Environment]::SetEnvironmentVariable("Path", "{bin_path};" + [Environment]::GetEnvironmentVariable("Path", "User"), "User")')
    else:
        print("Bash/Zsh:")
        print(f'  export PATH="{bin_path}:$PATH"')
        print()
        print("永久设置 (添加到 ~/.bashrc 或 ~/.zshrc):")
        print(f'  echo \'export PATH="{bin_path}:$PATH"\' >> ~/.bashrc')

    print()


# ============ 主流程 ============

def main() -> int:
    """
    主入口函数
    返回: 退出码 (0=成功, 1=失败)
    """
    log_info("Cavvy LLVM Minimal Setup")
    log_info("=" * 50)

    # 1. 检测平台
    try:
        os_name, arch = detect_platform()
        log_info(f"检测到平台: {os_name}-{arch}")
    except RuntimeError as e:
        log_error(str(e))
        return 1

    # 2. 解析版本信息
    version = parse_verinfo()
    if not version:
        return 1
    log_info(f"LLVM-MINIMAL版本: {version}")

    # 3. 构建URL和路径
    url = build_download_url(version, os_name, arch)
    install_dir = Path(CONFIG["install_dir"])
    archive_name = f"llvm-minimal-{version}-{os_name}-{arch}.tar.xz"
    archive_path = install_dir / archive_name

    # 4. 检查是否已安装
    if verify_installation(install_dir, os_name):
        log_info("LLVM已安装，跳过下载")
        setup_environment(install_dir, os_name)
        return 0

    # 5. 下载压缩包
    if not download_file(url, archive_path, CONFIG["timeout_seconds"]):
        log_error("下载失败，请检查网络连接和版本号")
        return 1

    # 6. 解压
    if not extract_tar_xz(archive_path, install_dir):
        log_error("解压失败")
        cleanup(archive_path)
        return 1

    # 7. 验证安装
    if not verify_installation(install_dir, os_name):
        log_error("安装验证失败")
        cleanup(archive_path)
        return 1

    # 8. 清理临时文件
    cleanup(archive_path)

    # 9. 输出环境变量设置
    setup_environment(install_dir, os_name)

    log_success("LLVM安装完成!")
    return 0


if __name__ == "__main__":
    sys.exit(main())
