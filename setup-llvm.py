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
import subprocess
from pathlib import Path
from typing import Optional, Tuple

# 强制使用UTF-8编码输出（解决Windows中文编码问题）
if sys.platform == "win32":
    import io
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace')
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8', errors='replace')


def is_ci_environment() -> bool:
    """检测是否在CI环境中运行"""
    ci_env_vars = [
        "CI", "GITHUB_ACTIONS", "GITLAB_CI", "TRAVIS", "CIRCLECI",
        "APPVEYOR", "BUILDKITE", "DRONE", "JENKINS_URL", "TF_BUILD"
    ]
    return any(os.environ.get(var) for var in ci_env_vars)


# ============ 配置区域 ============
CONFIG = {
    "github_repo": "cavvy-lang/Cavvy-src-Assets",
    "verinfo_path": ".verinfo",
    "install_dir": "llvm-minimal",
    "timeout_seconds": 300,
}

# LLVM官方发布URL模板 (用于完整开发包下载)
# 版本221对应LLVM 22.1.x
# Windows使用7z压缩包（ Portable 版本），可以直接解压到指定目录
LLVM_OFFICIAL_URLS = {
    "win": {
        "x86_64": "https://github.com/llvm/llvm-project/releases/download/llvmorg-{version}/LLVM-{version}-win64.7z",
    },
    "linux": {
        "x86_64": "https://github.com/llvm/llvm-project/releases/download/llvmorg-{version}/clang+llvm-{version}-x86_64-linux-gnu-ubuntu-22.04.tar.xz",
    },
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


def build_download_url(version: str, os_name: str, arch: str, full_llvm: bool = False) -> str:
    """
    构建下载URL
    URL格式: https://github.com/{repo}/releases/download/llvm-minimal/{version}/{os}-{arch}/bin/{bin_name}.tar.xz

    参数:
        version: LLVM版本号
        os_name: 操作系统名称 (win/linux)
        arch: 架构 (x86_64)
        full_llvm: 是否下载完整LLVM开发包（仅CI环境使用，约2GB）
    """
    if full_llvm:
        # 使用LLVM官方完整开发包
        # 将版本号从 22.1.6 转换为 22.1.6
        llvm_version = version
        if os_name in LLVM_OFFICIAL_URLS and arch in LLVM_OFFICIAL_URLS[os_name]:
            return LLVM_OFFICIAL_URLS[os_name][arch].format(version=llvm_version)
        else:
            log_error(f"不支持的平台/架构用于完整LLVM下载: {os_name}-{arch}")
            # 回退到minimal版本

    # 使用Cavvy minimal版本（仅二进制工具）
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
            last_percent = -1
            ci_mode = is_ci_environment()

            with open(temp_path, "wb") as f:
                while True:
                    chunk = response.read(chunk_size)
                    if not chunk:
                        break
                    f.write(chunk)
                    downloaded += len(chunk)

                    # 显示进度（CI环境下每10%更新一次，避免刷屏）
                    if total_size > 0:
                        percent = int((downloaded / total_size) * 100)
                        if ci_mode:
                            # CI环境：每10%更新一次
                            if percent // 10 > last_percent // 10:
                                sys.stdout.write(f"\r  进度: {percent}% ({downloaded}/{total_size} bytes)")
                                sys.stdout.flush()
                                last_percent = percent
                        else:
                            # 本地环境：每1%更新
                            if percent > last_percent:
                                sys.stdout.write(f"\r  进度: {percent}% ({downloaded}/{total_size} bytes)")
                                sys.stdout.flush()
                                last_percent = percent

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
    解压.tar.xz文件到bin子目录
    时间复杂度: O(n), n为归档内容大小
    磁盘IO: 顺序读取，随机写入
    """
    log_info(f"解压: {archive_path}")
    log_info(f"目标目录: {extract_to}")

    try:
        # 确保bin目标目录存在
        bin_dir = extract_to / "bin"
        bin_dir.mkdir(parents=True, exist_ok=True)

        # 打开并解压tar.xz文件到bin目录
        with tarfile.open(archive_path, "r:xz") as tar:
            # 安全检查：防止路径遍历攻击
            for member in tar.getmembers():
                member_path = bin_dir / member.name
                try:
                    member_path.resolve().relative_to(bin_dir.resolve())
                except ValueError:
                    log_error(f"检测到不安全的路径遍历: {member.name}")
                    return False

            # 执行解压到bin目录
            tar.extractall(path=bin_dir)

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

def extract_7z(archive_path: Path, extract_to: Path) -> bool:
    """
    解压.7z文件
    需要系统中安装7z或7za命令
    """
    log_info(f"解压7z: {archive_path}")
    log_info(f"目标目录: {extract_to}")

    try:
        # 确保目标目录存在
        extract_to.mkdir(parents=True, exist_ok=True)

        # 尝试使用7z或7za命令
        for cmd in ["7z", "7za"]:
            result = subprocess.run(
                [cmd, "x", str(archive_path), f"-o{extract_to}", "-y"],
                capture_output=True,
                text=True
            )
            if result.returncode == 0:
                log_success("7z解压完成")
                return True

        # 如果没有7z命令，尝试使用Python的py7zr库
        try:
            import py7zr
            with py7zr.SevenZipFile(archive_path, mode='r') as z:
                z.extractall(path=extract_to)
            log_success("7z解压完成 (py7zr)")
            return True
        except ImportError:
            log_error("未找到7z命令或py7zr库，请安装7-Zip或运行: pip install py7zr")
            return False

    except Exception as e:
        log_error(f"解压7z失败: {e}")
        return False


def download_and_install_llvm(
    version: str,
    os_name: str,
    arch: str,
    install_dir: Path,
    full_llvm: bool = False
) -> bool:
    """
    下载并安装LLVM

    参数:
        version: LLVM版本号
        os_name: 操作系统名称
        arch: 架构
        install_dir: 安装目录
        full_llvm: 是否下载完整开发包

    返回: 是否成功
    """
    # 构建URL
    url = build_download_url(version, os_name, arch, full_llvm=full_llvm)

    if full_llvm:
        log_info("使用LLVM官方完整开发包（约2GB）")
        # 完整包使用不同的文件名
        if os_name == "win":
            archive_name = f"LLVM-{version}-win64.7z"
        else:
            archive_name = f"clang+llvm-{version}-{arch}-linux-gnu-ubuntu-22.04.tar.xz"
        archive_path = install_dir / archive_name
    else:
        archive_name = f"llvm-minimal-{version}-{os_name}-{arch}.tar.xz"
        archive_path = install_dir / archive_name

    # 下载压缩包
    if not download_file(url, archive_path, CONFIG["timeout_seconds"]):
        log_error("下载失败，请检查网络连接和版本号")
        return False

    # 解压
    if full_llvm and os_name == "win":
        # Windows完整包是.7z格式，需要解压
        if not extract_7z(archive_path, install_dir):
            log_error("解压7z失败")
            cleanup(archive_path)
            return False
    else:
        # 解压tar.xz
        if not extract_tar_xz(archive_path, install_dir):
            log_error("解压失败")
            cleanup(archive_path)
            return False

    # 清理临时文件
    cleanup(archive_path)

    return True


def main() -> int:
    """
    主入口函数
    返回: 退出码 (0=成功, 1=失败)
    """
    log_info("Cavvy LLVM Minimal Setup")
    log_info("=" * 50)

    # 检测是否使用完整LLVM开发包（CI环境可配置）
    use_full_llvm = os.environ.get("CAVVY_USE_FULL_LLVM", "").lower() in ("1", "true", "yes")
    if use_full_llvm:
        log_info("检测到CAVVY_USE_FULL_LLVM=1，将下载完整LLVM开发包")

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

    install_dir = Path(CONFIG["install_dir"])

    # 3. 检查是否已安装
    if verify_installation(install_dir, os_name):
        log_info("LLVM已安装，跳过下载")
        setup_environment(install_dir, os_name)
        return 0

    # 4. 下载并安装
    if not download_and_install_llvm(version, os_name, arch, install_dir, full_llvm=use_full_llvm):
        # 如果完整包下载失败，尝试minimal版本
        if use_full_llvm:
            log_info("完整包下载失败，尝试minimal版本...")
            if not download_and_install_llvm(version, os_name, arch, install_dir, full_llvm=False):
                return 1
        else:
            return 1

    # 5. 验证安装
    if not verify_installation(install_dir, os_name):
        log_error("安装验证失败")
        return 1

    # 6. 输出环境变量设置
    setup_environment(install_dir, os_name)

    log_success("LLVM安装完成!")
    return 0


if __name__ == "__main__":
    sys.exit(main())
